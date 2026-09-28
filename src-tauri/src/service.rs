//! Local, token protected API for a host that starts the meter as a child process.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, OnceLock, RwLock};
use std::time::Duration;

use serde_json::{json, Value};
use sysinfo::{Pid, ProcessesToUpdate, System};
use tauri::{AppHandle, Manager};
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

use crate::dps_meter::engine::meter::DpsMeter;
use crate::dps_meter::history::HistoryRecord;
use crate::dps_meter::models::combat::{CombatSnapshot, PlayerOverviewStat, SkillStats, TargetInfo};

#[derive(Clone)]
pub struct Options {
    port: u16,
    token: String,
    parent: Option<u32>,
    lang: String,
    no_overlay: bool,
}

impl Options {
    pub fn parse(args: &[String]) -> Result<Option<Self>, String> {
        if !args.iter().any(|arg| arg == "--service") {
            return Ok(None);
        }
        let value = |flag: &str| -> Option<&str> {
            args.iter().position(|arg| arg == flag)
                .and_then(|index| args.get(index + 1))
                .map(String::as_str)
        };
        let port = value("--port")
            .ok_or("--service requires --port")?
            .parse::<u16>()
            .map_err(|_| "--port must be a TCP port from 1 to 65535")?;
        if port == 0 {
            return Err("--port must be a TCP port from 1 to 65535".into());
        }
        let token = value("--token").filter(|v| !v.is_empty() && v.is_ascii() && !v.starts_with("--"))
            .ok_or("--service requires a non-empty ASCII --token")?.to_string();
        let parent_text = if args.iter().any(|arg| arg == "--parent") {
            value("--parent").map(str::to_string).ok_or("--parent must be a PID")?
        } else {
            std::env::var("DBAION2_PARENT_PID").unwrap_or_default()
        };
        let parent = if parent_text.is_empty() { None } else {
            Some(parent_text.parse::<u32>().map_err(|_| "--parent must be a PID")?)
        };
        if parent == Some(0) {
            return Err("--parent must be a nonzero PID".into());
        }
        let lang = value("--lang").unwrap_or("en");
        if lang != "en" && lang != "ru" {
            return Err("--lang must be en or ru".into());
        }
        Ok(Some(Self { port, token, parent, lang: lang.into(),
            no_overlay: args.iter().any(|arg| arg == "--no-overlay") }))
    }
}

#[derive(Clone)]
struct CaptureHealth {
    capture: &'static str,
    message: String,
}

pub fn start(app: AppHandle, options: Options) -> Result<(), Box<dyn std::error::Error>> {
    let server = Server::http(format!("127.0.0.1:{}", options.port))
        .map_err(|error| std::io::Error::other(error.to_string()))?;
    let health = Arc::new(RwLock::new(CaptureHealth { capture: "starting", message: String::new() }));
    let http_app = app.clone();
    let http_options = options.clone();
    let http_health = Arc::clone(&health);
    std::thread::spawn(move || serve(server, http_app, http_options, http_health));

    if let Some(parent) = options.parent {
        let watchdog_app = app.clone();
        std::thread::spawn(move || watch_parent(watchdog_app, parent));
    }

    let meter = app.state::<DpsMeter>();
    *app.state::<crate::plugins::aion2_overlay::LanguageStore>().0.write().unwrap() = options.lang.clone();
    let result = meter.start_dps_meter();
    *health.write().unwrap() = match result {
        Ok(()) => CaptureHealth { capture: "ok", message: String::new() },
        Err(message) => {
            let report = crate::dps_meter::preflight::run();
            let capture = if report.checks.iter().any(|check| {
                check.id == crate::dps_meter::preflight::CheckId::Elevation && !check.ok
            }) { "no-admin" } else { "no-driver" };
            CaptureHealth { capture, message }
        }
    };
    drop(meter);

    if !options.no_overlay && health.read().unwrap().capture == "ok" {
        tauri::async_runtime::spawn(async move {
            if let Err(error) = crate::plugins::aion2_overlay::create_dps_overlay(app).await {
                eprintln!("[service] overlay: {error}");
            }
        });
    }
    Ok(())
}

fn watch_parent(app: AppHandle, parent: u32) {
    let pid = Pid::from_u32(parent);
    let mut system = System::new();
    system.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
    let started = system.process(pid).map(|process| process.start_time());
    loop {
        std::thread::sleep(Duration::from_secs(1));
        system.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
        if system.process(pid).map(|process| process.start_time()) != started || started.is_none() {
            app.exit(0);
            break;
        }
    }
}

