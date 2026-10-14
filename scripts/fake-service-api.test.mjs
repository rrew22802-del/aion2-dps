import assert from "node:assert/strict";
import { once } from "node:events";
import { readFileSync } from "node:fs";
import test from "node:test";
import { createFakeServiceApi } from "./fake-service-api.mjs";

const expected = JSON.parse(readFileSync(new URL("../src-tauri/tests/fixtures/target-hp-state.json", import.meta.url), "utf8"));

for (const scenario of ["hp-known", "hp-unknown", "percent-only", "dead", "idle"]) {
  test(`state/history nullable absolute HP: ${scenario}`, async (t) => {
    const server = createFakeServiceApi({ scenario, token: "test-token" });
    server.listen(0, "127.0.0.1");
    await once(server, "listening");
    t.after(() => new Promise((resolve, reject) => server.close((error) => error ? reject(error) : resolve())));
    const base = `http://127.0.0.1:${server.address().port}`;
    const get = async (route) => {
      const response = await fetch(base + route, { headers: { "x-dbaion2-token": "test-token" } });
      assert.equal(response.status, 200);
      assert.equal(response.headers.get("cache-control"), "no-store");
      return response.json();
    };
    assert.equal((await fetch(base + "/v1/state")).status, 403);
    assert.equal((await get("/v1/health")).version, "2.9.0");
    const live = await get("/v1/state");
    const archived = await get(`/v1/history/${expected.fightId}`);
    assert.equal(archived.active, false);
    assert.equal(archived.target.maxHp, null, "An observed peak is not a decoded maximum");
    if (scenario === "idle") {
      assert.equal(live.target, null);
      assert.equal(live.active, false);
      return;
    }
    assert.deepEqual(live.target, archived.target);
    assert.deepEqual(live.boss.target, live.target);
    assert.equal((await get("/v1/state?target=9001")).pinned, true);
    assert.ok(Object.hasOwn(live.target, "hp") && Object.hasOwn(live.target, "maxHp"));
    if (scenario === "hp-known") {
      assert.deepEqual(live, expected);
      assert.equal(live.target.hp, 37_500_000);
    } else if (scenario === "dead") {
      assert.equal(live.target.hp, 0);
      assert.equal(live.target.hpPct, 0);
    } else {
      assert.equal(live.target.hp, null);
      assert.equal(live.target.hpPct, scenario === "percent-only" ? 37.5 : null);
    }
  });
}
