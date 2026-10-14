// Development fixture server. No capture, game access, or installed service interaction.
import { readFileSync } from "node:fs";
import { createServer } from "node:http";
import path from "node:path";
import { fileURLToPath } from "node:url";

const stateFixture = JSON.parse(readFileSync(new URL("../src-tauri/tests/fixtures/target-hp-state.json", import.meta.url), "utf8"));
const version = JSON.parse(readFileSync(new URL("../package.json", import.meta.url), "utf8")).version;
const scenarios = new Set(["hp-known", "hp-unknown", "percent-only", "dead", "idle"]);

export function createFakeServiceApi({ token = "fake-service-token", scenario = "hp-known" } = {}) {
  if (!scenarios.has(scenario)) throw new Error(`Unknown scenario: ${scenario}`);
  const state = structuredClone(stateFixture);
  if (scenario === "hp-unknown" || scenario === "percent-only") {
    state.target.hp = null;
    state.target.maxHp = null;
    if (scenario === "hp-unknown") state.target.hpPct = null;
  } else if (scenario === "dead") {
    state.target.hp = 0;
    state.target.hpPct = 0;
  }
  state.boss.target = structuredClone(state.target);
  const archived = structuredClone(state);
  archived.active = false;
  delete archived.targetId;
  delete archived.pinned;
  delete archived.boss;
  const live = scenario === "idle"
    ? { fightId: null, active: false, startedAt: null, durationSec: 0, target: null, totalDamage: 0, players: [] }
    : state;
  return createServer((request, response) => {
    response.setHeader("Content-Type", "application/json; charset=utf-8");
    response.setHeader("Cache-Control", "no-store");
    const reply = (status, body) => {
      response.statusCode = status;
      response.end(JSON.stringify(body));
    };
    if (request.headers["x-dbaion2-token"] !== token) return reply(403, { error: "forbidden" });
    const url = new URL(request.url, "http://127.0.0.1");
    if (request.method !== "GET") return reply(404, { error: "not found" });
    if (url.pathname === "/v1/health") return reply(200, { version, capture: "ok", message: "fixture", game: false, pingMs: null });
    if (url.pathname === "/v1/state") {
      const pinned = structuredClone(live);
      if (pinned.target && url.searchParams.get("target") === String(pinned.targetId)) pinned.pinned = true;
      return reply(200, pinned);
    }
    if (url.pathname === `/v1/history/${archived.fightId}`) return reply(200, archived);
    return reply(404, { error: "not found" });
  });
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const port = Number(process.env.FAKE_DPS_PORT ?? 0);
  const server = createFakeServiceApi({ token: process.env.FAKE_DPS_TOKEN, scenario: process.env.FAKE_DPS_SCENARIO });
  server.listen(port, "127.0.0.1", () => {
    console.log(`Fake DPS API: http://127.0.0.1:${server.address().port} (fixture data only)`);
  });
}
