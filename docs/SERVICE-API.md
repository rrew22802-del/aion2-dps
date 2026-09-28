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
| GET | `/v1/history?limit=30` | Newest first; limit defaults to 30 and is capped at 500. |
| GET | `/v1/history/{fightId}` | Archived fight in the `/v1/state` shape, with `active: false`. |
| POST | `/v1/reset` | Saves an eligible fight to history and clears the live fight; returns `{}`. |
| POST | `/v1/quit` | Returns `{}` and exits. |

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

History rows are `{ "fightId", "startedAt", "durationSec", "target", "isBoss", "totalDamage", "selfDps", "selfShare" }`. History is the existing engine history; it saves fights over 1,000,000 damage and retains up to 500 records. A reset may therefore produce no history row.

Player names come from captured game data. English target names come from the bundled NPC catalogue and English skill names from the same catalogue used by the overlays. The existing project has no Russian NPC or skill catalogue. With `--lang ru`, class names are Russian and target/skill names without a Russian entry are `null`. The English catalogue is not mislabeled as Russian. All numeric measurements and IDs are language independent.
