# Local service API v1

Start the executable as a separate process:

```text
"DBAion2 DPS.exe" --service --port 4040 --token <secret> [--parent <pid>] [--lang en|ru] [--no-overlay]
```

The host can pass its PID with `--parent` or `DBAION2_PARENT_PID` (the flag wins). The service exits when that process exits, or after `POST /v1/quit`. Without a PID it runs until quit. The service opens no main or splash window and has no tray icon. Capture starts immediately. The usual in-game DPS overlay is created unless `--no-overlay` is set. An existing standalone app can run alongside the service.

The server listens only on `127.0.0.1:<port>`. The token must be nonempty ASCII. Every request needs `x-dbaion2-token: <secret>`, including health, assets, and quit; otherwise it returns HTTP 403. API responses are UTF-8 JSON with `Cache-Control: no-store`; asset responses are PNG bytes with `Content-Type: image/png` and `Cache-Control: max-age=86400`. Unknown values are JSON `null`. The API is intended for polling every 250 ms.

| Method | Path | Response |
| --- | --- | --- |
| GET | `/v1/health` | `{ "version": "2.8.0", "capture": "ok"|"no-driver"|"no-admin"|"starting", "message": "…", "game": true, "pingMs": 123 }` |
| GET | `/v1/state` | Current fight (shape below). When idle: `fightId`, `startedAt`, and `target` are `null`; `active` is `false`, numbers are zero, and `players` is empty. |
| GET | `/v1/party` | `{ "members": [{"id": 16450, "name": "…", "class": "GLADIATOR"}], "updatedAt": 1790000000000 }` current known party members. |
| GET | `/v1/nearby` | `{ "self": {"x": null, "y": null, "z": null}, "players": [...] }` recent visible players; order is by distance when positions are available. |
| GET | `/v1/live` | Current self buffs, self-applied effects on the current or pinned target, recent own skill uses, and target HP. `?target=<id>` pins a known mob target. |
| GET | `/v1/field-bosses` | Field-boss spawn timers last received from packet `01 91`; names are `null` with `--lang ru`. |
| GET | `/v1/assets/aion2/class/{name}.png` or `/v1/assets/aion2/skill/{name}.png` | Bundled icon bytes. Only PNG files directly in those two folders are served; missing or invalid paths return 404. |
| GET | `/v1/players/{id}/skills?fight=current` | Skills for the player on the current target. A live or archived `fightId` can replace `current`. |
| GET | `/v1/players/{id}?fight=current` | Player detail for the current target. A live or archived `fightId` can replace `current`. |
| GET | `/v1/players/{id}/buffs?fight=current` | Player and boss buff uptime for the current or an archived fight. |
| GET | `/v1/fights/{fightId}/timeline?player={id}` | One-second player/group damage, player's casts, and available buff intervals. `fightId` may be `current`. |
| GET | `/v1/fights/{fightId}/taken?player={id}` | Damage received by that player, grouped by observed source entity/NPC and skill code. |
| GET | `/v1/fights/{fightId}/deaths` | Death events and up to the last ten decoded incoming hits/heals for each death. |
| GET | `/v1/fights/{fightId}/buffs` | Buff intervals summarized per fight player and skill, with coverage uptime and observed appliers. |
| GET | `/v1/fights/{fightId}/boss-casts` | Observed skill codes used by NPC sources during the fight, with offsets. |
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
    "id": 16450, "name": "…", "classId": 2, "className": "Gladiator", "classIcon": "aion2/class/gladiator.png",
    "isSelf": true, "inParty": true, "isSummonOwner": false, "damage": 34567,
    "dps": 812.3, "share": 0.28, "hits": 140, "critRate": 0.31, "maxHit": 5120, "damageTaken": 2100,
    "cp": 18452, "deaths": 0, "healTotal": 9200, "hps": 216.5
  }]
}
```

`/v1/state` and archived fight players also expose `cp` (combat power, or `null` when not observed), `deaths`, `healTotal`, `hps`, `damageTaken`, and `inParty`. `damageTaken` sums decoded non-healing combat amounts directed at the player and associated with the current fight target; source detail is available from `/v1/fights/{fightId}/taken?player={id}`. The association uses the latest observed player target because the packet does not itself carry the encounter target. `inParty` is true for the main character and members in the latest known party roster; for other players it remains false because no party-only membership packet has been confirmed. Archived records preserve the roster captured when the fight was saved. `/v1/history` rows include a `players` array with the same `inParty` field. `mapId` is `null` in history until a confirmed dungeon/map identity packet is decoded. `GET /v1/party` returns the current roster; `updatedAt` is Unix milliseconds or `null` before the first update. A party roster is cleared when the main character changes and refreshed by local-character identity updates. `45 37` is not confirmed as a leave action and does not remove a member; `45 38` remains provisional. Alliance membership is not distinguished by the observed packet data, so this endpoint describes the party only. Healing totals cover healing packets with a decoded amount and are counted across targets; HPS divides by that player's observed fight duration. A packet without an amount contributes nothing.

`GET /v1/fights/{fightId}/taken?player={id}` returns `{ "player": 16450, "damageTaken": 2100, "sources": [{"npcId":2400017,"npcName":"…","sourceId":9001,"sourceName":"…","skillCode":11010000,"damage":2100}] }`. `npcId` and `npcName` use the NPC catalogue when the source entity is known as an NPC; name is `null` with Russian locale or when no catalogue entry is available. Player or unresolved sources have `null` NPC fields. `sourceName` is the observed actor name when available. Skill is the exact decoded skill code. This list is capped at 128 distinct source/skill pairs per player and fight.

`GET /v1/fights/{fightId}/deaths` returns `[{"player":16450,"at":42000,"events":[{"type":"hit","sourceId":9001,"sourceName":"…","skill":11010000,"amount":2100,"crit":true,"at":39500,"hpAfter":null}]}]`. Offsets are milliseconds from fight start. Each player's rolling recap retains ten decoded hit/heal events; up to 128 deaths are retained per fight. Heals are included only when the existing healing parser supplies an amount. HP after an event is `null` unless a health update can be associated with that event.

`GET /v1/fights/{fightId}/buffs` returns `{ "players": [{"id":16450,"buffs":[{"code":11010000,"uptime":0.75,"applications":2,"sourceIds":[16451],"intervals":[{"start":1000,"end":16000,"sourceId":16451}]}]}] }`. Interval offsets are milliseconds from fight start. Uptime is the union of observed buff intervals clipped to the fight window, divided by its duration (max 20 minutes); `sourceIds` contains observed appliers. `applications` counts saved intervals after the parser's existing near-overlap merge. Each interval list retains at most 1,024 intervals per target/source/skill, matching the existing bounded buff store.

`GET /v1/fights/{fightId}/boss-casts` returns `[{"npcId":2400017,"skill":11010000,"at":12500}]`. It records skill-bearing damage packets whose decoded source entity is already identified as an NPC; it does not infer cast start times from damage animations. Up to 5,000 entries are retained per fight.

`GET /v1/fights/{fightId}/timeline?player={id}` returns `{ "durationMs", "damagePerSecond": [{"offsetMs", "playerDamage", "groupDamage"}], "casts": [{"offsetMs", "skillCode", "damage", "crit"}], "buffs": [{"startOffsetMs", "endOffsetMs", "skillCode", "sourceId"}] }`. Resolution and included duration are capped at 20 minutes (1,200 one-second buckets); casts are capped at 25,000. Buff intervals are returned when the existing buff parser stored them. An unknown fight or player returns 404; a missing or invalid player query returns 400.

`GET /v1/live` returns active effect intervals and own skill uses from the current session. Names follow `--lang en|ru`; missing catalogue entries are empty strings for skills and `null` for the target name. Timestamps are Unix milliseconds. Example:

`GET /v1/nearby` returns players observed in `45 36` within the last 120 seconds, up to 500 entries. Each row has `id`, `name`, nullable `server`, `class`, `level`, `cp`, `legion`, `lastSeen` (Unix milliseconds), `lastPosition`, `inParty`, and `seenDamage`. `seenDamage` is the accumulated damage attributed to that actor in parsed combat packets during the current retained row. Party membership is self-only because a reliable party roster opcode has not been confirmed. Current captures do not establish player coordinates; therefore `self.x/y/z` and `lastPosition` are `null`, and rows are ordered by ID until position decoding is confirmed. Rows expire after 120 seconds without a `45 36` update. No packet is currently treated as a confirmed nearby-player despawn.

```json
{
  "now": 1790000000000,
  "selfId": 16450,
  "buffs": [{"skill": 11010000, "name": "Example buff", "icon": "aion2/skill/1101.png", "actor": 16450, "fromSelf": true, "startMs": 1789999990000, "endMs": 1790000010000, "durationMs": 20000}],
  "debuffs": [{"skill": 11020000, "name": "Example debuff", "icon": "aion2/skill/1102.png", "actor": 16450, "fromSelf": true, "startMs": 1789999995000, "endMs": 1790000005000, "durationMs": 10000}],
  "casts": [{"skill": 11030000, "name": "Example strike", "icon": "aion2/skill/1103.png", "lastMs": 1789999999000, "count": 12}],
  "target": {"id": 2400017, "name": "Example boss", "hpPct": 37.5, "isBoss": true}
}
```

`buffs` are active effects targeting the local character; `debuffs` are active effects applied by the local character to the selected target. `casts` are ordered by most recent hit and capped at 60 skills. Missing character, target, or HP data is reported as `null` (or an empty array).

The current fight is the last target of the main character, or the last observed target. Its `totalDamage` and players are for that target. `startedAt` is Unix milliseconds from its first observed hit; `durationSec` ends at its last observed hit. `share` is a fraction from 0 to 1. `classId` follows the game job groups used by the parser (for example Gladiator 2); unknown classes have `null` for `classId`, `className`, and `classIcon`. `isSummonOwner` refers to damage merged into a player's total from their summons. A `fightId` remains the same when the fight is archived.

`GET /v1/field-bosses` returns an array of `{ "npcId", "name", "mapId", "spawnAt", "state", "lastSeen" }`. `spawnAt` and `lastSeen` are Unix milliseconds. The rows reflect the latest `01 91` timer table captured for each map and remain in memory across DPS resets. `state` is `respawn` while `spawnAt` is in the future and `alive` after that time passes. The timer packet contains scheduled timestamps but no explicit alive/dead flag, so this endpoint cannot report `dead` from packet data alone. `name` is `null` with `--lang ru` and when the English catalogue has no name.

Skills are a JSON array of `{ "skillId", "name", "damage", "hits", "critRate", "maxHit", "share" }`. Skill `share` is a fraction of that player's damage on this target. `critRate` is `null` if no hits were counted. Empty known fights return `[]`; unknown fights return 404. Invalid IDs or missing `fight` return 400.

`/v1/players/{id}` includes the same `id`, `name`, `classId`, `className`, `classIcon`, `isSelf`, `inParty`, `isSummonOwner`, `damage`, `dps`, `share`, `hits`, `critRate`, and `maxHit` fields as the player in `/v1/state`, plus `fightSec`, `backRate`, `frontRate`, `doubleRate`, `perfectRate`, `parryRate`, and `multiRate`. `fightSec` is that player's first-to-last hit on the target, with a 1-second minimum as in Aether's detail window (`null` if unavailable); detail `dps` uses that duration. Rates are fractions from 0 to 1 and are `null` when no hits were counted. An unknown player returns 404.

Each skill also has `icon` (an asset path such as `aion2/skill/1101.png`), `spec` (active specialty slot numbers, 1–5), `perfectRate`, `doubleRate`, `frontRate`, `backRate`, `parryRate`, `multiRate`, `multiDamage`, `minHit`, and `avgHit`. Rates use the same fraction and null convention. `multiDamage` is the engine's counted multi-hit damage. `minHit` and `avgHit` are `null` with no hits. Request an icon path by prefixing it with `/v1/assets/` and the service token; icons are served from the executable's bundled frontend assets, without a website request.

`/v1/players/{id}/buffs` returns `{ "player": [{ "id", "name", "icon", "uptime", "source" }], "boss": [...] }`. Buff `id` is the skill code; `icon` uses the same asset path format as skill rows; `source` is the caster's known name or `null`. `uptime` is the engine's coverage fraction from 0 to 1. The arrays are empty when no buffs were observed. The boss array refers to the selected fight target. Buff uptime follows the engine's existing capture window.

`/v1/pvp` returns `{ "players": [{ "name", "serverId", "classId", "damage", "kills", "assists", "deaths" }], "damageDealt": [...], "damageTaken": [...] }`. `players` uses the engine's accumulated PvP combat stats, collected while PvP mode is enabled; its `damage` is damage to players counted in those stats. `damageDealt` and `damageTaken` are the current snapshot's existing player overview lists (camelCase engine fields such as `playerName`, `totalDamage`, and `playerClass` for dealt damage, and `actorName`, `totalDamage`, and `actorClass` for taken damage). These lists are empty without a current snapshot. Clearing PvP stats does not reset the current fight.

`/v1/pvp/watch` returns `{ "names": ["…"], "players": [{ "name", "actorId", "actorName", "classId", "currentHp", "maxHp", "hpPct" }] }`. `hpPct` is 0–100 using the highest observed HP as the denominator, or `null` when unavailable. The service watch list is kept in memory until service exit; it is separate from Aether's own overlay watch list. POST/DELETE require a nonempty `name` (up to 128 UTF-8 bytes) in a JSON object and return `{ "names": [...] }`.

When capture starts after login, 20 or more `00 8D` packets can identify the local player if one entity accounts for at least 60% of them. The login packet remains authoritative when received later. Class inference uses the existing skill-ID prefix table when a job byte is unavailable; unrecognized classes stay `null`.

History rows are `{ "fightId", "startedAt", "durationSec", "target", "mapId", "isBoss", "totalDamage", "selfDps", "selfShare", "healTotal", "hps", "deaths", "selfCp", "selfDeaths", "players" }`. `mapId` is nullable and remains `null` unless a reliable map source becomes available. Each player row includes `inParty` and `damageTaken`. `healTotal` and `deaths` are group totals; the `self*` fields refer to the main character when known. History is the existing engine history; it saves fights over 1,000,000 damage and retains up to 500 records. A reset may therefore produce no history row.

Player names come from captured game data. English target names come from the bundled NPC catalogue and English skill names from the same catalogue used by the overlays. The existing project has no Russian NPC or skill catalogue. With `--lang ru`, class names are Russian and target/skill names without a Russian entry are `null` on the existing fight and detail routes; `/v1/live` uses empty strings for missing skill names and `null` for target names. The English catalogue is not mislabeled as Russian. All numeric measurements and IDs are language independent.
