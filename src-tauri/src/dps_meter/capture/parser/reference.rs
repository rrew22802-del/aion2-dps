//! What a strictly decoded combat message is: damage for the meter, healing, or neither.
//!
//! Rules of the «reference» meter (the in-game analyzer as reproduced by the NotMeter study,
//! 19.10.2026): only positive amounts count; an actor hitting itself is not damage; HP restore /
//! resource restore effects are healing / support by their EFFECT id (catalog built from our own
//! client tables), never by the skill as a whole; periodic «pool» notices are not ticks.

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use serde::Deserialize;

use super::wire::{DirectHit, PeriodicTick};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CatalogFile {
    direct_hp: Vec<u32>,
    direct_support: Vec<u32>,
    periodic_hp: Vec<u32>,
    periodic_support: Vec<u32>,
}

/// Effect ids by kind, from `data/effect_catalog.json` (SkillEffect HpHeal*/Mp|Sp|Dp|Op|Fp|AP Heal* and
/// SkillAbnormalEffect Hot*/Mot*/Sot* of our client extract).
pub struct EffectCatalog {
    direct_hp: HashSet<u32>,
    direct_support: HashSet<u32>,
    periodic_hp: HashSet<u32>,
    periodic_support: HashSet<u32>,
}

pub fn catalog() -> &'static EffectCatalog {
    static CATALOG: OnceLock<EffectCatalog> = OnceLock::new();
    CATALOG.get_or_init(|| {
        let file: CatalogFile = serde_json::from_str(include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/data/effect_catalog.json")))
            .expect("effect catalog");
        EffectCatalog {
            direct_hp: file.direct_hp.into_iter().collect(),
            direct_support: file.direct_support.into_iter().collect(),
            periodic_hp: file.periodic_hp.into_iter().collect(),
            periodic_support: file.periodic_support.into_iter().collect(),
        }
    })
}

#[derive(Deserialize)]
struct AnalyzerFile {
    groups: HashMap<String, u32>,
    exceptions: Vec<u32>,
}

/// The game's own analyzer data (`data/analyzer_data.json`, from our client tables):
/// Skill.DamageAnalyzerSkillIdOverride — which skill a hit is reported under;
/// SkillAbnormalEffect.DamageAnalyzerException — effects the analyzer leaves out.
pub struct AnalyzerData {
    groups: HashMap<u32, u32>,
    exceptions: HashSet<u32>,
}

pub fn analyzer() -> &'static AnalyzerData {
    static DATA: OnceLock<AnalyzerData> = OnceLock::new();
    DATA.get_or_init(|| {
        let file: AnalyzerFile = serde_json::from_str(include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/data/analyzer_data.json")))
            .expect("analyzer data");
        AnalyzerData {
            groups: file.groups.into_iter().filter_map(|(k, v)| k.parse().ok().map(|k| (k, v))).collect(),
            exceptions: file.exceptions.into_iter().collect(),
        }
    })
}

