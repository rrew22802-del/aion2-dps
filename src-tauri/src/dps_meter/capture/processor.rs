use lz4_flex::block::decompress;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::dps_meter::capture::parser::context::ParserContext;
use crate::dps_meter::capture::parser::field_boss_timer;
use crate::dps_meter::capture::parser::nickname;
use crate::dps_meter::capture::parser::utils::read_varint;
use crate::dps_meter::config::SharedDpsMeterConfig;
use crate::dps_meter::capture::census;
use crate::dps_meter::storage::data_storage::DataStorage;
use crate::plugins::logger::AppLogger;

const KNOWN_PACKET_HEADERS: &[(u8, u8)] = &[
    (0x33, 0x36),
    (0x45, 0x36),
    (0x45, 0x37),
    (0x45, 0x38),
    (0x56, 0x36),
    (0x41, 0x36),
    (0x42, 0x36),
    (0x04, 0x38),
    (0x05, 0x38),
    (0x2A, 0x38),
    (0x2B, 0x38),
    (0x04, 0x8D),
    (0x00, 0x8D),
    // Field boss timers are parsed in full mode and remain known to stall resync/census.
    (0x01, 0x91),
    (0xFF, 0xFF),
];

/// Packet prefix metadata.
///
/// An optional extension byte in the 0xF0..0xFE range may appear after the
/// length varint. It is not part of the business opcode.
#[derive(Debug, Clone, Copy)]
struct PacketPrefixInfo {
    /// Offset of the business payload and opcode.
    payload_offset: usize,
}

/// Resolve the transport prefix following the length field.
///
/// Supported layouts:
/// 1. [length varint][opcode...]
/// 2. [length varint][extraFlag][opcode...]
fn resolve_packet_prefix(packet: &[u8], length_offset: usize) -> Option<PacketPrefixInfo> {
    let first_byte = *packet.get(length_offset)?;
    let has_extra_flag = (0xF0..0xFF).contains(&first_byte);
    let payload_offset = length_offset + if has_extra_flag { 1 } else { 0 };

    if payload_offset >= packet.len() {
        return None;
    }

    Some(PacketPrefixInfo { payload_offset })
}

