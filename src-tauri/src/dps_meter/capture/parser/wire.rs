//! Strict field-mask decoding of the combat messages `04 38` (direct) and `05 38` (periodic).
//!
//! The message body is read field by field from a presence mask; a message is accepted only when
//! every byte is consumed (`complete`). Packed booleans share one byte across reads, the way the
//! client writes them. Rules for what counts as damage live in `reference`, not here.
//!
//! Layout verified on real captures (DPS audit 19.10.2026): on 93 k direct messages the strict
//! reading gives the same target, actor, skill and damage as the old offset reader wherever both
//! decode, and it also reads the periodic `remaining` / `current` amounts apart.

/// Cursor over one message body (after the two opcode bytes).
pub struct WireReader<'a> {
    data: &'a [u8],
    pos: usize,
    failed: bool,
    bool_byte: u8,
    bool_bit: u8,
}

impl<'a> WireReader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0, failed: false, bool_byte: 0, bool_bit: 8 }
    }

    pub fn complete(&self) -> bool {
        !self.failed && self.pos == self.data.len()
    }

    fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    fn take(&mut self, count: usize) -> &'a [u8] {
        if self.failed || count > self.remaining() {
            self.failed = true;
            return &[];
        }
        let slice = &self.data[self.pos..self.pos + count];
        self.pos += count;
        slice
    }

    pub fn skip(&mut self, count: usize) {
        self.take(count);
    }

    pub fn u8(&mut self) -> u8 {
        self.take(1).first().copied().unwrap_or(0)
    }

    pub fn u16(&mut self) -> u16 {
        let bytes = self.take(2);
        if bytes.len() == 2 { u16::from_le_bytes([bytes[0], bytes[1]]) } else { 0 }
    }

    pub fn u32(&mut self) -> u32 {
        let bytes = self.take(4);
        if bytes.len() == 4 { u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) } else { 0 }
    }

    /// Little-endian base-128 value of at most `bits` bits; an over-long or overflowing value fails the read.
    fn var(&mut self, bits: u32) -> u64 {
        let mut value = 0u64;
        let mut shift = 0u32;
        while shift < bits {
            if self.failed {
                break;
            }
            let byte = self.u8();
            if self.failed {
                break;
            }
            if shift + 7 > bits && u64::from(byte & 0x7F) >= (1u64 << (bits - shift)) {
                self.failed = true;
                return 0;
            }
            value |= u64::from(byte & 0x7F) << shift;
            if byte & 0x80 == 0 {
                return value;
            }
            shift += 7;
        }
        self.failed = true;
        0
    }

    pub fn v32(&mut self) -> u32 {
        self.var(32) as u32
    }

    pub fn v64(&mut self) -> u64 {
        let value = self.var(64);
        if value > i64::MAX as u64 {
            self.failed = true;
        }
        if self.failed { 0 } else { value }
    }

    /// Entity id: a 32-bit value that must fit a signed int.
    pub fn id(&mut self) -> u32 {
        let value = self.v32();
        if value > i32::MAX as u32 {
            self.failed = true;
        }
        if self.failed { 0 } else { value }
    }

    /// Element count, refused when it cannot fit the bytes left (`min_bytes` per element).
    pub fn count(&mut self, min_bytes: usize) -> usize {
        let value = self.v32() as usize;
        if value > self.remaining() / min_bytes.max(1) {
            self.failed = true;
        }
        if self.failed { 0 } else { value }
    }

    /// Packed boolean: one byte holds eight flags and is shared by consecutive boolean reads.
    pub fn boolean(&mut self) -> bool {
        if self.bool_bit == 8 {
            self.bool_byte = self.u8();
            self.bool_bit = 0;
        }
        let set = self.bool_byte & (1 << self.bool_bit) != 0;
        self.bool_bit += 1;
        set
    }
}

/// Special flag bits of a direct hit (bits 0..5 are the first six packed booleans).
pub mod special {
    pub const BLOCK: u32 = 0x01;
    pub const PARRY: u32 = 0x02;
    pub const PERFECT: u32 = 0x04;
    pub const DOUBLE: u32 = 0x08;
    pub const SMITE: u32 = 0x20;
    pub const EXTRA: u32 = 0x40;
    pub const MASK_BIT4: u32 = 0x80;
    /// No damage field and no block/parry: the attack missed.
    pub const EVADE: u32 = 0x400;
}

