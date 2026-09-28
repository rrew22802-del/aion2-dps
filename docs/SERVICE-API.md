# Local service API v1

Start the executable as a separate process:

```text
"DBAion2 DPS.exe" --service --port 4040 --token <secret> [--parent <pid>] [--lang en|ru] [--no-overlay]
```

The host can pass its PID with `--parent` or `DBAION2_PARENT_PID` (the flag wins). The service exits when that process exits, or after `POST /v1/quit`. Without a PID it runs until quit. The service opens no main or splash window and has no tray icon. Capture starts immediately. The usual in-game DPS overlay is created unless `--no-overlay` is set. An existing standalone app can run alongside the service.

The server listens only on `127.0.0.1:<port>`. The token must be nonempty ASCII. Every request needs `x-dbaion2-token: <secret>`, including health and quit; otherwise it returns HTTP 403. Responses are UTF-8 JSON with `Cache-Control: no-store`. Unknown values are JSON `null`. The API is intended for polling every 250 ms.

| Method | Path | Response |
| --- | --- | --- |
| GET | `/v1/health` | `{ "version": "2.4.0", "capture": "ok"|"no-driver"|"no-admin"|"starting", "message": "…", "game": true, "pingMs": 123 }` |
| GET | `/v1/state` | Current fight (shape below). When idle: `fightId`, `startedAt`, and `target` are `null`; `active` is `false`, numbers are zero, and `players` is empty. |
| GET | `/v1/players/{id}/skills?fight=current` | Skills for the player on the current target. A live or archived `fightId` can replace `current`. |
| GET | `/v1/players/{id}?fight=current` | Player detail for the current target. A live or archived `fightId` can replace `current`. |
| GET | `/v1/players/{id}/buffs?fight=current` | Player and boss buff uptime for the current or an archived fight. |
| GET | `/v1/pvp` | PvP kill stats and current fight's damage dealt/taken lists. |
| POST | `/v1/pvp/clear` | Clears the engine's accumulated PvP kill stats; returns `{}`. |
| GET | `/v1/pvp/watch` | Service watch names and current HP of matching players. |
| POST | `/v1/pvp/watch` | Adds `{"name":"…"}` to the service watch list. |
| DELETE | `/v1/pvp/watch` | Removes `{"name":"…"}` from the service watch list. |
| GET | `/v1/history?limit=30` | Newest first; limit defaults to 30 and is capped at 500. |
| GET | `/v1/history/{fightId}` | Archived fight in the `/v1/state` shape, with `active: false`. |
| POST | `/v1/reset` | Saves an eligible fight to history and clears the live fight; returns `{}`. |
| POST | `/v1/ui/player-detail` | Opens or focuses Aether's player-detail window for a player and fight; returns `{}`. |
| POST | `/v1/ui/close` | Hides the player-detail window, if open; returns `{}`. |
| POST | `/v1/quit` | Returns `{}` and exits. |

`POST /v1/ui/player-detail` accepts `{"playerId":16450,"fight":"current","x":120,"y":80}`. `fight` may instead be a live or archived `fightId` from the state/history routes. `x` and `y` are optional screen-pixel coordinates and must be supplied together as 32-bit integers. The window is kept within the selected monitor. Without coordinates it follows the normal detail-window placement (or centers when there is no meter overlay). A missing fight or player returns 404; an invalid body returns 400; a window error returns 500. The endpoint works with `--service --no-overlay`: it shows the existing transparent Aether detail window without showing the meter overlay. The window's ✕ and `/v1/ui/close` hide it; the next request reuses it.

For a tracker-hosted tab, launch the executable separately with `--embedded --parent-hwnd <HWND> --theme nebula|nebula-light --lang ru`. Its main window opens directly on the current fight's player list, starts capture, and opens the same detail window when a player is clicked. The embedded list uses the Nebula palette and Russian UI strings with `--lang ru`; the detail window keeps Aether's own appearance. The overlay skill catalogue bundled here currently has English and Korean names only, so Russian skill names are unavailable until a licensed Russian catalogue is added to this fork.

`/v1/state` and `/v1/history/{fightId}`:

```json
{
  "fightId": "2400017-1790000000000",
  "active": true,
  "startedAt": 1790000000000,
  "durationSec": 42.5,
  "target": { "name": "…", "npcId": 2400017, "isBoss": true, "hpPct": 37.5 },
  "totalDamage": 123456,
  "players": [{
    "id": 16450, "name": "…", "classId": 2, "className": "Gladiator",
    "isSelf": true, "isSummonOwner": false, "damage": 34567,
    "dps": 812.3, "share": 0.28, "hits": 140, "critRate": 0.31, "maxHit": 5120
  }]
}
```