fn inspect_stall_candidate(buffer: &[u8]) -> StallCandidate {
    let length_info = read_varint(buffer, 0);
    let declared_packet_len = if length_info.is_valid() && length_info.value > 0 {
        transport_packet_size(length_info.value, length_info.length)
    } else {
        None
    };
    let opcode = if length_info.is_valid() {
        resolve_packet_prefix(buffer, length_info.length).and_then(|prefix| {
            Some((
                *buffer.get(prefix.payload_offset)?,
                *buffer.get(prefix.payload_offset + 1)?,
            ))
        })
    } else {
        None
    };

    StallCandidate {
        declared_packet_len,
        opcode,
        buffered: buffer.len(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProcessorMode {
    Full,
    NicknameOnly,
}

#[derive(Debug, Clone, Copy)]
enum StallResyncMode {
    Immediate,
    Delayed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct StallCandidate {
    declared_packet_len: Option<usize>,
    opcode: Option<(u8, u8)>,
    /// owner 12.10: bytes already here — while the tail keeps arriving the candidate changes and the wait starts over,
    /// so a packet still being written (1 818 → 2 965 of 4 244) is not dropped after a fixed delay
    buffered: usize,
}

pub struct StreamProcessor {
    pub(super) data_storage: Arc<DataStorage>,
    pub(super) logger: Arc<AppLogger>,
    pub(super) port: String,
    pub(super) config: SharedDpsMeterConfig,
    mode: ProcessorMode,
    stall_resync_mode: StallResyncMode,
    stalled_since: Option<Instant>,
    stalled_candidate: Option<StallCandidate>,
    combat_damage_enabled: bool,
    /// Periodic pool notices per stream (strict `05 38` rules).
    pub(super) periodic_pools: crate::dps_meter::capture::parser::reference::PeriodicPools,
}

impl StreamProcessor {
    pub fn new(
        data_storage: Arc<DataStorage>,
        logger: Arc<AppLogger>,
        port: String,
        config: SharedDpsMeterConfig,
    ) -> Self {
        Self {
            data_storage,
            logger,
            port,
            config,
            mode: ProcessorMode::Full,
            stall_resync_mode: StallResyncMode::Immediate,
            stalled_since: None,
            stalled_candidate: None,
            combat_damage_enabled: false,
            periodic_pools: Default::default(),
        }
    }

    pub fn new_nickname_only(
        data_storage: Arc<DataStorage>,
        logger: Arc<AppLogger>,
        port: String,
        config: SharedDpsMeterConfig,
    ) -> Self {
        Self {
            data_storage,
            logger,
            port,
            config,
            mode: ProcessorMode::NicknameOnly,
            stall_resync_mode: StallResyncMode::Delayed,
            stalled_since: None,
            stalled_candidate: None,
            combat_damage_enabled: false,
            periodic_pools: Default::default(),
        }
    }

    pub fn set_combat_damage_enabled(&mut self, enabled: bool) {
        self.combat_damage_enabled = enabled;
    }

    pub fn consume_stream(&mut self, buffer: &[u8]) -> usize {
        let mut offset = 0usize;

        while offset < buffer.len() {
            if buffer[offset] == 0x00 {
                offset += 1;
                continue;
            }

            let length_info = read_varint(buffer, offset);
            if !length_info.is_valid() || length_info.value <= 0 {
                if offset + 5 > buffer.len() {
                    break;
                }
                offset += 1;
                continue;
            }

            let Some(total_packet_bytes) = transport_packet_size(length_info.value, length_info.length)
            else {
                offset += 1;
                continue;
            };

            if total_packet_bytes == 0 || total_packet_bytes > 65_535 {
                offset += 1;
                continue;
            }

            if offset + total_packet_bytes > buffer.len() {
                // TCP may split any legal frame. Wait for its tail rather than
                // treating embedded payload bytes as fresh length fields.
                break;
            }

            let current_packet = &buffer[offset..offset + total_packet_bytes];
            let Some(prefix_info) = resolve_packet_prefix(current_packet, length_info.length)
            else {
                offset += 1;
                continue;
            };
            let payload_start = prefix_info.payload_offset;
            let is_bundle = payload_start + 1 < current_packet.len()
                && current_packet[payload_start] == 0xFF
                && current_packet[payload_start + 1] == 0xFF;

            if is_bundle {
                let bundle_size = total_packet_bytes;
                if offset + bundle_size > buffer.len() {
                    break;
                }
                self.unwrap_bundle(&buffer[offset + payload_start..offset + bundle_size]);
                offset += bundle_size;
            } else {
                self.parse_packet(current_packet, false);
                offset += total_packet_bytes;
            }
        }

        // if buffer.len() >= 4 {
        //     self.scan_for_embedded_048d(buffer);
        // }

        if offset == 0 && !buffer.is_empty() {
            let candidate: StallCandidate = inspect_stall_candidate(buffer);
            let is_known_or_undetermined = candidate
                .opcode
                .is_none_or(|opcode| KNOWN_PACKET_HEADERS.contains(&opcode));
            let delay_ms = if is_known_or_undetermined {
                let config = self.config.read().unwrap();
                match self.stall_resync_mode {
                    StallResyncMode::Immediate => config.full_processor_stall_resync_delay_ms,
                    StallResyncMode::Delayed => config.stall_resync_delay_ms,
                }
            } else {
                self.config
                    .read()
                    .unwrap()
                    .unknown_packet_stall_resync_delay_ms
            };

            if delay_ms == 0 {
                self.stalled_since = None;
                self.stalled_candidate = None;
                
                self.logger.info(format!(
                    "[{}] stream stalled for {}ms with buffer_size={} declared_packet_len={:?} opcode={} known_or_undetermined={}, forcing resync by skipping 1 byte",
                    self.port,
                    delay_ms,
                    buffer.len(),
                    candidate.declared_packet_len,
                    candidate
                        .opcode
                        .map(|(first, second)| format!("{first:02X}{second:02X}"))
                        .unwrap_or_else(|| "undetermined".to_string()),
                    is_known_or_undetermined
                ));

                return 1;
            }

            let now = Instant::now();
            if self.stalled_candidate != Some(candidate) {
                self.stalled_candidate = Some(candidate);
                self.stalled_since = Some(now);
                return 0;
            }

            if let Some(stalled_since) = self.stalled_since {
                if now.duration_since(stalled_since) >= Duration::from_millis(delay_ms) {
                    self.logger.info(format!(
                        "[{}] stream stalled for {}ms with buffer_size={} declared_packet_len={:?} opcode={} known_or_undetermined={}, forcing resync by skipping 1 byte",
                        self.port,
                        delay_ms,
                        buffer.len(),
                        candidate.declared_packet_len,
                        candidate
                            .opcode
                            .map(|(first, second)| format!("{first:02X}{second:02X}"))
                            .unwrap_or_else(|| "undetermined".to_string()),
                        is_known_or_undetermined
                    ));
                    self.stalled_since = Some(now);
                    return 1;
                }
            } else {
                self.stalled_since = Some(now);
            }
        } else {
            self.stalled_since = None;
            self.stalled_candidate = None;
        }

        offset
    }

    fn unwrap_bundle(&mut self, payload: &[u8]) {
        visit_bundle_packets(payload, 0, &mut |packet| { self.parse_packet(packet, true); });
    }

    fn parse_packet(&mut self, packet: &[u8], is_compressed_bundle: bool) -> bool {
        if packet.len() < 3 {
            return false;
        }

        let packet_length_info = read_varint(packet, 0);
        if !packet_length_info.is_valid() {
            return false;
        }

        let Some(prefix_info) = resolve_packet_prefix(packet, packet_length_info.length) else {
            return false;
        };

        let payload = &packet[prefix_info.payload_offset..];
        if payload.len() < 2 {
            return false;
        }

        // Tallied before dispatch, so unrecognised opcodes are counted too --
        // those are the ones worth knowing about on an unfamiliar service.
        let opcode = (payload[0], payload[1]);
        census::observe(opcode, payload.len(), KNOWN_PACKET_HEADERS.contains(&opcode));

        match self.mode {
            ProcessorMode::Full => match (payload[0], payload[1]) {
                (0x33, 0x36) => {
                    nickname::parse_main(&self.parser_context(), payload, is_compressed_bundle)
                }
                (0x45, 0x36) => {
                    nickname::parse_other(&self.parser_context(), payload, is_compressed_bundle)
                }
                (0x45, 0x37) | (0x45, 0x38) => {
                    nickname::parse_party_action(&self.parser_context(), payload)
                }
                (0x56, 0x36) => nickname::parse_main_combat_power(
                    &self.parser_context(),
                    payload,
                    is_compressed_bundle,
                ),
                (0x41, 0x36) => self.parse_summon_packet(payload, is_compressed_bundle),
                (0x42, 0x36) => {
                    if let Some(dead) = death_entity(payload) {
                        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_secs_f64()).unwrap_or_default();
                        self.data_storage.mark_target_dead(dead, now);
                        true
                    } else { false }
                },
                (0x04, 0x38) if self.combat_damage_enabled => {
                    self.parse_damage_packet(payload, is_compressed_bundle)
                }
                (0x05, 0x38) if self.combat_damage_enabled => {
                    self.parse_dot_packet(payload, is_compressed_bundle)
                }
                (0x2A, 0x38) | (0x2B, 0x38) => {
                    self.parse_buff_packet(payload, is_compressed_bundle)
                }
                (0x04, 0x8D) => self.parse_summon_packet_048d(payload, is_compressed_bundle),
                (0x00, 0x8D) => {
                    // only the "01 01" form right after the entity is about our own character (HP updates of mobs and other
                    // players use "02 01"); counting every 00 8D kept our share under 60 % in fights, so late attach never found us
                    let actor = read_varint(payload, 2);
                    let at = 2 + actor.length as usize;
                    let own_form = actor.is_valid() && actor.value > 0 && payload.get(at) == Some(&1) && payload.get(at + 1) == Some(&1);
                    let before = self.data_storage.main_actor_id();
                    if own_form {
                        self.data_storage.observe_self_packet(actor.value as u32);
                    }
                    // diagnostics while we still do not know our character: how many 00 8D arrive, how many in the own form
                    if before.is_none() {
                        use std::sync::atomic::{AtomicU32, Ordering};
                        static SEEN: AtomicU32 = AtomicU32::new(0);
                        static OWN: AtomicU32 = AtomicU32::new(0);
                        let seen = SEEN.fetch_add(1, Ordering::Relaxed) + 1;
                        let own = if own_form { OWN.fetch_add(1, Ordering::Relaxed) + 1 } else { OWN.load(Ordering::Relaxed) };
                        if let Some(found) = self.data_storage.main_actor_id() {
                            self.logger.info(format!("[{}] main actor inferred from 00 8D actor={found} after {seen} packets ({own} own-form)", self.port));
                        } else if seen % 50 == 0 {
                            self.logger.info(format!("[{}] self fallback waiting: 00 8D seen={seen} own-form={own} last actor={} bytes={:02x?}",
                                self.port, actor.value, &payload[..payload.len().min(12)]));
                        }
                    }
                    self.parse_remain_hp_packet(payload, is_compressed_bundle)
                }
                (0x01, 0x91) => field_boss_timer::parse_packet(&self.parser_context(), payload),
                _ => false,
            },
            ProcessorMode::NicknameOnly => match (payload[0], payload[1]) {
                (0x33, 0x36) => {
                    nickname::parse_main(&self.parser_context(), payload, is_compressed_bundle)
                }
                (0x45, 0x36) => {
                    nickname::parse_other(&self.parser_context(), payload, is_compressed_bundle)
                }
                (0x45, 0x37) | (0x45, 0x38) => {
                    nickname::parse_party_action(&self.parser_context(), payload)
                }
                (0x56, 0x36) => nickname::parse_main_combat_power(
                    &self.parser_context(),
                    payload,
                    is_compressed_bundle,
                ),
                (0x41, 0x36) => self.parse_summon_packet(payload, is_compressed_bundle),
                _ => false,
            },
        }
    }

    fn parser_context(&self) -> ParserContext<'_> {
        ParserContext::new(&self.data_storage, &self.logger, &self.port)
    }
}

fn death_entity(payload: &[u8]) -> Option<u32> {
    if payload.get(..2)? != [0x42, 0x36] { return None; }
    let dead=read_varint(payload,2);
    let killer=read_varint(payload,2+dead.length);
    let flag=read_varint(payload,2+dead.length+killer.length);
    (dead.is_valid() && killer.is_valid() && flag.is_valid() && dead.value>0)
        .then(||u32::try_from(dead.value).ok()).flatten()
}

// Transport length includes four virtual bytes, independently of varint width.
fn transport_packet_size(value: i64, width: usize) -> Option<usize> {
    usize::try_from(value.checked_add(i64::try_from(width).ok()?)?.checked_sub(4)?).ok().filter(|size| *size >= 3 && *size <= 65_535)
}

fn visit_bundle_packets(payload: &[u8], depth: usize, visit: &mut impl FnMut(&[u8])) {
    if payload.len() < 7 || depth >= 8 { return; }
    let expected = u32::from_le_bytes(payload[2..6].try_into().unwrap()) as usize;
    if expected == 0 || expected > 5_000_000 { return; }
    let Ok(data) = decompress(&payload[6..], expected) else { return; };
    let mut at = 0;
    while at < data.len() {
        if data[at] == 0 { at += 1; continue; }
        let length = read_varint(&data, at);
        if !length.is_valid() { break; }
        let Some(size) = transport_packet_size(length.value, length.length) else { break; };
        let Some(packet) = data.get(at..at + size) else { break; };
        let Some(prefix) = resolve_packet_prefix(packet, length.length) else { break; };
        if packet.get(prefix.payload_offset..prefix.payload_offset + 2) == Some(&[0xff, 0xff]) {
            visit_bundle_packets(&packet[prefix.payload_offset..], depth + 1, visit);
        } else { visit(packet); }
        at += size;
    }
}

#[cfg(test)]
mod framing_regressions {
    use super::*;
    fn var(mut n: usize) -> Vec<u8> {
        let mut out = Vec::new();
        loop { let byte = (n & 127) as u8; n >>= 7; out.push(byte | if n > 0 {128} else {0}); if n == 0 {return out;} }
    }
    fn frame(value: usize) -> Vec<u8> {
        let mut bytes=var(value); bytes.extend([0x01,0x91]); bytes.resize(value + var(value).len() - 4, 0x55); bytes
    }
    fn fixture(name: &str) -> Vec<u8> {
        let json: serde_json::Value=serde_json::from_str(include_str!("../../../tests/fixtures/native-transport.json")).unwrap();
        let text=json[name].as_str().unwrap(); (0..text.len()).step_by(2).map(|i|u8::from_str_radix(&text[i..i+2],16).unwrap()).collect()
    }
    #[test]
    fn varint_boundary_sizes_preserve_small_packets() {
        for value in [127usize,128,16383,16384] {
            let packet=frame(value); let len=read_varint(&packet,0);
            assert_eq!(transport_packet_size(len.value,len.length),Some(packet.len()),"encoded={value}");
            assert_eq!(inspect_stall_candidate(&packet).declared_packet_len,Some(packet.len()));
        }
    }
    #[test]
    fn split_tcp_never_dispatches_a_multibyte_frame_before_its_tail() {
        for value in [128usize,16384] {
            let packet=frame(value);
            for split in 1..packet.len() {
                let info=read_varint(&packet[..split],0);
                if info.is_valid() { assert!(transport_packet_size(info.value,info.length).unwrap()>split); }
            }
        }
    }
    #[test]
    fn real_login_resource_multibyte_frame_keeps_all_73_records() {
        let packet=fixture("0b61");let len=read_varint(&packet,0);
        let size=transport_packet_size(len.value,len.length).unwrap();
        assert_eq!(size,packet.len());assert_eq!(&packet[len.length..len.length+3],&[0x0b,0x61,73]);
    }
    #[test]
    fn real_outer_bundle_preserves_child_boundaries_and_the_following_hp() {
        let packet=fixture("bundle");let len=read_varint(&packet,0);
        let size=transport_packet_size(len.value,len.length).unwrap();
        assert_eq!(size,packet.len());
        let mut opcodes=Vec::new();
        visit_bundle_packets(&packet[len.length..size],0,&mut |child| {
            let length=read_varint(child,0);let prefix=resolve_packet_prefix(child,length.length).unwrap();
            opcodes.push(child[prefix.payload_offset..prefix.payload_offset+2].to_vec());
        });
        assert!(opcodes.len()>1);
        let hp=fixture("008d");let joined=[packet.as_slice(),hp.as_slice()].concat();
        assert_eq!(&joined[size..],hp.as_slice());
    }
    #[test]
    fn real_hp_and_death_frames_reach_the_correct_opcode_with_no_truncated_fields() {
        let hp=fixture("008d");let death=fixture("4236");
        let joined=[frame(128),hp.clone(),death.clone()].concat();
        let mut at=0;let mut decoded=Vec::new();
        while at<joined.len() {
            let length=read_varint(&joined,at);
            let size=transport_packet_size(length.value,length.length).unwrap();
            decoded.push(joined[at+length.length..at+size].to_vec());at+=size;
        }
        assert_eq!(decoded.len(),3);
        assert!(crate::dps_meter::capture::parser::summon::parse_remain_hp_fields(&decoded[1],2).is_some());
        assert_eq!(death_entity(&decoded[2]),Some(38114));
        for cut in 0..decoded[2].len() { assert_eq!(death_entity(&decoded[2][..cut]),None); }
    }
    #[test]
    fn compressed_children_cross_two_and_three_byte_lengths_without_losing_zero_hp() {
        let hp=vec![16,0x00,0x8d,0x8c,0x95,0x04,2,1,0,0,0,0,0];
        let body=[frame(128),frame(16384),hp.clone()].concat();
        let mut bundle=vec![0xff,0xff];bundle.extend((body.len() as u32).to_le_bytes());
        bundle.extend(lz4_flex::block::compress(&body));
        let mut children=Vec::new();visit_bundle_packets(&bundle,0,&mut |packet|children.push(packet.to_vec()));
        assert_eq!(children.len(),3);assert_eq!(children[2],hp);
        assert_eq!(crate::dps_meter::capture::parser::summon::parse_remain_hp_fields(&children[2][1..],2),Some((68236,0)));
    }
}
