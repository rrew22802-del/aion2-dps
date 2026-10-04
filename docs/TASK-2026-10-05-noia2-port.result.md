# NOIA2 4.x port result (2026-10-05)

## Ported: local field-boss timer capture

- Added `src-tauri/src/dps_meter/capture/parser/field_boss_timer.rs`, ported from `ZDYoung0519/NOIA2` `src-tauri/src/dps_meter/capture/parser/field_boss_timer.rs` (GPL-3.0; source path is also recorded in the file header). It includes the upstream map/wire-code/NPC-code table and timestamp parsing.
- Registered `field_boss_timer` in the parser module and connected it to full-mode packet dispatch. The packet header is `01 91` (opcode `0x0191`); the parser reads the table body beginning at payload offset 2. Nickname-only mode remains unchanged.
- Added an in-memory timer table to `DataStorage`, exposed through `DpsMeter`. A new map table replaces that map's prior timers; each saved row records local `lastSeen` time. DPS resets preserve timer rows.
- Added `GET /v1/field-bosses` to the local service and documented it in `docs/SERVICE-API.md`. The response uses `npcId`, `name`, `mapId`, `spawnAt`, `state`, and `lastSeen`. `name` is null with `--lang ru`, and unknown/generic English labels are null.

Example:

```sh
curl -H 'x-dbaion2-token: <secret>' http://127.0.0.1:4040/v1/field-bosses
```

```json
[
  {
    "npcId": 2400017,
    "name": "Example Boss",
    "mapId": 1110,
    "spawnAt": 1790000000000,
    "state": "respawn",
    "lastSeen": 1789999000000
  }
]
```

The example values are illustrative. `spawnAt` is the packet's Unix-millisecond target time. `state` is `respawn` before that time and `alive` after it passes. The timer packet has no explicit dead/alive flag, so a distinct `dead` state cannot be inferred locally from this opcode; no dead state is emitted. Rows remain as last received until another timer table for that map replaces them or the service exits.

## Feasibility only: Character Score and Class Statistics

NOIA2's Character Score is on `src/games/aion2/pages/character_view.tsx`. It takes item level from the character's `statList`, combat power from the profile, and Fengwo score from `data.rating.scores.score`. Character profile, equipment, skills, and stats are requested from `aion-api.bnshive.com` by `src/games/aion2/lib/fetchFengwo.tsx`; the additional rating is also server-supplied rather than computed from captured packets. The local packet stream does not contain a character's complete equipment/armory data. `dbaion2.ru`'s official-API character armory is the appropriate place for a comparable display; no score route was added to the local service.

NOIA2's Class Statistics view is the `classStats` tab in `src/games/aion2/pages/dps_rank.tsx`. It reads per-boss class sample counts and DPS distributions from Supabase table `aion2_dps_class_box_stats_v2`; the adjacent DPS ranking view reads uploads from `dps_leaderboard_v2`. Local packet capture can calculate DPS for the current observed fight and local archived fights, but cannot reproduce population-wide class distributions or rankings without collecting and aggregating uploads. No class-statistics route was added.

## Files

Added:

- `src-tauri/src/dps_meter/capture/parser/field_boss_timer.rs`
- `docs/TASK-2026-10-05-noia2-port.result.md`

Changed:

- `src-tauri/src/dps_meter/capture/parser/mod.rs`
- `src-tauri/src/dps_meter/capture/processor.rs`
- `src-tauri/src/dps_meter/storage/data_storage.rs`
- `src-tauri/src/dps_meter/engine/meter.rs`
- `src-tauri/src/service.rs`
- `docs/SERVICE-API.md`

## Verification and compiler follow-up

No Rust compiler/build or tests were run; this PC has no Rust toolchain. Review was limited to surrounding-code inspection and the displayed source diff. The lead developer/CI should double-check these integration points when compiling:

- `src-tauri/src/dps_meter/capture/parser/field_boss_timer.rs:7` — local Unix-millisecond helper and the upstream parser's offset/type assumptions.
- `src-tauri/src/dps_meter/capture/parser/field_boss_timer.rs:164` — `replace_field_boss_timers` receives the mapped iterator of `(u32, u64)` rows.
- `src-tauri/src/dps_meter/storage/data_storage.rs:942` — generic iterator signature, timer tuple storage, and snapshot tuple ordering.
- `src-tauri/src/dps_meter/engine/meter.rs:216` — snapshot/name forwarding methods match `DataStorage` visibility and types.
- `src-tauri/src/service.rs:238` — response inference, option serialization, and `state` string branches in `json!`.
- `src-tauri/src/dps_meter/capture/processor.rs:436` — dispatch arm is in full mode for header `01 91`.

No commit or push was made. No files in `G:\\dbaion2\\aion2-farm-tracker` were accessed or changed.
