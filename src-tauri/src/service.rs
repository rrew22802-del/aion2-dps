//! Local, token protected API for a host that starts the meter as a child process.

use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::sync::{Arc, OnceLock, RwLock};
use std::time::Duration;

use serde::Deserialize;
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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DetailRequest {
    player_id: u32,
    fight: String,
    x: Option<i32>,
    y: Option<i32>,
}

fn parse_detail_request(body: &str) -> Option<(u32, String, Option<(i32, i32)>)> {
    let request: DetailRequest = serde_json::from_str(body).ok()?;
    if request.player_id == 0 || request.fight.is_empty() { return None; }
    let position = match (request.x, request.y) {
        (None, None) => None,
        (Some(x), Some(y)) => Some((x, y)),
        _ => return None,
    };
    Some((request.player_id, request.fight, position))
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
    let mut watch_names = Vec::new();
    for mut request in server.incoming_requests() {
        let authorized = authorized(request.headers(), &options.token);
        if authorized && request.method() == &Method::Get
            && request.url().split('?').next().unwrap_or("").starts_with("/v1/assets/") {
            respond_asset(request, &app);
            continue;
        }
        let (status, body, quit) = if authorized {
            let mut body = String::new();
            let read = request.as_reader().take(16_385).read_to_string(&mut body);
            if read.is_err() || body.len() > 16_384 {
                (400, json!({"error": "invalid body"}), false)
            } else {
                route(&app, &options.lang, &health, request.method(), request.url(), &body, &mut watch_names)
            }
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

fn asset_path(url: &str) -> Option<&str> {
    let path = url.split('?').next()?.strip_prefix("/v1/assets/")?;
    let name = path.strip_prefix("aion2/class/")
        .or_else(|| path.strip_prefix("aion2/skill/"))?
        .strip_suffix(".png")?;
    if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-') {
        return None;
    }
    Some(path)
}

fn respond_asset(request: Request, app: &AppHandle) {
    let asset = asset_path(request.url()).and_then(|path| app.asset_resolver().get(path.into()))
        .filter(|asset| asset.mime_type() == "image/png");
    if let Some(asset) = asset {
        let response = Response::from_data(asset.bytes)
            .with_header(Header::from_bytes(&b"Content-Type"[..], &b"image/png"[..]).unwrap())
            .with_header(Header::from_bytes(&b"Cache-Control"[..], &b"max-age=86400"[..]).unwrap());
        let _ = request.respond(response);
    } else {
        respond(request, 404, json!({"error": "not found"}));
    }
}

fn route(app: &AppHandle, lang: &str, health: &RwLock<CaptureHealth>, method: &Method, url: &str,
    body: &str, watch_names: &mut Vec<String>)
    -> (u16, Value, bool)
{
    let meter = app.state::<DpsMeter>();
    let (path, query) = url.split_once('?').unwrap_or((url, ""));
    match (method, path) {
        (Method::Get, "/v1/live") => {
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_millis() as u64)
                .unwrap_or_default();
            let pinned = query_value(query, "target").and_then(|value| value.parse::<u32>().ok());
            let (self_id, target, buffs, debuffs, casts) = meter.live_combat_assist_snapshot(pinned, now_ms);
            let buff_rows = |rows: Vec<(u32, u32, u32, u64, u64)>| rows.into_iter().map(
                |(_, actor, skill, start_ms, end_ms)| json!({
                    "skill": skill, "name": skill_name(skill, lang).unwrap_or(""),
                    "icon": skill_icon(skill), "actor": actor, "fromSelf": self_id == Some(actor),
                    "startMs": start_ms, "endMs": end_ms,
                    "durationMs": end_ms.saturating_sub(start_ms)
                }),
            ).collect::<Vec<_>>();
            let target_json = target.map(|(id, mob_code, hp, is_boss)| {
                let hp_pct = hp.and_then(|(current, max)| (max > 0).then(|| current as f64 * 100.0 / max as f64));
                let name = if lang == "en" { mob_code.and_then(|code| meter.mob_name(code)) } else { None };
                json!({"id": id, "name": name, "hpPct": hp_pct, "isBoss": is_boss})
            });
            let cast_rows: Vec<_> = casts.into_iter().map(|(skill, last_ms, count)| json!({
                "skill": skill, "name": skill_name(skill, lang).unwrap_or(""),
                "icon": skill_icon(skill), "lastMs": last_ms, "count": count
            })).collect();
            (200, json!({"now": now_ms, "selfId": self_id, "buffs": buff_rows(buffs),
                "debuffs": buff_rows(debuffs), "casts": cast_rows, "target": target_json}), false)
        }
        (Method::Get, "/v1/health") => {
            let h = health.read().unwrap();
            (200, json!({"version": env!("CARGO_PKG_VERSION"), "capture": h.capture,
                "message": h.message, "game": meter.has_game_traffic(), "pingMs": meter.ping_ms()}), false)
        }
        (Method::Get, "/v1/state") => {
            // ?target=<id> pins one target of the session (owner 05.10: look at the boss alone, without resetting before it)
            let owners = meter.summon_owner_ids();
            let pinned = query_value(query, "target").and_then(|v| v.parse::<u32>().ok());
            let party = meter.party_snapshot();
            (200, current_state(meter.get_dps_snapshot(0).as_ref(), &owners, &party.member_ids, pinned, lang), false)
        }
        (Method::Get, "/v1/party") => {
            let party = meter.party_snapshot();
            (200, json!({"members": party.members, "updatedAt": party.updated_at}), false)
        }
        (Method::Get, "/v1/targets") => (200, targets_json(meter.get_dps_snapshot(0).as_ref(), lang), false),
        (Method::Get, "/v1/field-bosses") => {
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_millis() as u64)
                .unwrap_or_default();
            let timers: Vec<_> = meter.field_boss_timer_snapshot().into_iter().map(
                |(map_id, npc_id, spawn_at_ms, last_seen_ms)| {
                    let name = if lang == "ru" {
                        None
                    } else {
                        meter.mob_name(npc_id).filter(|name| {
                            name != &format!("Mob {npc_id}") && name != &format!("Boss {npc_id}")
                        })
                    };
                    json!({
                        "npcId": npc_id,
                        "name": name,
                        "mapId": (map_id != 0).then_some(map_id),
                        "spawnAt": spawn_at_ms,
                        "state": if spawn_at_ms > now_ms { "respawn" } else { "alive" },
                        "lastSeen": last_seen_ms
                    })
                },
            ).collect();
            (200, json!(timers), false)
        }
        (Method::Get, "/v1/history") => {
            let limit = query_value(query, "limit").unwrap_or("30").parse::<usize>();
            match limit {
                Ok(limit) => {
                    let mut records = meter.get_history();
                    records.reverse();
                    let party_ids = meter.party_snapshot().member_ids;
                    let rows: Vec<_> = records.iter().take(limit.min(500))
                        .map(|record| history_row(record, &party_ids, lang)).collect();
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
        (Method::Post, "/v1/ui/close") => {
            let result = app.get_webview_window("dps-detail")
                .map(|window| window.hide().map_err(|error| error.to_string()))
                .unwrap_or(Ok(()));
            match result {
                Ok(()) => (200, json!({}), false),
                Err(error) => (500, json!({"error": error}), false),
            }
        }
        (Method::Post, "/v1/ui/player-detail") => {
            match parse_detail_request(body) {
                Some((player, fight, position)) => match selected_fight(&meter, &fight) {
                    Some(selected) if player_detail(&selected, player, &meter.party_snapshot().member_ids, lang).is_some() => {
                        let selection = match selected {
                            SelectedFight::Current(_, target, _) => json!({"actorId": player, "targetId": target, "mode": "live"}),
                            SelectedFight::History(record) => json!({"actorId": player, "mode": "history", "record": record}),
                        };
                        let result = crate::plugins::aion2_overlay::set_detail_selection(
                            (*app).clone(), app.state(), selection)
                            .and_then(|_| crate::plugins::aion2_overlay::show_dps_detail(app, position, true));
                        match result {
                            Ok(()) => (200, json!({}), false),
                            Err(error) => (500, json!({"error": error}), false),
                        }
                    }
                    Some(_) => (404, json!({"error": "player not found"}), false),
                    None => (404, json!({"error": "fight not found"}), false),
                },
                None => (400, json!({"error": "playerId, fight, and paired integer x/y are required"}), false),
            }
        }
        (Method::Get, "/v1/pvp") => {
            let snapshot = meter.get_dps_snapshot(0);
            let known_players = meter.get_pvp_watch_info(&[]).known_players;
            let mut stats: Vec<_> = meter.get_pvp_combat_stats().into_iter().map(|row| {
                let class = known_players.iter().find(|p| p.actor_name == row.actor_name
                    && p.server_id.as_deref().unwrap_or("") == row.server_id.as_str());
                let class_id = class.as_ref().and_then(|p| p.actor_class.as_deref()).and_then(|c| class_info(c, lang).0);
                json!({"name": row.actor_name, "serverId": row.server_id, "classId": class_id,
                    "damage": row.damage, "kills": row.kills, "assists": row.assists, "deaths": row.deaths})
            }).collect();
            stats.sort_by(|a, b| b["damage"].as_u64().cmp(&a["damage"].as_u64()));
            (200, json!({"players": stats,
                "damageDealt": snapshot.as_ref().map(|s| &s.main_actor_dealt_player_overview_stats).cloned().unwrap_or_default(),
                "damageTaken": snapshot.as_ref().map(|s| &s.main_actor_received_player_overview_stats).cloned().unwrap_or_default()}), false)
        }
        (Method::Post, "/v1/pvp/clear") => {
            meter.clear_pvp_combat_stats();
            (200, json!({}), false)
        }
        (Method::Get, "/v1/pvp/watch") => {
            let info = meter.get_pvp_watch_info(watch_names);
            let watched: Vec<_> = info.watch_info.into_iter().map(|p| {
                let hp_pct = match (p.current_hp, p.max_hp) {
                    (Some(current), Some(max)) if max > 0 => Some(current as f64 * 100.0 / max as f64),
                    _ => None,
                };
                json!({"name": p.query_name, "actorId": p.actor_id, "actorName": p.actor_name,
                    "classId": p.actor_class.as_deref().and_then(|c| class_info(c, lang).0),
                    "currentHp": p.current_hp, "maxHp": p.max_hp, "hpPct": hp_pct})
            }).collect();
            (200, json!({"names": watch_names, "players": watched}), false)
        }
        (Method::Post | Method::Delete, "/v1/pvp/watch") => {
            let name = serde_json::from_str::<Value>(body).ok()
                .and_then(|v| v.get("name").and_then(Value::as_str).map(str::to_string));
            match name.map(|n| n.trim().to_string()).filter(|n| !n.is_empty() && n.len() <= 128) {
                Some(name) => {
                    if method == &Method::Post && !watch_names.contains(&name) { watch_names.push(name); }
                    else if method == &Method::Delete { watch_names.retain(|n| n != &name); }
                    (200, json!({"names": watch_names}), false)
                }
                None => (400, json!({"error": "valid name required"}), false),
            }
        }
        (Method::Get, _) if path.starts_with("/v1/history/") => {
            let id = &path[12..];
            match meter.get_history().into_iter().find(|record| record.id == id) {
                Some(record) => (200, record_state(&record, &meter.party_snapshot().member_ids, lang), false),
                None => (404, json!({"error": "fight not found"}), false),
            }
        }
        (Method::Get, _) if path.starts_with("/v1/fights/") && path.ends_with("/timeline") => {
            let fight = path.trim_start_matches("/v1/fights/").trim_end_matches("/timeline").trim_end_matches('/');
            match (fight, query_value(query, "player").and_then(|id| id.parse::<u32>().ok())) {
                (fight, Some(player)) if !fight.is_empty() => match selected_fight(&meter, fight) {
                    Some(selected) => match fight_timeline(&selected, player) {
                        Some(timeline) => (200, timeline, false),
                        None => (404, json!({"error":"player not found"}), false),
                    },
                    None => (404, json!({"error":"fight not found"}), false),
                },
                _ => (400, json!({"error":"player is required"}), false),
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
        (Method::Get, _) if path.starts_with("/v1/players/") && path.ends_with("/buffs") => {
            let id = path.trim_start_matches("/v1/players/").trim_end_matches("/buffs");
            match (id.parse::<u32>(), query_value(query, "fight")) {
                (Ok(id), Some(fight)) => match selected_fight(&meter, fight) {
                    Some(selected) => (200, buffs_for(&selected, id, lang), false),
                    None => (404, json!({"error": "fight not found"}), false),
                },
                _ => (400, json!({"error": "player id and fight are required"}), false),
            }
        }
        (Method::Get, _) if path.starts_with("/v1/players/") => {
            let id = path.trim_start_matches("/v1/players/");
            match (id.parse::<u32>(), query_value(query, "fight")) {
                (Ok(id), Some(fight)) => match selected_fight(&meter, fight) {
                    Some(selected) => match player_detail(&selected, id, &meter.party_snapshot().member_ids, lang) {
                        Some(player) => (200, player, false),
                        None => (404, json!({"error": "player not found"}), false),
                    },
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
        "RANGER" => (4, "Ranger", "Стрелок"),
        "SORCERER" => (7, "Sorcerer", "Волшебник"),
        "ELEMENTALIST" => (6, "Elementalist", "Заклинатель"),
        "CLERIC" => (8, "Cleric", "Целитель"),
        "CHANTER" => (9, "Chanter", "Чародей"),
        "FIGHTER" => (12, "Fighter", "Боец"),
        _ => return (None, None),
    };
    (Some(id), Some(if lang == "ru" { ru } else { en }))
}

fn player_json(p: &PlayerOverviewStat, self_id: Option<u32>, owners: Option<&HashSet<u32>>,
    party_ids: &HashSet<u32>, lang: &str) -> Value {
    let (class_id, class_name) = class_info(&p.actor_class, lang);
    let class_icon = class_id.map(|_| format!("aion2/class/{}.png", p.actor_class.to_ascii_lowercase()));
    json!({"id": p.actor_id, "name": if p.actor_name.is_empty() { None } else { Some(p.actor_name.as_str()) },
        "classId": class_id, "className": class_name, "classIcon": class_icon,
        "isSelf": self_id.map(|id| id == p.actor_id),
        "inParty": self_id == Some(p.actor_id) || party_ids.contains(&p.actor_id),
        "isSummonOwner": owners.map(|ids| ids.contains(&p.actor_id)), "damage": p.total_damage,
        "dps": p.dps, "share": p.damage_share, "hits": p.counts,
        "critRate": (p.counts > 0).then(|| *p.special_counts.get("CRITICAL").unwrap_or(&0) as f64 / p.counts as f64),
        "maxHit": p.max_damage, "cp": p.combat_power, "deaths": p.deaths,
        "healTotal": p.heal_total, "hps": p.hps})
}

fn fight_state(id: String, active: bool, target: Option<&TargetInfo>,
    stats: &HashMap<u32, PlayerOverviewStat>, self_id: Option<u32>, owners: Option<&HashSet<u32>>,
    party_ids: &HashSet<u32>, fallback_ms: u64, lang: &str) -> Value
{
    let (started, duration) = timing(target, fallback_ms);
    let total: u64 = stats.values().map(|p| p.total_damage).sum();
    let mut players: Vec<_> = stats.values().collect();
    players.sort_by(|a, b| b.total_damage.cmp(&a.total_damage));
    json!({"fightId": id, "active": active, "startedAt": started,
        "durationSec": duration, "target": target_json(target, lang), "totalDamage": total,
        "players": players.into_iter().map(|p| player_json(p, self_id, owners, party_ids, lang)).collect::<Vec<_>>()})
}

fn current_target(snapshot: &CombatSnapshot) -> Option<u32> {
    // once we know our character, only its own fights count: falling back to anyone's last target showed other
    // players' fights as ours whenever we stood still (owner, Global 01.10)
    if snapshot.combat_infos.main_actor_id.is_some() {
        return snapshot.combat_infos.last_target_by_main_actor
            .filter(|id| snapshot.by_target_player_stats.contains_key(id));
    }
    snapshot.combat_infos.last_target_by_main_actor
        .or(snapshot.combat_infos.last_target)
        .filter(|id| snapshot.by_target_player_stats.contains_key(id))
        .or_else(|| snapshot.by_target_player_stats.keys().copied().next())
}

// every target of the current session: bosses first, then the latest fights; the tracker shows them as chips
fn targets_json(snapshot: Option<&CombatSnapshot>, lang: &str) -> Value {
    let Some(snapshot) = snapshot else { return json!([]); };
    let current = current_target(snapshot);
    let fallback = (snapshot.combat_infos.time_now * 1000.0) as u64;
    let mut rows: Vec<(bool, f64, Value)> = snapshot.by_target_player_stats.iter().map(|(id, stats)| {
        let target = snapshot.combat_infos.target_infos.get(id);
        let (started, duration) = timing(target, fallback);
        let last = target.and_then(|t| t.target_last_time.values().copied().reduce(f64::max)).unwrap_or(0.0);
        let boss = target.is_some_and(|t| t.is_boss && t.target_mob_code.is_some());
        let own = snapshot.combat_infos.main_actor_id.and_then(|me| stats.get(&me)).map(|p| p.total_damage);
        (boss, last, json!({"id": id, "target": target_json(target, lang), "isBoss": boss, "current": current == Some(*id),
            "startedAt": started, "durationSec": duration, "lastHitAt": (last * 1000.0) as u64,
            "totalDamage": stats.values().map(|p| p.total_damage).sum::<u64>(), "ownDamage": own}))
    }).collect();
    rows.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.total_cmp(&a.1)));
    json!(rows.into_iter().take(20).map(|r| r.2).collect::<Vec<_>>())
}

fn current_state(snapshot: Option<&CombatSnapshot>, owners: &HashSet<u32>, party_ids: &HashSet<u32>, pinned: Option<u32>, lang: &str) -> Value {
    let Some(snapshot) = snapshot else {
        return json!({"fightId": null, "active": false, "startedAt": null,
            "durationSec": 0.0, "target": null, "totalDamage": 0, "players": []});
    };
    let Some(id) = pinned.filter(|id| snapshot.by_target_player_stats.contains_key(id)).or_else(|| current_target(snapshot)) else {
        return json!({"fightId": null, "active": false, "startedAt": null,
            "durationSec": 0.0, "target": null, "totalDamage": 0, "players": []});
    };
    let target = snapshot.combat_infos.target_infos.get(&id);
    let fallback = (snapshot.combat_infos.time_now * 1000.0) as u64;
    let (started, _) = timing(target, fallback);
    let mut state = fight_state(format!("{id}-{started}"), true, target,
        &snapshot.by_target_player_stats[&id], snapshot.combat_infos.main_actor_id,
        Some(owners), party_ids, fallback, lang);
    let boss = snapshot.by_target_player_stats.keys()
        .filter_map(|tid| snapshot.combat_infos.target_infos.get(tid).filter(|t| t.is_boss && t.target_mob_code.is_some()))
        .max_by(|a, b| {
            let last = |t: &TargetInfo| t.target_last_time.values().copied().reduce(f64::max).unwrap_or(0.0);
            last(a).total_cmp(&last(b))
        });
    state["targetId"] = json!(id);
    state["pinned"] = json!(pinned == Some(id));
    state["boss"] = boss.map(|t| json!({"id": t.id, "target": target_json(Some(t), lang)})).unwrap_or(Value::Null);
    state
}

fn record_state(record: &HistoryRecord, current_party_ids: &HashSet<u32>, lang: &str) -> Value {
    let party_ids = record.party_member_ids.as_ref().unwrap_or(current_party_ids);
    fight_state(record.id.clone(), false, record.target_info.as_ref(), &record.player_stats,
        record.combat_infos.main_actor_id, record.summon_owner_ids.as_ref(), party_ids, record.created_at, lang)
}

fn history_row(record: &HistoryRecord, current_party_ids: &HashSet<u32>, lang: &str) -> Value {
    let (started, duration) = timing(record.target_info.as_ref(), record.created_at);
    let self_stat = record.combat_infos.main_actor_id.and_then(|id| record.player_stats.get(&id));
    let party_ids = record.party_member_ids.as_ref().unwrap_or(current_party_ids);
    let mut players: Vec<_> = record.player_stats.values().collect();
    players.sort_by(|a, b| b.total_damage.cmp(&a.total_damage));
    json!({"fightId": record.id, "startedAt": started, "durationSec": duration,
        "target": record.target_info.as_ref().and_then(|t| target_name(t, lang)),
        "isBoss": record.target_info.as_ref().and_then(|t| t.target_mob_code.map(|_| t.is_boss)),
        "totalDamage": record.total_damage, "selfDps": self_stat.map(|s| s.dps),
        "selfShare": self_stat.map(|s| s.damage_share),
        "healTotal": record.player_stats.values().map(|p| p.heal_total).sum::<u64>(),
        "hps": self_stat.map(|s| s.hps), "deaths": record.player_stats.values().map(|p| p.deaths).sum::<u32>(),
        "selfCp": self_stat.and_then(|s| s.combat_power), "selfDeaths": self_stat.map(|s| s.deaths),
        "players": players.into_iter().map(|player| player_json(player, record.combat_infos.main_actor_id,
            record.summon_owner_ids.as_ref(), party_ids, lang)).collect::<Vec<_>>()})
}

fn fight_timeline(fight: &SelectedFight, player: u32) -> Option<Value> {
    let (target, events, intervals, duration, exists) = match fight {
        SelectedFight::Current(snapshot, target, _) => {
            let target_info = snapshot.combat_infos.target_infos.get(target);
            let events = snapshot.combat_events.iter().filter(|event| event.target_id == *target).cloned().collect::<Vec<_>>();
            let duration = target_info.and_then(|t| t.target_last_time.values().copied().reduce(f64::max).zip(t.target_start_time.values().copied().reduce(f64::min))).map(|(end,start)| ((end-start).max(1.0)*1000.0) as u64).unwrap_or(0);
            (*target, events, snapshot.buff_intervals.clone(), duration, snapshot.by_target_player_stats.get(target).is_some_and(|stats| stats.contains_key(&player)))
        }
        SelectedFight::History(record) => {
            let start = record.target_info.as_ref().and_then(|t| t.target_start_time.values().copied().reduce(f64::min)).unwrap_or(0.0);
            let end = record.target_info.as_ref().and_then(|t| t.target_last_time.values().copied().reduce(f64::max)).unwrap_or(start);
            (record.target_id, record.combat_events.clone(), record.buff_intervals.clone(), ((end-start).max(1.0)*1000.0) as u64, record.player_stats.contains_key(&player))
        }
    };
    if !exists { return None; }
    let start_ms = match fight {
        SelectedFight::Current(snapshot, _, _) => snapshot.combat_infos.target_infos.get(&target).and_then(|t| t.target_start_time.values().copied().reduce(f64::min)).unwrap_or(0.0),
        SelectedFight::History(record) => record.target_info.as_ref().and_then(|t| t.target_start_time.values().copied().reduce(f64::min)).unwrap_or(0.0),
    } * 1000.0;
    let duration_ms = duration.min(1_200_000);
    let seconds = ((duration_ms + 999) / 1000) as usize;
    let mut buckets = vec![(0u64, 0u64); seconds];
    let casts: Vec<_> = events.iter().filter_map(|event| {
        let offset = (event.at_ms as f64 - start_ms).max(0.0) as u64;
        if offset > duration_ms { return None; }
        let bucket = (offset / 1000) as usize;
        if bucket < buckets.len() { buckets[bucket].1 = buckets[bucket].1.saturating_add(event.damage); if event.actor_id == player { buckets[bucket].0 = buckets[bucket].0.saturating_add(event.damage); } }
        (event.actor_id == player).then_some(json!({"offsetMs":offset,"skillCode":event.skill_code,"damage":event.damage,"crit":event.is_crit}))
    }).take(25_000).collect();
    let per_second: Vec<_> = buckets.into_iter().enumerate().map(|(second,(own,group))| json!({"offsetMs":second*1000,"playerDamage":own,"groupDamage":group})).collect();
    let own_buffs = intervals.get(&player).map(|actors| {
        let mut rows_out = Vec::new();
        for (actor, skills) in actors {
            for (skill, rows) in skills {
                for row in rows {
                    let start = row.start_ms.saturating_sub(start_ms as u64);
                    if start > duration_ms { continue; }
                    let end = row.end_ms.saturating_sub(start_ms as u64).min(duration_ms);
                    if end >= start { rows_out.push(json!({"startOffsetMs":start,"endOffsetMs":end,"skillCode":skill,"sourceId":actor})); }
                }
            }
        }
        rows_out
    }).unwrap_or_default();
    Some(json!({"durationMs":duration_ms,"damagePerSecond":per_second,"casts":casts,"buffs":own_buffs}))
}

fn skill_names() -> &'static HashMap<String, String> {
    static NAMES: OnceLock<HashMap<String, String>> = OnceLock::new();
    NAMES.get_or_init(|| serde_json::from_str(include_str!("../../src/i18n/locales/aion2skills/en.json"))
        .unwrap_or_default())
}

fn skill_key(id: u32) -> Option<String> {
    let raw = id.to_string();
    let mut keys = vec![raw.clone()];
    if raw.len() > 8 {
        keys.push(raw[..8].to_string());
        keys.push(format!("{}0", &raw[..7]));
    }
    if raw.len() > 6 { keys.push(format!("{}00", &raw[..6])); }
    for key in keys {
        if skill_names().contains_key(&key) { return Some(key); }
    }
    None
}

fn skill_names_ru() -> &'static HashMap<String, String> {
    static NAMES: OnceLock<HashMap<String, String>> = OnceLock::new();
    NAMES.get_or_init(|| serde_json::from_str(include_str!("../../src/i18n/locales/aion2skills/ru.json"))
        .unwrap_or_default())
}

// ru.json has the same keys as en.json (untranslated entries keep the English name)
fn skill_name(id: u32, lang: &str) -> Option<&'static str> {
    let names = match lang { "en" => skill_names(), "ru" => skill_names_ru(), _ => return None };
    skill_key(id).and_then(|key| names.get(&key).map(String::as_str))
}

fn skill_icon(id: u32) -> String {
    let key = skill_key(id).unwrap_or_else(|| id.to_string().chars().take(8).collect());
    let name = if key.len() == 6 { key } else { key.chars().take(4).collect() };
    format!("aion2/skill/{name}.png")
}

fn rate(specials: &HashMap<String, u32>, hits: u32, key: &str) -> Option<f64> {
    (hits > 0).then(|| *specials.get(key).unwrap_or(&0) as f64 / hits as f64)
}

enum SelectedFight {
    Current(CombatSnapshot, u32, HashSet<u32>),
    History(HistoryRecord),
}

fn selected_fight(meter: &DpsMeter, fight: &str) -> Option<SelectedFight> {
    if let Some(snapshot) = meter.get_dps_snapshot(0) {
        if let Some(target) = current_target(&snapshot) {
            let started = timing(snapshot.combat_infos.target_infos.get(&target),
                (snapshot.combat_infos.time_now * 1000.0) as u64).0;
            if fight == "current" || fight == format!("{target}-{started}") {
                return Some(SelectedFight::Current(snapshot, target, meter.summon_owner_ids()));
            }
        }
    }
    if fight == "current" { return None; }
    meter.get_history().into_iter().find(|record| record.id == fight).map(SelectedFight::History)
}

fn skill_rows(skills: Option<&HashMap<u32, SkillStats>>, specs: Option<&HashMap<u32, Vec<u32>>>, lang: &str) -> Vec<Value> {
    let Some(skills) = skills else { return Vec::new(); };
    let total: u64 = skills.values().map(|s| s.total_damage).sum();
    let mut rows: Vec<_> = skills.iter().collect();
    rows.sort_by(|a, b| b.1.total_damage.cmp(&a.1.total_damage));
    rows.into_iter().map(|(id, s)| json!({"skillId": id, "name": skill_name(*id, lang),
        "icon": skill_icon(*id), "spec": specs.and_then(|map| map.get(id)).cloned().unwrap_or_default(),
        "damage": s.total_damage, "hits": s.counts,
        "critRate": rate(&s.special_counts, s.counts, "CRITICAL"),
        "perfectRate": rate(&s.special_counts, s.counts, "PERFECT"),
        "doubleRate": rate(&s.special_counts, s.counts, "DOUBLE"),
        "frontRate": rate(&s.special_counts, s.counts, "FRONT"),
        "backRate": rate(&s.special_counts, s.counts, "BACK"),
        "parryRate": rate(&s.special_counts, s.counts, "PARRY"),
        "multiRate": rate(&s.special_counts, s.counts, "MULTIHIT"),
        "multiDamage": s.special_counts.get("MULTIHITDMG").copied().unwrap_or(0),
        "minHit": (s.counts > 0).then_some(s.min_damage),
        "avgHit": (s.counts > 0).then(|| s.total_damage / s.counts as u64),
        "maxHit": s.max_damage,
        "share": if total > 0 { s.total_damage as f64 / total as f64 } else { 0.0 }})).collect()
}

fn skills_for(meter: &DpsMeter, player: u32, fight: &str, lang: &str) -> Option<Vec<Value>> {
    let selected = selected_fight(meter, fight)?;
    Some(match &selected {
        SelectedFight::Current(snapshot, target, _) => skill_rows(snapshot.by_target_player_skill_stats.get(target)
            .and_then(|players| players.get(&player)), snapshot.combat_infos.actor_infos.get(&player)
            .map(|actor| &actor.actor_skill_spec), lang),
        SelectedFight::History(record) => skill_rows(record.player_skill_stats.get(&player),
            record.combat_infos.actor_infos.get(&player).map(|actor| &actor.actor_skill_spec), lang),
    })
}

fn player_detail(fight: &SelectedFight, player: u32, party_ids: &HashSet<u32>, lang: &str) -> Option<Value> {
    let (stats, target, self_id, owners, selected_party_ids) = match fight {
        SelectedFight::Current(snapshot, id, owners) => (
            snapshot.by_target_player_stats.get(id)?.get(&player)?,
            snapshot.combat_infos.target_infos.get(id), snapshot.combat_infos.main_actor_id, Some(owners), party_ids),
        SelectedFight::History(record) => (
            record.player_stats.get(&player)?, record.target_info.as_ref(),
            record.combat_infos.main_actor_id, record.summon_owner_ids.as_ref(),
            record.party_member_ids.as_ref().unwrap_or(party_ids)),
    };
    let mut value = player_json(stats, self_id, owners, selected_party_ids, lang);
    let start = target.and_then(|t| t.target_start_time.get(&player)).copied();
    let end = target.and_then(|t| t.target_last_time.get(&player)).copied();
    let fight_sec = start.zip(end).map(|(a, b)| (b - a).max(1.0));
    let object = value.as_object_mut()?;
    object.insert("fightSec".into(), json!(fight_sec));
    if let Some(seconds) = fight_sec {
        object.insert("dps".into(), json!(stats.total_damage as f64 / seconds));
    }
    for (field, key) in [("backRate", "BACK"), ("frontRate", "FRONT"),
        ("doubleRate", "DOUBLE"), ("perfectRate", "PERFECT"),
        ("parryRate", "PARRY"), ("multiRate", "MULTIHIT")] {
        object.insert(field.into(), json!(rate(&stats.special_counts, stats.counts, key)));
    }
    Some(value)
}

fn buffs_for(fight: &SelectedFight, player: u32, lang: &str) -> Value {
    let (buffs, actors, boss) = match fight {
        SelectedFight::Current(snapshot, target, _) => (&snapshot.use_buffs_by_target,
            &snapshot.combat_infos.actor_infos, *target),
        SelectedFight::History(record) => (&record.use_buffs_by_target,
            &record.combat_infos.actor_infos, record.target_id),
    };
    let rows = |target| -> Vec<Value> {
        buffs.get(&target).into_iter().flatten().map(|buff| json!({
            "id": buff.skill_code, "name": skill_name(buff.skill_code, lang),
            "icon": skill_icon(buff.skill_code), "uptime": buff.coverage,
            "source": actors.get(&buff.actor_id).and_then(|actor| actor.actor_name.as_deref()),
        })).collect()
    };
    json!({"player": rows(player), "boss": rows(boss)})
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
        assert_eq!(asset_path("/v1/assets/aion2/class/gladiator.png"), Some("aion2/class/gladiator.png"));
        assert_eq!(asset_path("/v1/assets/aion2/skill/1101.png?x=1"), Some("aion2/skill/1101.png"));
        for path in ["aion2/class/../skill/1101.png", "aion2/skill/%2e%2e.png", "other/1101.png", "aion2/skill/no.svg"] {
            assert_eq!(asset_path(&format!("/v1/assets/{path}")), None);
        }
        assert_eq!(skill_icon(11_000_000), "aion2/skill/1100.png");
        assert!(skill_name(100001, "en").is_some());
        let header = Header::from_bytes(&b"X-DBAION2-TOKEN"[..], &b"secret"[..]).unwrap();
        assert!(authorized(&[header.clone()], "secret"));
        assert!(!authorized(&[header], "wrong"));
        let skills = HashMap::from([(11_000_000, SkillStats {
            counts: 2, total_damage: 300, min_damage: 100, max_damage: 200,
            special_counts: HashMap::from([("PERFECT".into(), 1), ("MULTIHITDMG".into(), 40)]),
        })]);
        let rows = skill_rows(Some(&skills), None, "en");
        assert_eq!(rows[0]["perfectRate"], json!(0.5));
        assert_eq!(rows[0]["multiDamage"], json!(40));
        assert_eq!(rows[0]["avgHit"], json!(150));
        assert_eq!(parse_detail_request(r#"{"playerId":7,"fight":"current","x":-50,"y":20}"#),
            Some((7, "current".into(), Some((-50, 20)))));
        assert!(parse_detail_request(r#"{"playerId":7,"fight":"current","x":20}"#).is_none());
    }
}
