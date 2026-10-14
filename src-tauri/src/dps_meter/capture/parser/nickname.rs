use super::context::ParserContext;
use super::utils::{find_bytes, last_index_of, read_u32_le, read_u64_le, read_varint};

const COMBAT_POWER_MARKER: [u8; 3] = [0xF4, 0xCB, 0x1F];

pub(crate) fn parse_main(
    context: &ParserContext<'_>,
    payload: &[u8],
    _is_compressed_bundle: bool,
) -> bool {
    parse_main_fixed(context, payload)
}

pub(crate) fn parse_main_combat_power(
    context: &ParserContext<'_>,
    payload: &[u8],
    _is_compressed_bundle: bool,
) -> bool {
    let Some(combat_power) = own_combat_power_update(payload) else {
        return false;
    };
    if !context
        .data_storage
        .save_main_actor_combat_power(combat_power)
    {
        return false;
    }

    context.logger.info(format!(
        "[{}] main actor combat power combat_power={combat_power}",
        context.port
    ));
    true
}

// 33 36 <actor_id> <five metadata bytes> <name_len> <name> <sid>
fn parse_main_fixed(context: &ParserContext<'_>, payload: &[u8]) -> bool {
    let Some(profile) = decode_main_profile(payload) else { return false; };
    let actor_id = profile.actor_id;
    let name = profile.name;
    let server_id = profile.server;
    let job = profile.job;
    let actor_class = job.and_then(job_to_actor_class);
    crate::dps_meter::region::observe_server_id(server_id);
    context.data_storage.append_actor(actor_id, &name, Some(&server_id.to_string()));
    if let Some(class) = actor_class { context.data_storage.set_actor_class(actor_id, class); }
    context.data_storage.set_main_actor(actor_id, &name);
    context.data_storage.upsert_party_member(actor_id, &name, actor_class);
    context.logger.info(format!("[{}] own profile actor={} name={} sid={} class={}", context.port,
        actor_id, name, server_id, actor_class.unwrap_or("none")));
    if let Some(cp) = profile_combat_power(payload) { context.data_storage.save_main_actor_combat_power(cp); }
    true
}

pub(crate) struct MainProfile {
    pub actor_id: u32,
    pub name: String,
    pub server: u32,
    pub job: Option<u8>,
}

pub(crate) fn decode_main_profile(payload: &[u8]) -> Option<MainProfile> {
    if payload.get(..2)? != [0x33, 0x36] { return None; }
    let actor = read_varint(payload, 2);
    if !actor.is_valid() || actor.value <= 0 { return None; }
    let start = 2 + actor.length;
    if let Some(profile) = decode_main_profile_at(payload, actor.value as u32, start + 5) { return Some(profile); }
    // Global profile metadata has varied in width. Search only the short header,
    // requiring an exact UTF-8 nickname, server and recognized job for fallback layouts.
    let mut found = None;
    for metadata_bytes in 0..=12 {
        if metadata_bytes == 5 { continue; }
        if let Some(profile) = decode_main_profile_at(payload, actor.value as u32, start + metadata_bytes) {
            if profile.job.and_then(job_to_actor_class).is_none() { continue; }
            if found.is_some() { return None; } // ambiguous headers are not identities
            found = Some(profile);
        }
    }
    found
}

fn decode_main_profile_at(payload: &[u8], actor_id: u32, at: usize) -> Option<MainProfile> {
    let len = usize::from(*payload.get(at)?);
    if !(1..=36).contains(&len) { return None; }
    let start = at + 1;
    let end = start + len;
    let raw = std::str::from_utf8(payload.get(start..end)?).ok()?;
    let name = sanitize_nickname(raw)?;
    if name != raw { return None; }
    let server_bytes = payload.get(end..end + 2)?;
    let server = u32::from(u16::from_le_bytes([server_bytes[0],server_bytes[1]]));
    if !is_available_server_id(server) { return None; }
    Some(MainProfile { actor_id, name, server, job: payload.get(end + 2).copied() })
}


const MAX_COMBAT_POWER: u64 = 10_000_000;