impl AnalyzerData {
    /// The skill the game reports `raw` under, when it differs from `raw`.
    pub fn group(&self, raw: u32) -> Option<u32> {
        self.groups.get(&raw).copied()
    }
    pub fn is_exception(&self, effect_id: u32) -> bool {
        self.exceptions.contains(&effect_id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceKind {
    Damage,
    Healing,
    Support,
}

impl EffectCatalog {
    pub fn direct(&self, effect_id: u32) -> ResourceKind {
        if self.direct_hp.contains(&effect_id) { ResourceKind::Healing }
        else if self.direct_support.contains(&effect_id) { ResourceKind::Support }
        else { ResourceKind::Damage }
    }
    pub fn periodic(&self, effect_id: u32) -> ResourceKind {
        if self.periodic_hp.contains(&effect_id) { ResourceKind::Healing }
        else if self.periodic_support.contains(&effect_id) { ResourceKind::Support }
        else { ResourceKind::Damage }
    }
}

/// Where a decoded event goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    /// Counted in the meter: (attacker, amount).
    Damage { actor_id: u32, amount: u64 },
    /// HP restored: (healer, amount).
    Healing { actor_id: u32, amount: u64 },
    /// Neither (support value, self event, pool notice, miss …).
    Ignore,
}

/// Direct hit. `summon_link(a, b)` is true when one of the two is the other's summon.
pub fn route_direct(hit: &DirectHit, summon_link: impl Fn(u32, u32) -> bool) -> Route {
    if hit.damage == 0 {
        return Route::Ignore;
    }
    match catalog().direct(hit.effect_id) {
        ResourceKind::Healing => return Route::Healing { actor_id: hit.actor_id, amount: hit.damage },
        ResourceKind::Support => return Route::Ignore,
        ResourceKind::Damage => {}
    }
    if hit.actor_id == hit.target_id {
        return Route::Ignore;
    }
    // a summon and its owner exchange a bare value (mask 4, type 2, one loop): support, not damage
    if hit.and_result() == 4 && hit.flag_value() == 0 && hit.type_value == 2 && hit.loop_value == 1
        && summon_link(hit.actor_id, hit.target_id)
    {
        return Route::Ignore;
    }
    Route::Damage { actor_id: hit.actor_id, amount: hit.damage }
}

/// Periodic pools: mode 9 opens a pool, 11 refreshes it, the mode-10 message that closes it is not a tick.
#[derive(Debug, Default)]
pub struct PeriodicPools {
    states: HashMap<(u32, u32, u32), f64>,
}

const POOL_TTL_SECONDS: f64 = 120.0;

impl PeriodicPools {
    pub fn clear(&mut self) {
        self.states.clear();
    }

    fn counts_as_damage(&mut self, tick: &PeriodicTick, at: f64) -> bool {
        let key = (tick.target_id, tick.switch_value, tick.skill_code);
        match tick.mode {
            8 | 48 => false,
            9 => {
                if tick.switch_value != 0 {
                    self.states.retain(|_, seen| at - *seen <= POOL_TTL_SECONDS);
                    self.states.insert(key, at);
                }
                false
            }
            11 => {
                if tick.switch_value != 0 {
                    if let Some(seen) = self.states.get_mut(&key) {
                        *seen = at;
                    }
                }
                false
            }
            10 => {
                if tick.switch_value != 0 && self.states.remove(&key).is_some() {
                    return false;
                }
                tick.switch_value != 0 && tick.skill_code > 0
            }
            _ => true,
        }
    }

    /// Periodic tick at `at` seconds.
    pub fn route(&mut self, tick: &PeriodicTick, at: f64) -> Route {
        // the game's analyzer leaves these effects out (SkillAbnormalEffect.DamageAnalyzerException)
        if analyzer().is_exception(tick.effect_id) {
            return Route::Ignore;
        }
        let kind = catalog().periodic(tick.effect_id);
        if kind != ResourceKind::Damage && (tick.mode & 0x02 == 0 || tick.current == 0) {
            return Route::Ignore;
        }
        if !tick.supported() {
            return Route::Ignore;
        }
        let zero = tick.zero_notice();
        if tick.skill_code == 0 && kind == ResourceKind::Damage {
            return Route::Ignore;
        }
        let self_heal = kind == ResourceKind::Damage && tick.actor_id == tick.target_id && tick.mode == 11;
        if tick.actor_id == tick.target_id && !self_heal && !zero && kind == ResourceKind::Damage {
            return Route::Ignore;
        }
        let amount = tick.amount();
        if zero || amount == 0 {
            return Route::Ignore;
        }
        let actor_id = tick.attacker();
        if kind == ResourceKind::Healing || self_heal {
            return Route::Healing { actor_id, amount };
        }
        if kind == ResourceKind::Support || actor_id == tick.target_id {
            return Route::Ignore;
        }
        if self.counts_as_damage(tick, at) {
            Route::Damage { actor_id, amount }
        } else {
            Route::Ignore
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tick(mode: u8, switch_value: u32, skill_code: u32, remaining: u64, current: u64) -> PeriodicTick {
        PeriodicTick { target_id: 300, mode, actor_id: 200, switch_value, effect_id: 999_999_999, remaining, current, skill_code, source_id: 0 }
    }

    #[test]
    fn catalog_loads_all_four_lists() {
        let c = catalog();
        assert_eq!((c.direct_hp.len(), c.direct_support.len(), c.periodic_hp.len(), c.periodic_support.len()), (418, 181, 134, 16));
        assert_eq!(c.direct(41), ResourceKind::Healing);
        assert_eq!(c.direct(51), ResourceKind::Support);
        assert_eq!(c.periodic(2941), ResourceKind::Healing);
        assert_eq!(c.periodic(2981), ResourceKind::Support);
        assert_eq!(c.direct(11), ResourceKind::Damage);
    }

    #[test]
    fn analyzer_data_from_the_client_tables() {
        let a = analyzer();
        assert_eq!((a.groups.len(), a.exceptions.len()), (6624, 18));
        assert_eq!(a.group(100018), Some(100011)); // elementalist spirit hits fold onto its basic attack
        assert_eq!(a.group(16001104), Some(16001101));
        assert_eq!(a.group(14090240), Some(14090000));
        assert!(a.is_exception(190000031));
        let mut pools = PeriodicPools::default();
        let mut excluded = tick(10, 7, 1234, 0, 500);
        excluded.effect_id = 190000031;
        assert_eq!(pools.route(&excluded, 1.0), Route::Ignore);
    }

    #[test]
    fn mode_eleven_never_counts_its_remaining_pool() {
        let mut pools = PeriodicPools::default();
        // the old reader took 3000 (remaining) here — the ×3 overcount
        assert_eq!(pools.route(&tick(11, 7, 1234, 3000, 1000), 1.0), Route::Ignore);
        assert_eq!(pools.route(&tick(10, 7, 1234, 0, 1000), 2.0), Route::Damage { actor_id: 200, amount: 1000 });
    }

    #[test]
    fn mode_ten_closing_a_refreshed_pool_is_not_a_tick() {
        let mut pools = PeriodicPools::default();
        pools.states.insert((300, 7, 1234), 0.0);
        assert_eq!(pools.route(&tick(11, 7, 1234, 3000, 1000), 1.0), Route::Ignore); // refresh
        assert_eq!(pools.route(&tick(10, 7, 1234, 0, 500), 2.0), Route::Ignore); // closes the pool
        assert_eq!(pools.route(&tick(10, 7, 1234, 0, 500), 3.0), Route::Damage { actor_id: 200, amount: 500 });
    }

    #[test]
    fn mode_ten_needs_a_switch_and_a_skill() {
        let mut pools = PeriodicPools::default();
        assert_eq!(pools.route(&tick(10, 0, 1234, 0, 500), 1.0), Route::Ignore);
        assert_eq!(pools.route(&tick(2, 7, 0, 0, 500), 1.0), Route::Ignore);
        assert_eq!(pools.route(&tick(48, 7, 1234, 0, 500), 1.0), Route::Ignore);
    }

    #[test]
    fn self_mode_eleven_is_healing_and_other_self_ticks_are_dropped() {
        let mut pools = PeriodicPools::default();
        let mut own = tick(11, 7, 1234, 3000, 400);
        own.target_id = own.actor_id;
        assert_eq!(pools.route(&own, 1.0), Route::Healing { actor_id: 200, amount: 400 });
        own.mode = 10;
        assert_eq!(pools.route(&own, 2.0), Route::Ignore);
    }

    #[test]
    fn direct_rules() {
        let mut hit = super::super::wire::decode_direct(&[
            0x04, 0x38, 0x82, 0x4A, 0x06, 0x00, 0x9F, 0xA5, 0x03, 0xEC, 0x87, 0x18, 0x00, 0x02, 0x02, 0x00, 0x00, 0x02,
            0x3B, 0x18, 0x95, 0x09, 0x01, 0x00, 0x00, 0x00, 0x90, 0x4E, 0x94, 0x0C, 0x01, 0x00,
        ]).unwrap();
        assert_eq!(route_direct(&hit, |_, _| false), Route::Damage { actor_id: 53919, amount: 1556 });
        hit.effect_id = 41;
        assert_eq!(route_direct(&hit, |_, _| false), Route::Healing { actor_id: 53919, amount: 1556 });
        hit.effect_id = 51;
        assert_eq!(route_direct(&hit, |_, _| false), Route::Ignore);
        hit.effect_id = 11;
        hit.target_id = hit.actor_id;
        assert_eq!(route_direct(&hit, |_, _| false), Route::Ignore);
        hit.target_id = 9474;
        hit.mask = 4;
        assert_eq!(route_direct(&hit, |_, _| true), Route::Ignore);
        assert_eq!(route_direct(&hit, |_, _| false), Route::Damage { actor_id: 53919, amount: 1556 });
    }
}

/// Cross-check against the independent Python implementation (DPS audit bench):
/// `DPS_CROSSCHECK=<prefix> cargo test --lib crosscheck -- --ignored` reads `<prefix>.messages.txt`
/// (one `seconds hex` line per 04 38 / 05 38 message) and writes `<prefix>.rust.json`.
#[cfg(test)]
mod crosscheck {
    use super::*;
    use crate::dps_meter::capture::parser::wire::{decode_direct, decode_periodic};
    use std::collections::BTreeMap;

    #[test]
    #[ignore]
    fn crosscheck_real_capture() {
        let Ok(prefix) = std::env::var("DPS_CROSSCHECK") else { return };
        let text = std::fs::read_to_string(format!("{prefix}.messages.txt")).unwrap();
        let mut pools = PeriodicPools::default();
        let mut damage: BTreeMap<String, u64> = BTreeMap::new();
        let mut healing: BTreeMap<String, u64> = BTreeMap::new();
        for line in text.lines() {
            let Some((ts, hex)) = line.split_once(' ') else { continue };
            let at: f64 = ts.parse().unwrap();
            let bytes: Vec<u8> = (0..hex.len()).step_by(2).map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap()).collect();
            let route = if bytes[0] == 0x04 {
                decode_direct(&bytes).map(|hit| route_direct(&hit, |_, _| false)).unwrap_or(Route::Ignore)
            } else {
                decode_periodic(&bytes).map(|tick| pools.route(&tick, at)).unwrap_or(Route::Ignore)
            };
            match route {
                Route::Damage { actor_id, amount } => *damage.entry(actor_id.to_string()).or_default() += amount,
                Route::Healing { actor_id, amount } => *healing.entry(actor_id.to_string()).or_default() += amount,
                Route::Ignore => {}
            }
        }
        std::fs::write(format!("{prefix}.rust.json"), serde_json::json!({"damage": damage, "healing": healing}).to_string()).unwrap();
    }
}