fn serve(server: Server, app: AppHandle, options: Options, health: Arc<RwLock<CaptureHealth>>) {
    for request in server.incoming_requests() {
        let authorized = authorized(request.headers(), &options.token);
        let (status, body, quit) = if authorized {
            route(&app, &options.lang, &health, request.method(), request.url())
        } else {
            (403, json!({"error": "forbidden"}), false)
        };
        respond(request, status, body);
        if quit {
            app.exit(0);
            break;
        }
    }
}

fn authorized(headers: &[Header], token: &str) -> bool {
    headers.iter().any(|header| {
            header.field.to_string().eq_ignore_ascii_case("x-dbaion2-token")
                && header.value.as_str() == token
    })
}

fn respond(request: Request, status: u16, body: Value) {
    let bytes = serde_json::to_vec(&body).unwrap_or_else(|_| b"{}".to_vec());
    let response = Response::from_data(bytes)
        .with_status_code(StatusCode(status))
        .with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json; charset=utf-8"[..]).unwrap())
        .with_header(Header::from_bytes(&b"Cache-Control"[..], &b"no-store"[..]).unwrap());
    let _ = request.respond(response);
}

fn route(app: &AppHandle, lang: &str, health: &RwLock<CaptureHealth>, method: &Method, url: &str)
    -> (u16, Value, bool)
{
    let meter = app.state::<DpsMeter>();
    let (path, query) = url.split_once('?').unwrap_or((url, ""));
    match (method, path) {
        (Method::Get, "/v1/health") => {
            let h = health.read().unwrap();
            (200, json!({"version": env!("CARGO_PKG_VERSION"), "capture": h.capture,
                "message": h.message, "game": meter.has_game_traffic(), "pingMs": meter.ping_ms()}), false)
        }
        (Method::Get, "/v1/state") => {
            let owners = meter.summon_owner_ids();
            (200, current_state(meter.get_dps_snapshot(0).as_ref(), &owners, lang), false)
        }
        (Method::Get, "/v1/history") => {
            let limit = query_value(query, "limit").unwrap_or("30").parse::<usize>();
            match limit {
                Ok(limit) => {
                    let mut records = meter.get_history();
                    records.reverse();
                    let rows: Vec<_> = records.iter().take(limit.min(500))
                        .map(|record| history_row(record, lang)).collect();
                    (200, json!(rows), false)
                }
                Err(_) => (400, json!({"error": "invalid limit"}), false),
            }
        }
        (Method::Post, "/v1/reset") => {
            meter.reset_dps_meter(true);
            (200, json!({}), false)
        }
        (Method::Post, "/v1/quit") => (200, json!({}), true),
        (Method::Get, _) if path.starts_with("/v1/history/") => {
            let id = &path[12..];
            match meter.get_history().into_iter().find(|record| record.id == id) {
                Some(record) => (200, record_state(&record, lang), false),
                None => (404, json!({"error": "fight not found"}), false),
            }
        }
        (Method::Get, _) if path.starts_with("/v1/players/") && path.ends_with("/skills") => {
            let id = path.trim_start_matches("/v1/players/").trim_end_matches("/skills");
            match (id.parse::<u32>(), query_value(query, "fight")) {
                (Ok(id), Some(fight)) => match skills_for(&meter, id, fight, lang) {
                    Some(skills) => (200, json!(skills), false),
                    None => (404, json!({"error": "fight not found"}), false),
                },
                _ => (400, json!({"error": "player id and fight are required"}), false),
            }
        }
        _ => (404, json!({"error": "not found"}), false),
    }
}

fn query_value<'a>(query: &'a str, key: &str) -> Option<&'a str> {
    query.split('&').filter_map(|pair| pair.split_once('=')).find(|(k, _)| *k == key).map(|(_, v)| v)
}

fn timing(target: Option<&TargetInfo>, fallback_ms: u64) -> (u64, f64) {
    let started = target.and_then(|t| t.target_start_time.values().copied().reduce(f64::min))
        .map(|s| (s * 1000.0) as u64).unwrap_or(fallback_ms);
    let ended = target.and_then(|t| t.target_last_time.values().copied().reduce(f64::max))
        .unwrap_or(started as f64 / 1000.0);
    (started, (ended - started as f64 / 1000.0).max(0.0))
}

fn target_json(target: Option<&TargetInfo>, lang: &str) -> Value {
    match target {
        Some(t) => json!({"name": target_name(t, lang),
            "npcId": t.target_mob_code, "isBoss": t.target_mob_code.map(|_| t.is_boss),
            "hpPct": match (t.current_hp, t.max_hp) {
                (Some(current), Some(max)) if max > 0 => Some(current as f64 * 100.0 / max as f64),
                _ => None,
            }}),
        None => Value::Null,
    }
}

