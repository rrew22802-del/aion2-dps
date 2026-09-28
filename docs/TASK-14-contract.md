# TASK 14 — service mode + local API (so a host app can show the meter in its own UI)

A host application starts this meter as a separate background process and reads its numbers over a local HTTP API on 127.0.0.1.
The meter stays GPL-3.0; the host only talks to it over the socket.

## API contract v1 (both sides implement exactly this)
DPS started as: `"DBAion2 DPS.exe" --service --port <p> --token <t> [--lang ru|en] [--no-overlay]`
- `--service`: no main window, no splash/gate window, no tray icon; capture starts at once (the checks that the gate did —
  capture driver, admin rights — are reported in `/v1/health` instead). In-game overlay windows keep working as upstream
  unless `--no-overlay`.
- Server binds 127.0.0.1 only; every request must carry header `x-dbaion2-token: <t>`, else 403. Exit when the parent process
  (the tracker, pid from env `DBAION2_PARENT_PID` or `--parent <pid>`) is gone, or on `POST /v1/quit`.
- JSON, UTF-8, numbers as numbers. Poll-friendly (the tracker polls every 250 ms).

`GET /v1/health` → `{ "version": "…", "capture": "ok"|"no-driver"|"no-admin"|"starting", "message": "…", "game": true|false, "pingMs": 123|null }`
`GET /v1/state` → current fight
```
{ "fightId": "…", "active": true, "startedAt": 1790…, "durationSec": 42.5,
  "target": { "name": "…", "npcId": 2400017, "isBoss": true, "hpPct": 37.5 } | null,
  "totalDamage": 123456,
  "players": [ { "id": 16450, "name": "…", "classId": 2, "className": "Gladiator", "isSelf": true, "isSummonOwner": false,
                 "damage": 34567, "dps": 812.3, "share": 0.28, "hits": 140, "critRate": 0.31, "maxHit": 5120 } ] }
```
`GET /v1/players/{id}/skills?fight=<fightId|current>` → `[ { "skillId": 11020, "name": "…", "damage": 1234, "hits": 12, "critRate": 0.25, "maxHit": 300, "share": 0.12 } ]`
`GET /v1/history?limit=30` → `[ { "fightId": "…", "startedAt": …, "durationSec": …, "target": "…", "isBoss": …, "totalDamage": …, "selfDps": …, "selfShare": … } ]`
`GET /v1/history/{fightId}` → same shape as `/v1/state` for that fight
`POST /v1/reset` → clears the current fight; `POST /v1/quit` → exits
Names/skill names in the requested language (upstream's i18n/data). Unknown values → null, never omitted.

## Implement the service mode + the API in Rust (src-tauri), reusing the
existing engine (dps_meter/engine, history, personal_best, ping_tracker). Small HTTP server on a background thread (a light
crate, e.g. tiny_http), no new UI. No Rust toolchain on this PC: write carefully, CI (`.github/workflows/build.yml`) compiles
it — keep changes minimal and typed. Document the API in docs/SERVICE-API.md.