The current fight is the last target of the main character, or the last observed target. Its `totalDamage` and players are for that target. `startedAt` is Unix milliseconds from its first observed hit; `durationSec` ends at its last observed hit. `share` is a fraction from 0 to 1. `classId` follows the game job groups used by the parser (for example Gladiator 2); unknown classes are `null`. `isSummonOwner` refers to damage merged into a player's total from their summons. A `fightId` remains the same when the fight is archived.

Skills are a JSON array of `{ "skillId", "name", "damage", "hits", "critRate", "maxHit", "share" }`. Skill `share` is a fraction of that player's damage on this target. `critRate` is `null` if no hits were counted. Empty known fights return `[]`; unknown fights return 404. Invalid IDs or missing `fight` return 400.

`/v1/players/{id}` includes the same `id`, `name`, `classId`, `className`, `isSelf`, `isSummonOwner`, `damage`, `dps`, `share`, `hits`, `critRate`, and `maxHit` fields as the player in `/v1/state`, plus `fightSec`, `backRate`, `frontRate`, `doubleRate`, `perfectRate`, `parryRate`, and `multiRate`. `fightSec` is that player's first-to-last hit on the target, with a 1-second minimum as in Aether's detail window (`null` if unavailable); detail `dps` uses that duration. Rates are fractions from 0 to 1 and are `null` when no hits were counted. An unknown player returns 404.

Each skill also has `icon` (the UI's skill image key, used as `/aion2/skill/{icon}.png`), `spec` (active specialty slot numbers, 1–5), `perfectRate`, `doubleRate`, `frontRate`, `backRate`, `parryRate`, `multiRate`, `multiDamage`, `minHit`, and `avgHit`. Rates use the same fraction and null convention. `multiDamage` is the engine's counted multi-hit damage. `minHit` and `avgHit` are `null` with no hits.

`/v1/players/{id}/buffs` returns `{ "player": [{ "id", "name", "icon", "uptime", "source" }], "boss": [...] }`. Buff `id` is the skill code; `source` is the caster's known name or `null`. `uptime` is the engine's coverage fraction from 0 to 1. The arrays are empty when no buffs were observed. The boss array refers to the selected fight target. Buff uptime follows the engine's existing capture window.

`/v1/pvp` returns `{ "players": [{ "name", "serverId", "classId", "damage", "kills", "assists", "deaths" }], "damageDealt": [...], "damageTaken": [...] }`. `players` uses the engine's accumulated PvP combat stats, collected while PvP mode is enabled; its `damage` is damage to players counted in those stats. `damageDealt` and `damageTaken` are the current snapshot's existing player overview lists (camelCase engine fields such as `playerName`, `totalDamage`, and `playerClass` for dealt damage, and `actorName`, `totalDamage`, and `actorClass` for taken damage). These lists are empty without a current snapshot. Clearing PvP stats does not reset the current fight.

`/v1/pvp/watch` returns `{ "names": ["…"], "players": [{ "name", "actorId", "actorName", "classId", "currentHp", "maxHp", "hpPct" }] }`. `hpPct` is 0–100 using the highest observed HP as the denominator, or `null` when unavailable. The service watch list is kept in memory until service exit; it is separate from Aether's own overlay watch list. POST/DELETE require a nonempty `name` (up to 128 UTF-8 bytes) in a JSON object and return `{ "names": [...] }`.

When capture starts after login, 20 or more `00 8D` packets can identify the local player if one entity accounts for at least 60% of them. The login packet remains authoritative when received later. Class inference uses the existing skill-ID prefix table when a job byte is unavailable; unrecognized classes stay `null`.

History rows are `{ "fightId", "startedAt", "durationSec", "target", "isBoss", "totalDamage", "selfDps", "selfShare" }`. History is the existing engine history; it saves fights over 1,000,000 damage and retains up to 500 records. A reset may therefore produce no history row.

Player names come from captured game data. English target names come from the bundled NPC catalogue and English skill names from the same catalogue used by the overlays. The existing project has no Russian NPC or skill catalogue. With `--lang ru`, class names are Russian and target/skill names without a Russian entry are `null`. The English catalogue is not mislabeled as Russian. All numeric measurements and IDs are language independent.
