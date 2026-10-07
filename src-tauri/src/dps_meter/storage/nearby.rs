use std::collections::HashMap;

use serde::Serialize;

pub const NEARBY_TTL_MS: u64 = 120_000;
pub const NEARBY_CAPACITY: usize = 500;

#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
pub struct Position {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NearbyPlayer {
    pub id: u32,
    pub name: String,
    pub server: Option<u32>,
    pub class: Option<&'static str>,
    pub level: Option<u32>,
    pub cp: Option<u64>,
    pub legion: Option<String>,
    pub last_seen: u64,
    pub last_position: Option<Position>,
    pub in_party: bool,
    pub seen_damage: u64,
}

#[derive(Debug, Clone)]
pub(crate) struct NearbyEntry {
    pub(crate) id: u32,
    pub(crate) server: Option<u32>,
    pub(crate) class: Option<&'static str>,
    pub(crate) level: Option<u32>,
    pub(crate) cp: Option<u64>,
    pub(crate) legion: Option<String>,
    pub(crate) last_seen: u64,
    pub(crate) last_position: Option<Position>,
    pub(crate) in_party: bool,
    pub(crate) seen_damage: u64,
    // Intrusive recency links make refresh and expiry O(1) without an
    // unbounded queue of stale expiry records.
    previous: Option<u32>,
    next: Option<u32>,
}

#[derive(Debug, Default, Clone)]
pub struct NearbyPlayers {
    players: HashMap<u32, NearbyEntry>,
    oldest: Option<u32>,
    newest: Option<u32>,
}

impl NearbyPlayers {
    pub(crate) fn upsert(&mut self, id: u32, server: Option<u32>, class: Option<&'static str>, cp: Option<u64>, now: u64) {
        if self.players.get(&id).is_some_and(|player| now.saturating_sub(player.last_seen) >= NEARBY_TTL_MS) {
            self.remove(id);
        }
        if !self.players.contains_key(&id) {
            if self.players.len() == NEARBY_CAPACITY {
                if let Some(oldest) = self.oldest { self.remove(oldest); }
            }
            self.players.insert(id, NearbyEntry {
                id, server, class, level: None, cp, legion: None,
                last_seen: now, last_position: None, in_party: false, seen_damage: 0,
                previous: None, next: None,
            });
        } else if let Some(player) = self.players.get_mut(&id) {
            if server.is_some() { player.server = server; }
            if class.is_some() { player.class = class; }
            if cp.is_some() { player.cp = cp; }
            player.last_seen = now;
        }
        self.move_to_newest(id);
    }

    pub(crate) fn add_damage(&mut self, id: u32, damage: u64, now: u64) {
        let active = self.players.get(&id).is_some_and(|player| now.saturating_sub(player.last_seen) < NEARBY_TTL_MS);
        if active {
            let Some(player) = self.players.get_mut(&id) else { return; };
            player.seen_damage = player.seen_damage.saturating_add(damage);
        }
    }

    pub(crate) fn remove(&mut self, id: u32) -> bool {
        let Some(player) = self.players.remove(&id) else { return false; };
        if let Some(previous) = player.previous {
            if let Some(node) = self.players.get_mut(&previous) { node.next = player.next; }
        } else { self.oldest = player.next; }
        if let Some(next) = player.next {
            if let Some(node) = self.players.get_mut(&next) { node.previous = player.previous; }
        } else { self.newest = player.previous; }
        true
    }

    pub(crate) fn snapshot(&mut self, now: u64) -> Vec<NearbyEntry> {
        self.expire(now);
        self.players.values().cloned().collect()
    }

    fn expire(&mut self, now: u64) {
        let expired: Vec<_> = self.players.iter()
            .filter(|(_, player)| now.saturating_sub(player.last_seen) >= NEARBY_TTL_MS)
            .map(|(id, _)| *id)
            .collect();
        for id in expired { self.remove(id); }
    }

    fn move_to_newest(&mut self, id: u32) {
        if self.newest == Some(id) { return; }
        if self.players.get(&id).is_some_and(|player| player.previous.is_some() || player.next.is_some() || self.oldest == Some(id)) {
            self.remove_links(id);
        }
        let previous = self.newest;
        if let Some(previous) = previous {
            if let Some(node) = self.players.get_mut(&previous) { node.next = Some(id); }
        } else { self.oldest = Some(id); }
        if let Some(player) = self.players.get_mut(&id) {
            player.previous = previous;
            player.next = None;
        }
        self.newest = Some(id);
    }

    fn remove_links(&mut self, id: u32) {
        let (previous, next) = match self.players.get(&id) { Some(player) => (player.previous, player.next), None => return };
        if let Some(previous) = previous {
            if let Some(node) = self.players.get_mut(&previous) { node.next = next; }
        } else { self.oldest = next; }
        if let Some(next) = next {
            if let Some(node) = self.players.get_mut(&next) { node.previous = previous; }
        } else { self.newest = previous; }
        if let Some(player) = self.players.get_mut(&id) { player.previous = None; player.next = None; }
    }
}

#[cfg(test)]
mod tests {
    use super::{NearbyPlayers, NEARBY_CAPACITY, NEARBY_TTL_MS};

    #[test]
    fn refreshes_metadata_expires_and_bounds_entries() {
        let mut table = NearbyPlayers::default();
        table.upsert(1, Some(22), Some("GLADIATOR"), Some(9000), 10);
        table.add_damage(1, 400, 11);
        table.upsert(1, None, None, None, 12);
        let rows = table.snapshot(12);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].server, Some(22));
        assert_eq!(rows[0].cp, Some(9000));
        assert_eq!(rows[0].seen_damage, 400);
        assert!(table.snapshot(12 + NEARBY_TTL_MS).is_empty());

        for id in 1..=(NEARBY_CAPACITY as u32 + 1) {
            table.upsert(id, None, None, None, 20_000 + id as u64);
        }
        let rows = table.snapshot(21_000);
        assert_eq!(rows.len(), NEARBY_CAPACITY);
        assert!(!rows.iter().any(|row| row.id == 1));
    }
}