/// `56 36` + two u64 LE (18 bytes): the current combat power and a second value that is never below it
/// (captures 07.10 and 10.10: 36947/46099 -> 46099/46099, 59292/75697 -> 75567/75697 during a login).
/// The game also sends a 10-byte `56 36` with an unrelated payload — that one is rejected by the length.
fn own_combat_power_update(payload: &[u8]) -> Option<u64> {
    if payload.len() != 18 || payload[0] != 0x56 || payload[1] != 0x36 {
        return None;
    }
    plausible_power_pair(read_u64_le(payload, 2)?, read_u64_le(payload, 10)?)
}

/// The pair is (current, settled). At login and after a zone change the game sends a ladder whose first value climbs
/// to the settled one within a fraction of a second (10.10: 59 292 → 64 816 → 75 567, settled 75 697 in every step).
/// Owner 12.10: after a teleport the later steps were lost in the following data burst and the own power stuck at the
/// first step (59 359 instead of 75 637) — so while the first value is still far below the settled one, the settled
/// one is the answer.
fn plausible_power_pair(power: u64, second: u64) -> Option<u64> {
    if !((1..=MAX_COMBAT_POWER).contains(&power) && (1..=MAX_COMBAT_POWER).contains(&second) && second >= power) { return None; }
    Some(if power * 100 >= second * 97 { power } else { second })
}

/// The own profile `33 36` carries the same pair right before two stat blocks, each `<n> (<k> <u16>){n}` with k = 1..n
/// (every full profile in the 06.10–10.10 captures: the pair sits 64 bytes before the end). Searched from the end and
/// accepted only together with both blocks, so stray numbers inside the profile cannot pass.
fn profile_combat_power(payload: &[u8]) -> Option<u64> {
    if payload.len() < 2 + 16 + 2 || payload[0] != 0x33 || payload[1] != 0x36 {
        return None;
    }
    (2..=payload.len() - 16).rev().find_map(|at| {
        let power = plausible_power_pair(read_u64_le(payload, at)?, read_u64_le(payload, at + 8)?)?;
        let first = stat_block_end(payload, at + 16)?;
        stat_block_end(payload, first)?;
        Some(power)
    })
}

fn stat_block_end(payload: &[u8], at: usize) -> Option<usize> {
    let count = usize::from(*payload.get(at)?);
    let end = at + 1 + count * 3;
    if !(1..=10).contains(&count) || end > payload.len() {
        return None;
    }
    (0..count).all(|k| usize::from(payload[at + 1 + k * 3]) == k + 1).then_some(end)
}

pub(crate) fn parse_other(
    context: &ParserContext<'_>,
    payload: &[u8],
    _is_compressed_bundle: bool,
) -> bool {
    let Some(player) = decode_other_player(payload) else { return false; };
    let actor_id = player.actor_id;
    let actor_name = player.name;
    let job = player.job;
    let actor_class = player.class;
    let server_id = player.server_id;
    let combat_power = player.combat_power;

    let server_id_string = server_id.map(|value| value.to_string());
    let server_id_text = server_id_string.as_deref().unwrap_or("none");
    context
        .data_storage
        .append_actor(actor_id, &actor_name, server_id_string.as_deref());
    if let Some(actor_class) = actor_class {
        context.data_storage.set_actor_class(actor_id, actor_class);
    }
    if let Some(combat_power) = combat_power {
        context
            .data_storage
            .set_actor_combat_power(actor_id, combat_power);
    }
    context.data_storage.upsert_nearby_player(actor_id, server_id, actor_class, combat_power);
    // 45 36 is every visible player (138 names in a town capture, 07.10), NOT the party: it must not
    // touch the roster. The real party packet is still unknown; until it is found the party is self only.
    context.logger.info(format!(
        "[{}] actor actor={} name={} sid={} job={} class={} combat_power={}",
        context.port,
        actor_id,
        actor_name,
        server_id_text,
        job,
        actor_class.unwrap_or("none"),
        combat_power
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_string())
    ));

    true
}

struct OtherPlayer {
    actor_id: u32,
    name: String,
    job: u8,
    class: Option<&'static str>,
    server_id: Option<u32>,
    combat_power: Option<u64>,
}