fn target_name<'a>(target: &'a TargetInfo, lang: &str) -> Option<&'a str> {
    if lang != "en" { return None; }
    let name = target.target_name.as_deref()?;
    if target.target_mob_code.is_some_and(|code| name == format!("Mob {code}") || name == format!("Boss {code}")) {
        None
    } else {
        Some(name)
    }
}

fn class_info(class: &str, lang: &str) -> (Option<u32>, Option<&'static str>) {
    // The parser's job ranges are groups of four: 5..=8 is Gladiator (group 2).
    let (id, en, ru) = match class {
        "GLADIATOR" => (2, "Gladiator", "Гладиатор"),
        "TEMPLAR" => (3, "Templar", "Страж"),
        "ASSASSIN" => (5, "Assassin", "Убийца"),
        "RANGER" => (4, "Ranger", "Лучник"),
        "SORCERER" => (7, "Sorcerer", "Волшебник"),
        "ELEMENTALIST" => (6, "Elementalist", "Заклинатель"),
        "CLERIC" => (8, "Cleric", "Целитель"),
        "CHANTER" => (9, "Chanter", "Чародей"),
        "FIGHTER" => (12, "Fighter", "Боец"),
        _ => return (None, None),
    };
    (Some(id), Some(if lang == "ru" { ru } else { en }))
}

fn player_json(p: &PlayerOverviewStat, self_id: Option<u32>, owners: Option<&HashSet<u32>>, lang: &str) -> Value {
    let (class_id, class_name) = class_info(&p.actor_class, lang);
    json!({"id": p.actor_id, "name": if p.actor_name.is_empty() { None } else { Some(p.actor_name.as_str()) },
        "classId": class_id, "className": class_name, "isSelf": self_id.map(|id| id == p.actor_id),
        "isSummonOwner": owners.map(|ids| ids.contains(&p.actor_id)), "damage": p.total_damage,
        "dps": p.dps, "share": p.damage_share, "hits": p.counts,
        "critRate": (p.counts > 0).then(|| *p.special_counts.get("CRITICAL").unwrap_or(&0) as f64 / p.counts as f64),
        "maxHit": p.max_damage})
}

fn fight_state(id: String, active: bool, target: Option<&TargetInfo>,
    stats: &HashMap<u32, PlayerOverviewStat>, self_id: Option<u32>, owners: Option<&HashSet<u32>>,
    fallback_ms: u64, lang: &str) -> Value
{
    let (started, duration) = timing(target, fallback_ms);
    let total: u64 = stats.values().map(|p| p.total_damage).sum();
    let mut players: Vec<_> = stats.values().collect();
    players.sort_by(|a, b| b.total_damage.cmp(&a.total_damage));
    json!({"fightId": id, "active": active, "startedAt": started,
        "durationSec": duration, "target": target_json(target, lang), "totalDamage": total,
        "players": players.into_iter().map(|p| player_json(p, self_id, owners, lang)).collect::<Vec<_>>()})
}

fn current_target(snapshot: &CombatSnapshot) -> Option<u32> {
    snapshot.combat_infos.last_target_by_main_actor
        .or(snapshot.combat_infos.last_target)
        .filter(|id| snapshot.by_target_player_stats.contains_key(id))
        .or_else(|| snapshot.by_target_player_stats.keys().copied().next())
}

fn current_state(snapshot: Option<&CombatSnapshot>, owners: &HashSet<u32>, lang: &str) -> Value {
    let Some(snapshot) = snapshot else {
        return json!({"fightId": null, "active": false, "startedAt": null,
            "durationSec": 0.0, "target": null, "totalDamage": 0, "players": []});
    };
    let Some(id) = current_target(snapshot) else {
        return json!({"fightId": null, "active": false, "startedAt": null,
            "durationSec": 0.0, "target": null, "totalDamage": 0, "players": []});
    };
    let target = snapshot.combat_infos.target_infos.get(&id);
    let fallback = (snapshot.combat_infos.time_now * 1000.0) as u64;
    let (started, _) = timing(target, fallback);
    fight_state(format!("{id}-{started}"), true, target,
        &snapshot.by_target_player_stats[&id], snapshot.combat_infos.main_actor_id,
        Some(owners), fallback, lang)
}

