use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillStats {
    pub counts: u32,
    pub total_damage: u64,
    pub min_damage: u64,
    pub max_damage: u64,
    #[serde(default)]
    pub special_counts: HashMap<String, u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PvpWatchInfo {
    pub query_name: String,
    pub actor_id: Option<u32>,
    pub actor_name: Option<String>,
    pub server_id: Option<String>,
    pub actor_class: Option<String>,
    pub current_hp: Option<u32>,
    pub max_hp: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PvpKnownPlayer {
    pub actor_id: u32,
    pub actor_name: String,
    pub server_id: Option<String>,
    pub actor_class: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PvpWatchInfoResponse {
    pub watch_info: Vec<PvpWatchInfo>,
    pub known_players: Vec<PvpKnownPlayer>,
    pub last_dealt_player: Option<PvpWatchInfo>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PvpCombatStats {
    pub damage: u64,
    pub kills: u32,
    pub assists: u32,
    pub deaths: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PvpCombatStatsRow {
    pub actor_name: String,
    pub server_id: String,
    pub damage: u64,
    pub kills: u32,
    pub assists: u32,
    pub deaths: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerHpInfo {
    pub actor_id: u32,
    pub current_hp: u32,
    pub max_observed_hp: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuffInterval {
    pub start_ms: u64,
    pub end_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuffSummary {
    pub target_id: u32,
    pub actor_id: u32,
    pub skill_code: u32,
    pub coverage: f64,
    pub active: bool,
    pub last_start_ms: u64,
    pub last_end_ms: u64,
}

impl SkillStats {
    pub fn new() -> Self {
        Self {
            counts: 0,
            total_damage: 0,
            min_damage: u64::MAX,
            max_damage: 0,
            special_counts: HashMap::new(),
        }
    }
}

impl Default for SkillStats {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActorInfo {
    pub id: u32,
    pub actor_name: Option<String>,
    pub actor_server_id: Option<String>,
    pub actor_class: Option<String>,
    #[serde(default)]
    pub combat_power: Option<u64>,
    pub actor_skill_spec: HashMap<u32, Vec<u32>>,
    #[serde(default)]
    pub heal_total: u64,
    #[serde(default)]
    pub deaths: u32,
}

/// Absolute health decoded from packets; never populate from an observed peak or percentage.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PacketHp {
    pub hp: Option<u32>,
    pub max_hp: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetInfo {
    pub id: u32,
    pub target_mob_code: Option<u32>,
    pub target_name: Option<String>,
    pub is_boss: bool,
    pub current_hp: Option<u32>,
    /// Legacy highest observed HP, used by overlays and hpPct; not a decoded maximum.
    pub max_hp: Option<u32>,
    /// Persist packet provenance separately so old history estimates remain distinguishable.
    #[serde(default)]
    pub packet_hp: Option<PacketHp>,
    pub target_start_time: HashMap<u32, f64>,
    pub target_last_time: HashMap<u32, f64>,
    /// Encounter clock, separate from the damage interval used for run DPS.
    #[serde(default)]
    pub elapsed_at: Option<f64>,
    #[serde(default)]
    pub stopped_at: Option<f64>,
}

impl TargetInfo {
    pub fn started_at(&self) -> Option<f64> {
        self.target_start_time.values().copied().filter(|v| v.is_finite()).reduce(f64::min)
    }
    pub fn ended_at(&self) -> Option<f64> {
        self.stopped_at.or(self.elapsed_at).or_else(|| self.target_last_time.values().copied().reduce(f64::max))
    }
    pub fn duration_seconds(&self) -> f64 {
        self.started_at().zip(self.ended_at()).map(|(start, end)| (end-start).max(0.0)).unwrap_or(0.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CombatInfos {
    pub actor_infos: HashMap<u32, ActorInfo>,
    pub target_infos: HashMap<u32, TargetInfo>,
    pub main_actor_id: Option<u32>,
    pub main_actor_name: Option<String>,
    pub last_target_by_main_actor: Option<u32>,
    pub last_target: Option<u32>,
    pub time_now: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerOverviewStat {
    pub actor_id: u32,
    pub actor_name: String,
    pub actor_server_id: String,
    pub actor_class: String,
    #[serde(default)]
    pub combat_power: Option<u64>,
    pub counts: u32,
    pub total_damage: u64,
    pub min_damage: u64,
    pub max_damage: u64,
    #[serde(default)]
    pub special_counts: HashMap<String, u32>,
    pub dps: f64,
    pub damage_share: f64,
    pub damage_contribution: f64,
    #[serde(default)]
    pub heal_total: u64,
    #[serde(default)]
    pub hps: f64,
    #[serde(default)]
    pub deaths: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CombatEvent {
    pub actor_id: u32,
    pub target_id: u32,
    pub skill_code: u32,
    pub damage: u64,
    pub is_crit: bool,
    pub at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TakenSourceStat {
    pub source_id: u32,
    pub npc_id: Option<u32>,
    pub source_name: Option<String>,
    pub skill_code: u32,
    pub damage: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecapEvent {
    pub kind: String,
    pub source_id: u32,
    pub source_name: Option<String>,
    pub skill_code: u32,
    pub amount: u64,
    pub crit: bool,
    pub at_ms: u64,
    pub hp_after: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeathRecap {
    pub player_id: u32,
    pub at_ms: u64,
    pub events: Vec<RecapEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BossCast {
    pub npc_id: u32,
    pub skill: u32,
    pub at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PvpDamageOverviewStat {
    pub player_id: u32,
    pub player_name: String,
    pub player_server_id: String,
    pub player_class: String,
    pub counts: u32,
    pub total_damage: u64,
    pub min_damage: u64,
    pub max_damage: u64,
    pub dps: f64,
    pub damage_contribution: f64,
    pub hp_damage_ratio: Option<f64>,
    pub battle_start_time: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CombatSnapshot {
    pub total_damage: u64,
    pub by_target_player_skill_stats: HashMap<u32, HashMap<u32, HashMap<u32, SkillStats>>>,
    pub by_target_player_stats: HashMap<u32, HashMap<u32, PlayerOverviewStat>>,
    #[serde(default)]
    pub use_buffs_by_target: HashMap<u32, Vec<BuffSummary>>,
    pub combat_infos: CombatInfos,
    pub last_target_info: Option<TargetInfo>,
    #[serde(default)]
    pub last_target_all_players_overview_stats: Vec<PlayerOverviewStat>,
    #[serde(default)]
    pub main_actor_received_player_overview_stats: Vec<PlayerOverviewStat>,
    #[serde(default)]
    pub main_actor_dealt_player_overview_stats: Vec<PvpDamageOverviewStat>,
    #[serde(default, skip_serializing)]
    pub combat_events: Vec<CombatEvent>,
    #[serde(default, skip_serializing)]
    pub buff_intervals: HashMap<u32, HashMap<u32, HashMap<u32, Vec<BuffInterval>>>>,
    #[serde(default)]
    pub damage_taken_by_target: HashMap<u32, HashMap<u32, Vec<TakenSourceStat>>>,
    #[serde(default)]
    pub death_recaps_by_target: HashMap<u32, Vec<DeathRecap>>,
    #[serde(default)]
    pub boss_casts_by_target: HashMap<u32, Vec<BossCast>>,
}

impl CombatSnapshot {
    /// The UI never receives these internal collections (serde skips them).
    /// Do not allocate a second copy of the entire event archive just to cache a view.
    pub fn ui_copy(&self) -> Self {
        Self {
            total_damage: self.total_damage,
            by_target_player_skill_stats: self.by_target_player_skill_stats.clone(),
            by_target_player_stats: self.by_target_player_stats.clone(),
            use_buffs_by_target: self.use_buffs_by_target.clone(),
            combat_infos: self.combat_infos.clone(),
            last_target_info: self.last_target_info.clone(),
            last_target_all_players_overview_stats: self.last_target_all_players_overview_stats.clone(),
            main_actor_received_player_overview_stats: self.main_actor_received_player_overview_stats.clone(),
            main_actor_dealt_player_overview_stats: self.main_actor_dealt_player_overview_stats.clone(),
            combat_events: Vec::new(),
            buff_intervals: HashMap::new(),
            damage_taken_by_target: self.damage_taken_by_target.clone(),
            death_recaps_by_target: self.death_recaps_by_target.clone(),
            boss_casts_by_target: self.boss_casts_by_target.clone(),
        }
    }
}

#[cfg(test)]
mod ui_snapshot_tests {
    use super::*;

    #[test]
    fn ui_copy_preserves_json_without_copying_internal_event_history() {
        let snapshot: CombatSnapshot = serde_json::from_value(serde_json::json!({
            "totalDamage": 12345, "byTargetPlayerSkillStats": {}, "byTargetPlayerStats": {},
            "combatInfos": {"actorInfos": {}, "targetInfos": {}, "timeNow": 1.0},
            "combatEvents": [{"actorId": 1, "targetId": 2, "skillCode": 3, "damage": 12345, "isCrit": false, "atMs": 1000}],
            "buffIntervals": {"2": {"1": {"3": [{"startMs": 0, "endMs": 1000}]}}}
        })).unwrap();
        let ui = snapshot.ui_copy();
        assert!(ui.combat_events.is_empty(), "UI cache copied internal event history");
        assert!(ui.buff_intervals.is_empty());
        assert_eq!(serde_json::to_value(&ui).unwrap(), serde_json::to_value(&snapshot).unwrap());
        assert_eq!(snapshot.combat_events.len(), 1, "Original history was changed");
        assert_eq!(snapshot.buff_intervals.len(), 1, "Original buff history was changed");
    }
}