fn decode_other_player(payload: &[u8]) -> Option<OtherPlayer> {
    if payload.get(..2)? != [0x45, 0x36] { return None; }
    let actor_id_info = read_varint(payload, 2);
    if !actor_id_info.is_valid() || actor_id_info.value <= 0 { return None; }
    let actor_id = actor_id_info.value as u32;
    let mut offset = 2 + actor_id_info.length;
    if payload.len() <= offset {
        return None;
    }

    let unknown_info_1 = read_varint(payload, offset);
    if !unknown_info_1.is_valid() {
        return None;
    }
    offset += unknown_info_1.length;
    if payload.len() <= offset {
        return None;
    }

    let unknown_info_2 = read_varint(payload, offset);
    if !unknown_info_2.is_valid() {
        return None;
    }
    offset += unknown_info_2.length;
    if payload.len().saturating_sub(offset) <= 2 {
        return None;
    }

    offset += 1;
    let base = offset;
    let mut best_actor_name = None;
    let mut best_actor_name_end = None;
    let mut best_actor_name_bytes = 0usize;

    for relative in 0..5usize {
        let name_offset = base + relative;
        if name_offset >= payload.len() {
            continue;
        }

        let name_length_info = read_varint(payload, name_offset);
        if !name_length_info.is_valid() {
            continue;
        }

        let candidate_length = name_length_info.value as usize;
        if !(1..=71).contains(&candidate_length) {
            continue;
        }

        let value_start = name_offset + name_length_info.length;
        let value_end = value_start + candidate_length;
        if value_end > payload.len() {
            continue;
        }

        let Ok(candidate_name) = std::str::from_utf8(&payload[value_start..value_end]) else {
            continue;
        };
        let Some(sanitized_name) = sanitize_nickname(candidate_name) else {
            continue;
        };

        let sanitized_bytes = sanitized_name.len();
        if sanitized_bytes > best_actor_name_bytes {
            best_actor_name_bytes = sanitized_bytes;
            best_actor_name = Some(sanitized_name);
            best_actor_name_end = Some(value_end);
        }
    }

    let actor_name = best_actor_name?;
    let actor_name_end = best_actor_name_end?;
    if actor_name_end >= payload.len() {
        return None;
    }

    let job = payload[actor_name_end];
    let actor_class = job_to_actor_class(job);
    let server_base = actor_name_end + 1;
    let server_id = find_server_id(payload, server_base);
    let combat_power = parse_snapshot_combat_power(payload);

    Some(OtherPlayer { actor_id, name: actor_name, job, class: actor_class, server_id, combat_power })
}