/// One `04 38` message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectHit {
    pub target_id: u32,
    pub actor_id: u32,
    pub mask: u16,
    pub raw_skill_code: u32,
    pub action: u8,
    /// Hit type: 3 is a critical hit.
    pub type_value: u8,
    pub special: u32,
    /// 1 = back, 2 = front.
    pub position: u8,
    pub restoration_hp: u64,
    pub effect_id: u32,
    pub sequence: u32,
    pub switch_value: u32,
    pub damage: u64,
    pub sub_hits: Vec<u64>,
    pub loop_value: u16,
    pub drain: u64,
    pub regeneration: u64,
}

impl DirectHit {
    pub fn has_damage_field(&self) -> bool {
        self.mask & 0x04 != 0
    }
    pub fn and_result(&self) -> u16 {
        self.mask & 0xFF
    }
    pub fn flag_value(&self) -> u16 {
        self.mask >> 8
    }
}

/// Decodes a `04 38` payload (starting at the opcode). `None` when the message is not this layout.
pub fn decode_direct(payload: &[u8]) -> Option<DirectHit> {
    if payload.len() < 2 || payload[0] != 0x04 || payload[1] != 0x38 {
        return None;
    }
    let mut r = WireReader::new(&payload[2..]);
    let target_id = r.id();
    let mask = r.u16();
    if mask & 0xC000 != 0 {
        return None;
    }
    let actor_id = r.id();
    let raw_skill_code = r.u32();
    let action = r.u8();
    if mask & 0x01 != 0 {
        r.u32();
    }
    let type_value = r.u8();
    let mut special = 0u32;
    let mut position = 0u8;
    let mut restoration_hp = 0u64;
    if mask & 0x02 != 0 {
        for bit in 0..6 {
            if r.boolean() {
                special |= 1 << bit;
            }
        }
        restoration_hp = r.v64();
        if r.boolean() {
            special |= special::EXTRA;
        }
        position = r.u8();
    }
    let effect_id = r.u32();
    let sequence = r.u32();
    let switch_value = r.v32();
    let damage = if mask & 0x04 != 0 { r.v64() } else { 0 };
    if mask & 0x08 != 0 {
        r.u32();
    }
    if mask & 0x10 != 0 && r.boolean() {
        special |= special::MASK_BIT4;
    }
    let mut sub_hits = Vec::new();
    if mask & 0x20 != 0 {
        let count = r.count(1);
        for _ in 0..count {
            sub_hits.push(r.v64());
        }
    }
    if mask & 0x40 != 0 {
        let count = r.count(4);
        r.skip(count * 4);
    }
    if mask & 0x80 != 0 {
        r.u8();
    }
    if mask & 0x100 != 0 {
        let count = r.count(4);
        r.skip(count * 4);
    }
    let loop_value = r.u16();
    let drain = if mask & 0x200 != 0 { r.v64() } else { 0 };
    let regeneration = if mask & 0x400 != 0 { r.v64() } else { 0 };
    if mask & 0x800 != 0 {
        r.u32();
    }
    if mask & 0x1000 != 0 {
        r.boolean();
    }
    if mask & 0x2000 != 0 {
        r.u32();
    }
    if !r.complete() || target_id == 0 || actor_id == 0 {
        return None;
    }
    // a miss: no damage field, not blocked/parried, hit type 1
    if mask & 0x04 == 0 && special & (special::BLOCK | special::PARRY) == 0 && type_value == 1 {
        special |= special::EVADE;
    }
    Some(DirectHit {
        target_id, actor_id, mask, raw_skill_code, action, type_value, special, position, restoration_hp,
        effect_id, sequence, switch_value, damage, sub_hits, loop_value, drain, regeneration,
    })
}

/// One `05 38` message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PeriodicTick {
    pub target_id: u32,
    /// Presence mode: 1 remaining, 2 current, 8 skill, 0x10 source, …; 10/11/48/56 are the common ones.
    pub mode: u8,
    pub actor_id: u32,
    pub switch_value: u32,
    pub effect_id: u32,
    pub remaining: u64,
    pub current: u64,
    pub skill_code: u32,
    pub source_id: u32,
}

impl PeriodicTick {
    /// Modes that carry a usable amount (1 = remaining only, any with the current tick) or are known
    /// zero-amount notices (48, 56).
    pub fn supported(&self) -> bool {
        self.mode == 1 || self.mode & 0x02 != 0 || self.zero_notice()
    }
    pub fn zero_notice(&self) -> bool {
        self.mode == 48 || self.mode == 56
    }
    /// The tick amount: the current value when present, otherwise the remaining pool.
    pub fn amount(&self) -> u64 {
        if self.mode & 0x02 != 0 { self.current } else { self.remaining }
    }
    /// Mode 48 names the real source separately.
    pub fn attacker(&self) -> u32 {
        if self.mode == 48 && self.source_id > 0 { self.source_id } else { self.actor_id }
    }
}