fn record_state(record: &HistoryRecord, lang: &str) -> Value {
    fight_state(record.id.clone(), false, record.target_info.as_ref(), &record.player_stats,
        record.combat_infos.main_actor_id, record.summon_owner_ids.as_ref(), record.created_at, lang)
}

fn history_row(record: &HistoryRecord, lang: &str) -> Value {
    let (started, duration) = timing(record.target_info.as_ref(), record.created_at);
    let self_stat = record.combat_infos.main_actor_id.and_then(|id| record.player_stats.get(&id));
    json!({"fightId": record.id, "startedAt": started, "durationSec": duration,
        "target": record.target_info.as_ref().and_then(|t| target_name(t, lang)),
        "isBoss": record.target_info.as_ref().and_then(|t| t.target_mob_code.map(|_| t.is_boss)),
        "totalDamage": record.total_damage, "selfDps": self_stat.map(|s| s.dps),
        "selfShare": self_stat.map(|s| s.damage_share)})
}

fn skill_names() -> &'static HashMap<String, String> {
    static NAMES: OnceLock<HashMap<String, String>> = OnceLock::new();
    NAMES.get_or_init(|| serde_json::from_str(include_str!("../../src/i18n/locales/aion2skills/en.json"))
        .unwrap_or_default())
}

fn skill_name(id: u32, lang: &str) -> Option<&'static str> {
    if lang != "en" { return None; }
    let raw = id.to_string();
    let mut keys = vec![raw.clone()];
    if raw.len() > 8 {
        keys.push(raw[..8].to_string());
        keys.push(format!("{}0", &raw[..7]));
    }
    if raw.len() > 6 { keys.push(format!("{}00", &raw[..6])); }
    for key in keys {
        if let Some(name) = skill_names().get(&key) { return Some(name.as_str()); }
    }
    None
}

fn skill_rows(skills: Option<&HashMap<u32, SkillStats>>, lang: &str) -> Vec<Value> {
    let Some(skills) = skills else { return Vec::new(); };
    let total: u64 = skills.values().map(|s| s.total_damage).sum();
    let mut rows: Vec<_> = skills.iter().collect();
    rows.sort_by(|a, b| b.1.total_damage.cmp(&a.1.total_damage));
    rows.into_iter().map(|(id, s)| json!({"skillId": id, "name": skill_name(*id, lang),
        "damage": s.total_damage, "hits": s.counts,
        "critRate": (s.counts > 0).then(|| *s.special_counts.get("CRITICAL").unwrap_or(&0) as f64 / s.counts as f64),
        "maxHit": s.max_damage,
        "share": if total > 0 { s.total_damage as f64 / total as f64 } else { 0.0 }})).collect()
}

fn skills_for(meter: &DpsMeter, player: u32, fight: &str, lang: &str) -> Option<Vec<Value>> {
    if fight == "current" {
        let snapshot = meter.get_dps_snapshot(0)?;
        let target = current_target(&snapshot)?;
        return Some(skill_rows(snapshot.by_target_player_skill_stats.get(&target)
            .and_then(|players| players.get(&player)), lang));
    }
    if let Some(snapshot) = meter.get_dps_snapshot(0) {
        if let Some(target) = current_target(&snapshot) {
            let started = timing(snapshot.combat_infos.target_infos.get(&target),
                (snapshot.combat_infos.time_now * 1000.0) as u64).0;
            if fight == format!("{target}-{started}") {
                return Some(skill_rows(snapshot.by_target_player_skill_stats.get(&target)
                    .and_then(|players| players.get(&player)), lang));
            }
        }
    }
    meter.get_history().into_iter().find(|record| record.id == fight)
        .map(|record| skill_rows(record.player_skill_stats.get(&player), lang))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_contract_basics() {
        let args = ["meter", "--service", "--port", "4040", "--token", "secret"]
            .map(str::to_string);
        let options = Options::parse(&args).unwrap().unwrap();
        assert_eq!(options.port, 4040);
        assert_eq!(options.lang, "en");
        assert!(Options::parse(&["meter", "--service", "--port", "0", "--token", "x"]
            .map(str::to_string)).is_err());
        assert_eq!(query_value("fight=current&limit=2", "fight"), Some("current"));
        assert_eq!(class_info("GLADIATOR", "en"), (Some(2), Some("Gladiator")));
        assert!(skill_name(100001, "en").is_some());
        let header = Header::from_bytes(&b"X-DBAION2-TOKEN"[..], &b"secret"[..]).unwrap();
        assert!(authorized(&[header.clone()], "secret"));
        assert!(!authorized(&[header], "wrong"));
    }
}