/// Only handle actions whose semantics are established. `45 37` currently
/// has no confirmed party meaning, so it must not remove a roster member.
pub(crate) fn parse_party_action(context: &ParserContext<'_>, payload: &[u8]) -> bool {
    match decode_party_action(payload) {
        Some(PartyAction::Disband) => {
            context.data_storage.clear_party();
            true
        }
        None => false,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PartyAction {
    Disband,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PartyPacket {
    UnconfirmedAction(u32),
    Disband,
}

fn decode_party_packet(payload: &[u8]) -> Option<PartyPacket> {
    if payload.len() < 2 || payload[0] != 0x45 { return None; }
    match payload[1] {
        0x37 => {
            let actor = read_varint(payload, 2);
            if !actor.is_valid() || actor.value <= 0 { return None; }
            let actor_id = actor.value as u32;
            Some(PartyPacket::UnconfirmedAction(actor_id))
        }
        0x38 => Some(PartyPacket::Disband),
        _ => None,
    }
}

fn decode_party_action(payload: &[u8]) -> Option<PartyAction> {
    match decode_party_packet(payload)? {
        PartyPacket::Disband => Some(PartyAction::Disband),
        PartyPacket::UnconfirmedAction(_) => None,
    }
}

fn parse_snapshot_combat_power(packet: &[u8]) -> Option<u64> {
    let marker_index = last_index_of(packet, &COMBAT_POWER_MARKER)?;
    let mut offset = marker_index + 11;

    while offset + 8 <= packet.len() {
        let combat_power = u64::from(read_u32_le(packet, offset)?);
        let trailing_zero = read_u32_le(packet, offset + 4)? == 0;
        if (1..=10_000_000).contains(&combat_power) && trailing_zero {
            return Some(combat_power);
        }
        offset += 1;
    }

    None
}

fn job_to_actor_class(job: u8) -> Option<&'static str> {
    match job {
        5..=8 => Some("GLADIATOR"),
        9..=12 => Some("TEMPLAR"),
        13..=16 => Some("RANGER"),
        17..=20 => Some("ASSASSIN"),
        21..=24 => Some("ELEMENTALIST"),
        25..=28 => Some("SORCERER"),
        29..=32 => Some("CLERIC"),
        33..=36 => Some("CHANTER"),
        45..=48 => Some("FIGHTER"),
        _ => None,
    }
}

fn find_server_id(payload: &[u8], server_base: usize) -> Option<u32> {
    let mut relative = 0usize;
    let mut fallback_server_id = None;

    loop {
        let offset = server_base + relative;
        relative += 1;
        if offset + 2 > payload.len() {
            break;
        }

        let server_id = u32::from(u16::from_le_bytes([payload[offset], payload[offset + 1]]));
        if !is_available_server_id(server_id) {
            continue;
        }
        if fallback_server_id.is_none() {
            fallback_server_id = Some(server_id);
        }

        let legion_length_offset = offset + 2;
        if legion_length_offset >= payload.len() {
            continue;
        }
        let legion_length_info = read_varint(payload, legion_length_offset);
        if !legion_length_info.is_valid() {
            continue;
        }
        let legion_length = legion_length_info.value as usize;
        if legion_length > 24 {
            continue;
        }

        let legion_start = legion_length_offset + legion_length_info.length;
        let legion_end = legion_start + legion_length;
        if legion_end > payload.len() {
            continue;
        }
        if legion_length == 0 {
            return Some(server_id);
        }

        let Ok(legion_name) = std::str::from_utf8(&payload[legion_start..legion_end]) else {
            continue;
        };
        if legion_name.trim().is_empty() || legion_name.chars().any(|ch| !ch.is_ascii_digit()) {
            return Some(server_id);
        }
    }

    fallback_server_id.or_else(|| find_server_id_after_marker(payload, server_base))
}

fn is_available_server_id(server_id: u32) -> bool {
    // Was Taiwan's catalogue hardcoded here, which silently rejected every
    // player packet on any other service. See dps_meter::region.
    crate::dps_meter::region::accepts_server_id(server_id)
}

fn find_server_id_after_marker(payload: &[u8], search_start: usize) -> Option<u32> {
    let search_end = payload.len().saturating_sub(1).min(search_start + 200);
    let mut position = search_start;

    while position < search_end {
        let Some(index) = find_bytes(payload, position, &[0x11, 0x11]) else {
            break;
        };
        if index < 4 {
            break;
        }
        if payload[index - 4] == 0x00 && payload[index - 3] == 0x02 {
            let server_id = u32::from(u16::from_le_bytes([payload[index - 3], payload[index - 2]]));
            if is_available_server_id(server_id) {
                return Some(server_id);
            }
        }
        position = index + 2;
    }

    None
}

fn sanitize_nickname(nickname: &str) -> Option<String> {
    let sanitized = nickname.split('\0').next().unwrap_or_default().trim();
    if sanitized.is_empty() {
        return None;
    }

    let mut result = String::new();
    let mut only_numbers = true;
    for character in sanitized.chars() {
        let code = character as u32;
        if code < 32 || code == 127 || (0x80..=0x9F).contains(&code) || character == '\u{FFFD}' {
            continue;
        }

        let is_han = is_han_character(character);
        if character.is_alphanumeric() || is_han {
            result.push(character);
            if character.is_alphabetic() || is_han {
                only_numbers = false;
            }
        }
    }

    if result.is_empty() || only_numbers {
        return None;
    }
    if result.chars().count() == 1 {
        let character = result.chars().next()?;
        if !is_han_character(character) && !is_hangul_syllable(character) {
            return None;
        }
    }

    Some(result)
}

fn is_hangul_syllable(character: char) -> bool {
    matches!(character as u32, 0xAC00..=0xD7A3)
}

fn is_han_character(character: char) -> bool {
    let code = character as u32;
    matches!(
        code,
        0x3400..=0x4DBF
            | 0x4E00..=0x9FFF
            | 0xF900..=0xFAFF
            | 0x20000..=0x2A6DF
            | 0x2A700..=0x2B73F
            | 0x2B740..=0x2B81F
            | 0x2B820..=0x2CEAF
            | 0x2CEB0..=0x2EBEF
            | 0x2F800..=0x2FA1F
            | 0x30000..=0x3134F
            | 0x31350..=0x323AF
    )
}

#[cfg(test)]
mod tests {
    use super::{decode_other_player, decode_party_action, decode_party_packet, sanitize_nickname, PartyAction, PartyPacket};

    fn hex(text: &str) -> Vec<u8> {
        (0..text.len()).step_by(2).map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap()).collect()
    }

    #[test]
    fn own_profile_supports_variable_metadata_width_without_guessing_another_player() {
        use super::decode_main_profile;
        let profile = decode_main_profile(&hex("3336B20C5E81C128370472776E64FE0810000000022D000000FF050000FF0500002D000000")).unwrap();
        assert_eq!((profile.actor_id, profile.name.as_str(), profile.server, profile.job), (1586, "rwnd", 2302, Some(16)));
        let shorter = decode_main_profile(&hex("3336B20C5E81C1280472776E64FE0810000000")).unwrap();
        assert_eq!((shorter.actor_id, shorter.name.as_str(), shorter.server), (1586, "rwnd", 2302));
        assert!(decode_main_profile(&hex("4536B20C5E81C1280472776E64FE0810")).is_none());
        assert!(decode_main_profile(&hex("3336B20C5E81C1280472776E64FE")).is_none());
        assert!(decode_main_profile(&hex("3336B20C5E81C12804FFFFFFFFFE0810")).is_none());
    }

    #[test]
    fn own_combat_power_update_reads_the_login_ladder_and_rejects_other_56_36() {
        use super::own_combat_power_update;
        // owner login 10.10 (rwnd, Global): three updates within 0.2 s, the last one is the real value; the early steps
        // already answer with the settled value (12.10: a lost tail must not leave the first step on screen)
        assert_eq!(own_combat_power_update(&hex("56369ce7000000000000b127010000000000")), Some(75_697));
        assert_eq!(own_combat_power_update(&hex("563630fd000000000000b127010000000000")), Some(75_697));
        assert_eq!(own_combat_power_update(&hex("56362f27010000000000b127010000000000")), Some(75_567));
        // 07.10: equal pair
        assert_eq!(own_combat_power_update(&hex("563613b400000000000013b4000000000000")), Some(46_099));
        // the 10-byte 56 36 is another message: its bytes used to be read as power
        assert_eq!(own_combat_power_update(&hex("5636b7673a7b915ad2f9")), None);
        // truncated, padded, second value below the first, zero, absurd
        assert_eq!(own_combat_power_update(&hex("56362f27010000000000b1270100")), None);
        assert_eq!(own_combat_power_update(&hex("56362f27010000000000b12701000000000000")), None);
        assert_eq!(own_combat_power_update(&hex("5636b127010000000000b11f010000000000")), None);
        assert_eq!(own_combat_power_update(&hex("56360000000000000000b127010000000000")), None);
        assert_eq!(own_combat_power_update(&hex("5636ffffffff00000000ffffffff00000000")), None);
    }

    #[test]
    fn profile_combat_power_is_found_before_the_two_stat_blocks() {
        use super::profile_combat_power;
        // the real tails of the own 33 36 profile (10.10 and 07.10) behind a filler that also contains a decoy pair
        let mut packet = hex("3336");
        packet.extend(hex("2f27010000000000b127010000000000"));
        packet.extend(vec![0x11; 300]);
        packet.extend(hex("000000000000000000000000000000002f27010000000000b12701000000000005014c010200000300000400000500000501c60002000003030004000005000002010000000700000002000000010000"));
        assert_eq!(profile_combat_power(&packet), Some(75_567));
        let mut packet = hex("3336");
        packet.extend(hex("0000000000000000000000000000000013b400000000000013b40000000000000501af000200000300000400000500000501340002000003000004000005000002010000000100000002000000010000"));
        assert_eq!(profile_combat_power(&packet), Some(46_099));
        // a pair without the stat blocks behind it is not a profile power; neither is a short 33 36
        let mut packet = hex("3336");
        packet.extend(hex("2f27010000000000b127010000000000"));
        packet.extend(vec![0x07; 48]);
        assert_eq!(profile_combat_power(&packet), None);
        assert_eq!(profile_combat_power(&hex("3336a1b2c3d4e5f60708")), None);
    }

    #[test]
    fn captured_town_samples_decode_name_and_job_class() {
        let fixture = include_str!("../../../../tests/fixtures/45-36-town.hex");
        let expected = [
            ("Sempialia", Some("CHANTER")), ("Smolsaka", Some("SORCERER")), ("Connnnix", Some("SORCERER")),
            ("Mikemon", Some("TEMPLAR")), ("Sonofman", Some("CHANTER")), ("fhdgs", Some("TEMPLAR")),
            ("Анджелина", Some("CHANTER")), ("辣条哟i", Some("RANGER")),
        ];
        let rows: Vec<_> = fixture.lines().filter(|line| !line.starts_with('#')).collect();
        assert_eq!(rows.len(), expected.len());
        for (line, (name, class)) in rows.into_iter().zip(expected) {
            let bytes: Vec<u8> = line.split_whitespace().map(|byte| u8::from_str_radix(byte, 16).unwrap()).collect();
            let player = decode_other_player(&bytes).expect("captured 45 36 player");
            assert!(player.actor_id > 0);
            assert_eq!(player.name, name);
            assert_eq!(player.class, class);
        }
    }

    #[test]
    fn visible_player_packet_is_not_a_party_action() {
        assert_eq!(decode_party_action(&[0x45, 0x36, 42]), None);
        assert_eq!(decode_party_packet(&[0x45, 0x36, 42]), None);
    }

    #[test]
    fn unconfirmed_45_37_does_not_remove_party_members() {
        assert_eq!(decode_party_action(&[0x45, 0x37, 0xAC, 0x02]), None);
        assert_eq!(decode_party_action(&[0x45, 0x38]), Some(PartyAction::Disband));
        assert_eq!(decode_party_packet(&[0x45, 0x37, 0xAC, 0x02]), Some(PartyPacket::UnconfirmedAction(300)));
        assert_eq!(decode_party_packet(&[0x45, 0x38]), Some(PartyPacket::Disband));
        assert_eq!(decode_party_action(&[0x45, 0x37, 0x80]), None);
        assert_eq!(decode_party_action(&[0x46, 0x37, 1]), None);
    }

    #[test]
    fn sanitize_nickname_keeps_single_cjk_extension_character() {
        assert_eq!(sanitize_nickname("𠮷"), Some("𠮷".to_string()));
        assert_eq!(sanitize_nickname("𫠜"), Some("𫠜".to_string()));
    }

    #[test]
    fn sanitize_nickname_keeps_single_hangul_syllable() {
        for name in ["범", "가", "힣"] {
            assert_eq!(sanitize_nickname(name), Some(name.to_string()));
        }
    }

    #[test]
    fn own_profile_keeps_single_hangul_name_with_legacy_and_fallback_headers() {
        // Same nickname/name-length support as Nania GPL; keep the late profile decoder.
        for metadata_bytes in [5usize, 7] {
            let mut packet = vec![0x33, 0x36, 0x82, 0x4A]; // actor 9474
            packet.extend(std::iter::repeat_n(0, metadata_bytes));
            packet.push(3); // UTF-8 byte length, not character count
            packet.extend("범".as_bytes());
            packet.extend(2302u16.to_le_bytes());
            packet.push(13); // Ranger
            let profile = super::decode_main_profile(&packet).unwrap();
            assert_eq!(profile.actor_id, 9474);
            assert_eq!(profile.name, "범");
            assert_eq!(profile.server, 2302);
            assert_eq!(super::job_to_actor_class(profile.job.unwrap()), Some("RANGER"));
        }
    }

    #[test]
    fn sanitize_nickname_still_rejects_single_ascii_letter() {
        assert_eq!(sanitize_nickname("A"), None);
    }
}