/// Decodes a `05 38` payload (starting at the opcode). `None` when the message is not this layout.
pub fn decode_periodic(payload: &[u8]) -> Option<PeriodicTick> {
    if payload.len() < 2 || payload[0] != 0x05 || payload[1] != 0x38 {
        return None;
    }
    let mut r = WireReader::new(&payload[2..]);
    let target_id = r.id();
    let mode = r.u8();
    if mode & 0x80 != 0 {
        return None;
    }
    let actor_id = r.id();
    let switch_value = r.id();
    let effect_id = r.u32();
    let remaining = if mode & 0x01 != 0 { r.v64() } else { 0 };
    let current = if mode & 0x02 != 0 { r.v64() } else { 0 };
    if mode & 0x04 != 0 {
        let count = r.count(4);
        r.skip(count * 4);
    }
    let skill_code = if mode & 0x08 != 0 { r.u32() } else { 0 };
    let source_id = if mode & 0x10 != 0 { r.id() } else { 0 };
    if mode & 0x20 != 0 {
        r.u32();
    }
    if mode & 0x40 != 0 {
        r.u32();
    }
    if !r.complete() || target_id == 0 || actor_id == 0 {
        return None;
    }
    Some(PeriodicTick { target_id, mode, actor_id, switch_value, effect_id, remaining, current, skill_code, source_id })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(text: &str) -> Vec<u8> {
        (0..text.len()).step_by(2).map(|at| u8::from_str_radix(&text[at..at + 2], 16).unwrap()).collect()
    }

    #[test]
    fn packed_booleans_share_one_byte_across_other_reads() {
        let data = [0b0000_0101u8, 0x07];
        let mut r = WireReader::new(&data);
        assert!(r.boolean());
        assert!(!r.boolean());
        assert_eq!(r.u8(), 0x07);
        assert!(r.boolean()); // bit 2 of the first byte, no new byte read
        assert!(r.complete());
    }

    #[test]
    fn overlong_values_fail_instead_of_wrapping() {
        let mut r = WireReader::new(&[0xFF, 0xFF, 0xFF, 0xFF, 0x7F]);
        assert_eq!(r.v32(), 0);
        assert!(!r.complete());
        let mut ok = WireReader::new(&[0xFF, 0xFF, 0xFF, 0xFF, 0x0F]);
        assert_eq!(ok.v32(), u32::MAX);
        assert!(ok.complete());
    }

    #[test]
    fn real_ferk_incoming_hit_decodes_completely() {
        // Ferk 53919 -> rwnd 9474, 13 October (also covered by the old reader's regression test).
        let hit = decode_direct(&hex("0438824A06009FA503EC87180002020000023B18950901000000904E940C0100")).unwrap();
        assert_eq!((hit.target_id, hit.actor_id, hit.raw_skill_code, hit.damage), (9474, 53919, 1607660, 1556));
        assert_eq!((hit.type_value, hit.position, hit.switch_value, hit.loop_value), (2, 2, 10000, 1));
        assert_eq!(hit.special, 0);
    }

    #[test]
    fn truncated_or_extended_messages_are_refused() {
        let good = hex("0438824A06009FA503EC87180002020000023B18950901000000904E940C0100");
        for cut in 2..good.len() {
            assert!(decode_direct(&good[..cut]).is_none(), "cut {cut}");
        }
        let mut longer = good.clone();
        longer.push(0);
        assert!(decode_direct(&longer).is_none());
    }

    #[test]
    fn periodic_reads_remaining_and_current_apart() {
        // target 300, mode 11 (remaining|current|skill), actor 200, switch 5, effect 0x01020304,
        // remaining 3000, current 1000, skill 0x00D7B5E0.
        let mut body = vec![0x05, 0x38, 0xAC, 0x02, 11, 0xC8, 0x01, 5, 0x04, 0x03, 0x02, 0x01];
        body.extend([0xB8, 0x17]); // 3000
        body.extend([0xE8, 0x07]); // 1000
        body.extend([0xE0, 0xB5, 0xD7, 0x00]);
        let tick = decode_periodic(&body).unwrap();
        assert_eq!((tick.target_id, tick.actor_id, tick.mode), (300, 200, 11));
        assert_eq!((tick.remaining, tick.current, tick.amount()), (3000, 1000, 1000));
        assert_eq!(tick.skill_code, 0x00D7B5E0);
        assert!(tick.supported());
    }
}
