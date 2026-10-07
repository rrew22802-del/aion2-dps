# Task: bring NOIA2 4.x features into our DPS fork (LOCAL ONLY, no push)

APPROVED by the owner — finish with "NOIA2-1005-DONE". Repo G:\dbaion2\aion2-dps, branch dbaion2. Never push, never commit, never
touch G:\dbaion2\aion2-farm-tracker (closed program — GPL code must never go there). TMP/TEMP/TMPDIR = G:\dbaion2\.tmp-deploy.
There is NO Rust toolchain on this PC: write the code carefully, keep it compiling by reading the surrounding code; the lead developer
pushes and GitHub Actions builds it.

Our fork comes from Aether, which comes from NOIA2 (https://github.com/ZDYoung0519/NOIA2, GPL-3.0, branch main = the open 4.x).
Both are GPL-3.0 like ours, so porting code is allowed — keep their copyright/attribution comments and note the source file in a comment.
Use `gh api repos/ZDYoung0519/NOIA2/contents/<path>` (base64) to read their files.

## 1. Field boss timers (main item)
NOIA2 has src-tauri/src/dps_meter/capture/parser/field_boss_timer.rs (commit 2026-08-05 «地图boss和活动计时实现» — map boss and event timers).
Our fork does not have it. Port it into src-tauri/src/dps_meter/capture/parser/ (wire it into the parser dispatch like the other parsers),
keep the timers in the meter state, and expose them in our local service API (src-tauri/src/service.rs):
  GET /v1/field-bosses -> [{ "npcId": u32, "name": string|null, "mapId"/"zone": if known, "spawnAt": ms epoch or null, "state": "alive"|"dead"|"respawn", "lastSeen": ms }]
Names: same convention as target_name() in service.rs (lang ru → null so the client uses its own Russian names by npcId).
Add a short section to the service API doc if there is one (search docs/ for "/v1/state").

## 2. Character score and class statistics — feasibility first
In NOIA2 these pages (src/pages, «Character Score», «Class Statistics») may depend on their own server (noia2.top uploads / rankings).
Read their code and write down: what data each page uses, whether that data is available to us locally (packets) or only from their server,
and whether our site (dbaion2.ru has character armory pages from the official API) is the better place. Implement ONLY parts that need no
external server and fit our service API; otherwise just report.

## Result
docs/TASK-2026-10-05-noia2-port.result.md: files added/changed, the packet opcodes used, API examples, what was ported vs only reported, and
any doubt about compile errors (list the exact lines to double-check).
