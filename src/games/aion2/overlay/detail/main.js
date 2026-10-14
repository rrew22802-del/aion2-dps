import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { t, setLanguage } from "../../i18n.js";
import skillsEn from "@/i18n/locales/aion2skills/en.json";
import skillsKo from "@/i18n/locales/aion2skills/ko.json";
import buffMetadata from "@/i18n/locales/aion2skills/buff_metadata.json";
import skillsRu from "@/i18n/locales/aion2skills/ru.json";
import serversData from "@/games/aion2/data/servers.json";

const SKILLS = { en: skillsEn, ko: skillsKo, ru: skillsRu };
let currentSkills = skillsEn;

// ── Server name lookup ──
const serverMap = new Map(serversData.map((s) => [s.serverId, s.serverName]));
function getServerName(serverId) {
  return serverMap.get(Number(serverId)) || serverId || "";
}

// ── DOM ──
const $modeBadge = document.getElementById("mode-badge");
const $content = document.getElementById("detail-content");
const $playerIcon = document.getElementById("player-icon");
const $playerName = document.getElementById("player-name");
const $playerServer = document.getElementById("player-server");
const $playerPower = document.getElementById("player-power");
const $playerPowerText = document.getElementById("player-power-text");
const $closeStatus = document.getElementById("close-status");
const detailWindow = getCurrentWindow();
let lastCloseOperation = Promise.resolve();

let currentLanguage = "en";
let selectionGeneration = 0;
let snapshotRenderTimer = null;
let animationFrame = null;
let lastSnapshotRenderAt = 0;
let renderPending = false;
let lastContentMarkup = null;
let lastContentScope = null;
let lastRenderDurationMs = 0;
const SNAPSHOT_RENDER_INTERVAL_MS = 200;

function reportCloseFailure(error) {
  console.error("[dps-detail] window hide failed:", error);
  $closeStatus.textContent = t("dps-detail.closeFailed");
}

async function closeDetailWindow() {
  $closeStatus.textContent = "";
  if (animationFrame !== null) window.cancelAnimationFrame(animationFrame);
  if (snapshotRenderTimer !== null) window.clearTimeout(snapshotRenderTimer);
  animationFrame = null;
  snapshotRenderTimer = null;
  renderPending = false;
  try {
    await detailWindow.hide();
  } catch (error) {
    reportCloseFailure(error);
  }
}

document.getElementById("close-btn").addEventListener("click", () => {
  lastCloseOperation = closeDetailWindow();
});

document.addEventListener("keydown", (event) => {
  if (event.key === "Escape") {
    event.preventDefault();
    lastCloseOperation = closeDetailWindow();
  }
});

document.getElementById("detail-drag-handle").addEventListener("mousedown", (event) => {
  if (event.button !== 0 || event.target.closest("button, a, input, select, textarea")) return;
  detailWindow.startDragging().catch((error) => {
    console.warn("[dps-detail] drag start failed:", error);
  });
});

// ── State ──
let mode = "live";
let selectedActorId = null;
let selectedTargetId = null;
let frozenRecord = null;
let lastSnapshot = null;
let snapshotGeneration = 0;
let isPlayerBuffPanelOpen = false;
let isBossBuffPanelOpen = false;

// ── Formatters ──
function fmtDamage(n) {
  if (!n) return "0";
  if (n < 1e4) return String(n);
  if (n < 1e6) return (n / 1e3).toFixed(1) + "K";
  if (n < 1e9) return (n / 1e6).toFixed(2) + "M";
  return (n / 1e9).toFixed(2) + "B";
}
function fmtFull(n) {
  return Math.round(n || 0).toLocaleString("en-US");
}
function fmtPower(n) {
  const value = Number(n);
  if (!Number.isFinite(value) || value <= 0) return "";
  return (value / 1000).toFixed(1) + "k";
}
function esc(s) {
  if (!s) return "";
  const d = document.createElement("div");
  d.textContent = s;
  return d.innerHTML;
}
function fmtPct(n) {
  return (n || 0).toFixed(1) + "%";
}
function fmtDuration(s) {
  if (!s || s <= 0) return "--";
  if (s < 60) return Math.floor(s) + t("dps-detail.secondsShort");
  const m = Math.floor(s / 60),
    sec = Math.floor(s % 60);
  return m + t("dps-detail.minutesShort") + " " + String(sec).padStart(2, "0") + t("dps-detail.secondsShort");
}
function getClassIcon(c) {
  return c ? "/aion2/class/" + c.toLowerCase() + ".png" : "";
}
function normalizeSkillId(id) {
  // Take the first 8 digits, or fewer when the id is shorter. Normalising is
  // only needed to look up a skill name -- skills are already grouped in the
  // backend, so the frontend merges nothing itself.
  return String(id).slice(0, 8);
}
function skillLookupCandidates(id) {
  const raw = String(id);
  const candidates = [raw, normalizeSkillId(raw)];
  if (raw.length > 8) {
    candidates.push(raw.slice(0, 8).replace(/\d$/, "0"));
  }
  if (raw.length > 6) {
    candidates.push(raw.slice(0, 6).padEnd(8, "0"));
  }
  return [...new Set(candidates)];
}
function resolveSkillId(id) {
  return (
    skillLookupCandidates(id).find((candidate) => currentSkills[candidate]) || normalizeSkillId(id)
  );
}
function skillName(id) {
  const k = resolveSkillId(id);
  return currentSkills[k] || t("dps-detail.skill") + " #" + id;
}
function skillIcon(id) {
  const base = resolveSkillId(id);
  return base.length === 6
    ? "/aion2/skill/" + base + ".png"
    : "/aion2/skill/" + base.slice(0, 4) + ".png";
}
function specDots(slots) {
  const set = new Set((slots || []).filter((n) => n >= 1 && n <= 5));
  return [1, 2, 3, 4, 5].map((s) => set.has(s));
}
function getSpecial(stats, key) {
  return Number(stats?.specialCounts?.[key] ?? 0);
}
function fmtBuffCoverage(value) {
  const pct = Math.max(0, Math.min(100, Number(value || 0) * 100));
  return pct >= 99.95 ? "100%" : pct.toFixed(1) + "%";
}
function getTargetBuffs(dataSource, targetId) {
  if (targetId == null) return [];
  return (
    dataSource?.useBuffsByTarget?.[String(targetId)] ||
    dataSource?.useBuffsByTarget?.[targetId] ||
    []
  );
}
function getActorInfo(dataSource, actorId) {
  return (
    dataSource?.combatInfos?.actorInfos?.[actorId] ||
    dataSource?.combatInfos?.actorInfos?.[String(actorId)] ||
    null
  );
}
function getActorLabel(dataSource, actorId) {
  const actorInfo = getActorInfo(dataSource, actorId);
  return actorInfo?.actorName || `#${actorId}`;
}
function renderBuffTimeline(buffs, dataSource) {
  const rows = (buffs || [])
    .slice()
    .sort((a, b) => {
      if (Boolean(a.active) !== Boolean(b.active)) return a.active ? -1 : 1;
      return Number(b.coverage || 0) - Number(a.coverage || 0);
    })
    .map((buff) => {
      const skillCode = Number(buff.skillCode);
      if (!Number.isFinite(skillCode)) return null;
      const metadata = buffMetadata[String(buff.skillCode)];
      const name = metadata?.[currentSkills === skillsRu ? "ru" : "en"] || skillName(buff.skillCode);
      const icon = metadata?.icon ? "/aion2/buff/" + metadata.icon + ".png" : skillIcon(buff.skillCode);
      const casterInfo = getActorInfo(dataSource, buff.actorId);
      const caster = getActorLabel(dataSource, buff.actorId);
      const casterIcon = getClassIcon(casterInfo?.actorClass);
      const coverage = fmtBuffCoverage(buff.coverage);
      const coverageWidth = Math.max(2, Math.min(100, Number(buff.coverage || 0) * 100));
      const activeClass = buff.active ? " is-active" : "";
      const label = `${name} (${buff.skillCode}) / ${caster} / ${coverage}`;
      return `
        <div class="buff-timeline__row${activeClass}" title="${esc(label)}">
          <div class="buff-timeline__art">
            ${icon ? `<img class="buff-timeline__art-icon" src="${icon}" alt="" onerror="this.style.display='none'"/>` : ""}
          </div>
          <div class="buff-timeline__content">
            <div class="buff-timeline__head">
              <span class="buff-timeline__skill">${esc(name)}</span>
              <span class="buff-timeline__coverage">${coverage}</span>
            </div>
            <div class="buff-timeline__meta">
              <span class="buff-timeline__caster">
                ${
                  casterIcon
                    ? `<img class="buff-timeline__class-icon" src="${casterIcon}" alt="" onerror="this.style.display='none'"/>`
                    : ""
                }
                <span>${esc(caster)}</span>
              </span>
              <span class="buff-timeline__state${buff.active ? " is-active" : ""}"></span>
            </div>
            <div class="buff-timeline__track">
              <div class="buff-timeline__bar" style="width:${coverageWidth.toFixed(2)}%"></div>
            </div>
          </div>
        </div>`;
    })
    .filter(Boolean);

  return `
    <div class="buff-timeline">
      ${
        rows.length > 0
          ? rows.join("")
          : `<div class="buff-timeline__empty">${t("dps-detail.noBuffs")}</div>`
      }
    </div>`;
}
function renderBuffPanel({ id, title, open, buffs, dataSource, fightStart }) {
  return `
    <details class="buff-panel" data-buff-panel="${id}" ${open ? "open" : ""}>
      <summary class="buff-panel__summary">
        <span>${title}</span>
        <span class="buff-panel__chevron">▾</span>
      </summary>
      <div class="buff-panel__body">
        ${renderBuffTimeline(buffs, dataSource)}
      </div>
    </details>`;
}
function mergeSkillStats(target, source) {
  target.totalDamage += source.totalDamage || 0;
  target.counts += source.counts || 0;
  target.maxDamage = Math.max(target.maxDamage, source.maxDamage || 0);
  target.minDamage = Math.min(target.minDamage, source.minDamage || 1e99);
  for (const [k, v] of Object.entries(source.specialCounts || {})) {
    target.specialCounts[k] = (target.specialCounts[k] || 0) + v;
  }
}
function emptySkillStats() {
  return {
    totalDamage: 0,
    counts: 0,
    maxDamage: 0,
    minDamage: 1e99,
    specialCounts: {},
  };
}

// ── Render ──
function setMode(m) {
  mode = m;
  $modeBadge.textContent = mode === "history" ? t("dps-overlay.history") : t("dps-overlay.live");
  $modeBadge.className = "titlebar__mode " + (mode === "history" ? "is-history" : "is-live");
}

function requestRender(immediate = false) {
  renderPending = true;
  if (immediate) {
    if (snapshotRenderTimer !== null) {
      clearTimeout(snapshotRenderTimer);
      snapshotRenderTimer = null;
    }
  } else if (snapshotRenderTimer !== null) {
    return;
  } else {
    const wait = Math.max(0, SNAPSHOT_RENDER_INTERVAL_MS - (performance.now() - lastSnapshotRenderAt));
    if (wait > 0) {
      snapshotRenderTimer = window.setTimeout(() => {
        snapshotRenderTimer = null;
        requestRender(true);
      }, wait);
      return;
    }
  }
  if (animationFrame !== null) return;
  animationFrame = window.requestAnimationFrame(() => {
    animationFrame = null;
    if (!renderPending) return;
    renderPending = false;
    const startedAt = performance.now();
    try {
      render();
    } catch (error) {
      console.error("[dps-detail] render failed:", error);
      $content.innerHTML = `<div class="empty">${t("dps-detail.noData")}</div>`;
    } finally {
      lastSnapshotRenderAt = performance.now();
      const elapsed = lastSnapshotRenderAt - startedAt;
      lastRenderDurationMs = elapsed;
      if (elapsed >= 50) console.warn(`[dps-detail] slow render: ${elapsed.toFixed(1)}ms`);
    }
  });
}

function applySelection(payload) {
  if (!payload || !payload.actorId) return;
  selectionGeneration += 1;
  selectedActorId = payload.actorId;
  selectedTargetId = payload.targetId ?? null;
  mode = payload.mode || "live";
  frozenRecord = mode === "history" ? payload.record || null : null;
  requestRender(true);
  if (payload.diagnostic === true) void runWindowDiagnostic();
}

let diagnosticStarted = false;
async function runWindowDiagnostic() {
  if (diagnosticStarted) return;
  diagnosticStarted = true;
  const wait = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
  try {
    await invoke("detail_diagnostic_stage", { stage: "begin" });
    await wait(500);
    await invoke("detail_diagnostic_stage", { stage: "initial-render" });
    const rendered = $playerName.textContent === "Example player" && $content.querySelector(".empty") === null;
    const initialRenderMs = lastRenderDurationMs;
    const originalTable = $content.querySelector(".skill-table");
    const previousMarkup = lastContentMarkup;
    const previousScope = lastContentScope;
    render();
    const unchangedDomReused = $content.querySelector(".skill-table") === originalTable;
    let firstDifference = -1;
    if (previousMarkup !== lastContentMarkup) {
      for (let index = 0; index < Math.max(previousMarkup?.length || 0, lastContentMarkup?.length || 0); index += 1) {
        if (previousMarkup?.[index] !== lastContentMarkup?.[index]) { firstDifference = index; break; }
      }
    }
    const domChange = unchangedDomReused ? null : {
      scopeChanged: previousScope !== lastContentScope,
      firstDifference,
      before: previousMarkup?.slice(Math.max(0, firstDifference - 20), firstDifference + 50),
      after: lastContentMarkup?.slice(Math.max(0, firstDifference - 20), firstDifference + 50),
    };
    $content.scrollTop = 120;
    const beforeScroll = $content.scrollTop;
    const initialSkillScroll = $content.querySelector(".skill-table-scroll");
    initialSkillScroll.scrollTop = 120;
    const beforeSkillScroll = initialSkillScroll.scrollTop;
    frozenRecord.playerSkillStats[selectedActorId]["10000000"].totalDamage += 1;
    render();
    const changedDomUpdated = $content.querySelector(".skill-table") !== originalTable;
    const scrollPreserved = beforeSkillScroll > 0 && $content.scrollTop === beforeScroll
      && $content.querySelector(".skill-table-scroll").scrollTop === beforeSkillScroll;
    lastSnapshot = {
      combatInfos: frozenRecord.combatInfos,
      byTargetPlayerStats: { [frozenRecord.targetId]: frozenRecord.playerStats },
      byTargetPlayerSkillStats: { [frozenRecord.targetId]: frozenRecord.playerSkillStats },
    };
    selectedTargetId = frozenRecord.targetId;
    mode = "live";
    render();
    const liveRendered = $playerName.textContent === "Example player" && $content.querySelector(".skill-table") !== null;
    let snapshotPending = true;
    const slowSnapshot = invoke("get_last_snapshot").finally(() => { snapshotPending = false; });
    const started = performance.now();
    await wait(50);
    const heartbeatMs = performance.now() - started;
    const closeStartedAt = performance.now();
    document.getElementById("close-btn").click();
    await invoke("detail_diagnostic_stage", { stage: "close-clicked" });
    await lastCloseOperation;
    const buttonClosed = !(await detailWindow.isVisible());
    const closeDuringSnapshotMs = performance.now() - closeStartedAt;
    const closedWhileSnapshotPending = snapshotPending;
    await invoke("detail_diagnostic_stage", { stage: "button-closed" });
    await detailWindow.show();
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
    await lastCloseOperation;
    const escapeClosed = !(await detailWindow.isVisible());
    await invoke("detail_diagnostic_stage", { stage: "escape-closed" });
    let repeatedCloseCycles = 0;
    for (let cycle = 0; cycle < 10; cycle += 1) {
      await detailWindow.show();
      await invoke("detail_diagnostic_stage", { stage: `cycle-${cycle}-shown` });
      document.getElementById("close-btn").click();
      await lastCloseOperation;
      if (await detailWindow.isVisible()) break;
      repeatedCloseCycles += 1;
    }
    // WebView may defer async IPC callbacks while hidden. Reopen through its
    // native API before awaiting the test reply; never rely on hidden timers.
    await detailWindow.show();
    await slowSnapshot;
    await invoke("detail_diagnostic_report", { report: { rendered, liveRendered, unchangedDomReused, domChange, changedDomUpdated, scrollPreserved, scrollPixels: beforeSkillScroll, initialRenderMs, heartbeatMs, buttonClosed, escapeClosed, repeatedCloseCycles, closedWhileSnapshotPending, closeDuringSnapshotMs } });
  } catch (error) {
    await invoke("detail_diagnostic_report", { report: { error: String(error) } });
  }
}

function render() {
  if (!selectedActorId) {
    $playerName.textContent = t("dps-detail.player");
    $playerServer.textContent = "";
    $playerPower.style.display = "none";
    $playerIcon.style.display = "none";
    $content.innerHTML = `<div class="empty">${t("dps-detail.empty")}</div>`;
    return;
  }

  // Gather data
  let playerNameText = `ID:${selectedActorId}`,
    playerClass = "",
    playerServer = "",
    playerPower = "",
    iconSrc = "";
  let totalDmg = 0,
    skillMap = {},
    targetInfo = null,
    currentTargetId = null,
    actorInfo = null,
    playerStats = null;

  // Unified data source: snapshot from live, or record from history
  const dataSource = mode === "history" && frozenRecord ? frozenRecord : lastSnapshot;
  if (!dataSource) {
    $content.innerHTML = `<div class="empty">${t("dps-detail.noData")}</div>`;
    return;
  }

  // ── Actor info ──
  actorInfo = dataSource.combatInfos?.actorInfos?.[selectedActorId] || null;
  playerNameText = actorInfo?.actorName || playerNameText;
  playerClass = actorInfo?.actorClass || "";
  playerServer = getServerName(actorInfo?.actorServerId);
  playerPower = fmtPower(actorInfo?.combatPower);
  iconSrc = playerClass ? getClassIcon(playerClass) : "";

  // ── Target info (for fight duration) ──
  if (mode === "history") {
    targetInfo = dataSource.targetInfo || null;
    currentTargetId = dataSource.targetId ?? targetInfo?.id ?? null;
  } else {
    const targetId = selectedTargetId ?? dataSource.combatInfos?.lastTargetByMainActor;
    currentTargetId = targetId ?? null;
    if (targetId != null) {
      targetInfo =
        dataSource.combatInfos?.targetInfos?.[targetId] || dataSource.lastTargetInfo || null;
    }
  }

  // ── Player overview stats ──
  if (mode === "history") {
    playerStats = dataSource.playerStats?.[selectedActorId] || null;
  } else {
    playerStats =
      (
        (selectedTargetId != null
          ? Object.values(dataSource.byTargetPlayerStats?.[selectedTargetId] || {})
          : dataSource.lastTargetAllPlayersOverviewStats) || []
      ).find((p) => p.actorId === selectedActorId) || null;
  }

  // ── Skill stats ──
  let rawSkillMap = {};
  if (mode === "history") {
    rawSkillMap = dataSource.playerSkillStats?.[selectedActorId] || {};
  } else {
    // Only show skills for the current boss target, not all targets merged together
    const lastTargetId = selectedTargetId ?? dataSource.combatInfos?.lastTargetByMainActor;
    const targetStats = dataSource.byTargetPlayerSkillStats || {};
    const targetSkillStats = (lastTargetId != null ? targetStats[lastTargetId] : null) || {};
    rawSkillMap = targetSkillStats[selectedActorId] || {};
  }
  skillMap = {};
  for (const [sid, st] of Object.entries(rawSkillMap)) {
    if (!skillMap[sid]) skillMap[sid] = emptySkillStats();
    mergeSkillStats(skillMap[sid], st);
  }
  for (const s of Object.values(skillMap)) {
    if (s.minDamage === 1e99) s.minDamage = 0;
    totalDmg += s.totalDamage;
  }

  // Titlebar
  playerNameText = actorInfo?.actorName || playerStats?.actorName || playerNameText;
  setMode(mode);
  $playerName.textContent = playerNameText;
  $playerServer.textContent = playerServer ? `[${playerServer}]` : "";
  if (playerPower) {
    $playerPowerText.textContent = playerPower;
    $playerPower.style.display = "";
  } else {
    $playerPower.style.display = "none";
  }
  if (iconSrc) {
    $playerIcon.src = iconSrc;
    $playerIcon.style.display = "";
  } else {
    $playerIcon.style.display = "none";
  }

  // Skills sorted
  const skills = Object.entries(skillMap).map(([sid, s]) => ({ skillId: sid, ...s }));
  skills.sort((a, b) => b.totalDamage - a.totalDamage);
  const maxDmg = skills.length > 0 ? skills[0].totalDamage : 1;
  const totalSkillDmg = skills.reduce((s, r) => s + r.totalDamage, 0);

  // Summary
  const totalHits = Math.max(1, playerStats?.counts || skills.reduce((s, r) => s + r.counts, 0));
  const fightStart = targetInfo?.targetStartTime?.[selectedActorId] || 0;
  const fightEnd = targetInfo?.targetLastTime?.[selectedActorId] || 0;
  const fightDur = Math.max(1, fightEnd - fightStart);
  const dps = totalDmg / fightDur;
  const allSpecials = skills.reduce((acc, s) => {
    for (const [k, v] of Object.entries(s.specialCounts || {})) acc[k] = (acc[k] || 0) + v;
    return acc;
  }, {});
  const critR = totalHits > 0 ? fmtPct(((allSpecials["CRITICAL"] || 0) / totalHits) * 100) : "--";
  const backR = totalHits > 0 ? fmtPct(((allSpecials["BACK"] || 0) / totalHits) * 100) : "--";
  const frontR = totalHits > 0 ? fmtPct(((allSpecials["FRONT"] || 0) / totalHits) * 100) : "--";
  const doubleR = totalHits > 0 ? fmtPct(((allSpecials["DOUBLE"] || 0) / totalHits) * 100) : "--";
  const perfectR = totalHits > 0 ? fmtPct(((allSpecials["PERFECT"] || 0) / totalHits) * 100) : "--";
  const parryR = totalHits > 0 ? fmtPct(((allSpecials["PARRY"] || 0) / totalHits) * 100) : "--";
  const multiR = totalHits > 0 ? fmtPct(((allSpecials["MULTIHIT"] || 0) / totalHits) * 100) : "--";

  // Spec dots from actor info
  const skillSpecMap = actorInfo?.actorSkillSpec || {};

  let html = `<div class="summary-grid summary-row4">`;
  html += `<div class="summary-box"><div class="summary-box__label">${t("dps-detail.damage")}</div><div class="summary-box__value">${fmtFull(totalDmg)}</div></div>`;
  html += `<div class="summary-box"><div class="summary-box__label">${t("dps-detail.dps")}</div><div class="summary-box__value dps">${fmtFull(dps)}</div></div>`;
  html += `<div class="summary-box"><div class="summary-box__label">${t("dps-detail.fight")}</div><div class="summary-box__value">${fmtDuration(fightDur)}</div></div>`;
  html += `<div class="summary-box"><div class="summary-box__label">${t("dps-detail.hits")}</div><div class="summary-box__value">${totalHits}</div></div>`;
  html += `</div><div class="summary-grid summary-row7">`;
  html += `<div class="summary-box"><div class="summary-box__label">${t("dps-detail.critical")}</div><div class="summary-box__value crit">${critR}</div></div>`;
  html += `<div class="summary-box"><div class="summary-box__label">${t("dps-detail.back")}</div><div class="summary-box__value back">${backR}</div></div>`;
  html += `<div class="summary-box"><div class="summary-box__label">${t("dps-detail.front")}</div><div class="summary-box__value front">${frontR}</div></div>`;
  html += `<div class="summary-box"><div class="summary-box__label">${t("dps-detail.double")}</div><div class="summary-box__value double">${doubleR}</div></div>`;
  html += `<div class="summary-box"><div class="summary-box__label">${t("dps-detail.perfect")}</div><div class="summary-box__value perfect">${perfectR}</div></div>`;
  html += `<div class="summary-box"><div class="summary-box__label">${t("dps-detail.parry")}</div><div class="summary-box__value parry">${parryR}</div></div>`;
  html += `<div class="summary-box"><div class="summary-box__label">${t("dps-detail.multi")}</div><div class="summary-box__value multi">${multiR}</div></div>`;
  html += `</div>`;

  // Skill table
  if (skills.length > 0) {
    html += `<div class="skill-table-wrap"><div class="skill-table-scroll"><div class="skill-table">`;
    // Header
    // short column labels where a locale has them (RU words are too long for the fixed grid)
    const col = (short, full) => { const v = t(`dps-detail.${short}`); return v && v !== `dps-detail.${short}` ? v : t(`dps-detail.${full}`); };
    html += `<div class="skill-header"><span>${t("dps-detail.skill")}</span><span>${t("dps-detail.spec")}</span><span>${t("dps-detail.count")}</span><span>${col("colCritical", "critical")}%</span><span>${col("colPerfect", "perfect")}%</span><span>${col("colDouble", "double")}%</span><span>${col("colFront", "front")}%</span><span>${col("colBack", "back")}%</span><span>${col("colParry", "parry")}%</span><span>${col("colMulti", "multi")}%</span><span>${col("colMultiHitDmg", "multiHitDmg")}</span><span>${t("dps-detail.min")}</span><span>${t("dps-detail.max")}</span><span>${t("dps-detail.avg")}</span><span>${t("dps-detail.total")}</span></div>`;
    for (const s of skills) {
      const sc = getSpecial(s, "CRITICAL");
      const bk = getSpecial(s, "BACK");
      const fr = getSpecial(s, "FRONT");
      const db = getSpecial(s, "DOUBLE");
      const pf = getSpecial(s, "PERFECT");
      const pa = getSpecial(s, "PARRY");
      const mu = getSpecial(s, "MULTIHIT");
      const muDmg = getSpecial(s, "MULTIHITDMG");
      // Build multi-hit tooltip: show each hit variant count & probability
      let muTip = "";
      if (muDmg > 0) {
        const lines = Object.entries(s.specialCounts || {})
          .filter(
            ([k, v]) => k.startsWith("MULTIHIT") && k !== "MULTIHIT" && k !== "MULTIHITDMG" && v > 0
          )
          .sort(
            ([a], [b]) => parseInt(a.replace("MULTIHIT", "")) - parseInt(b.replace("MULTIHIT", ""))
          )
          .map(([k, v]) => {
            const hits = k.replace("MULTIHIT", "");
            const pct = s.counts > 0 ? fmtPct((v / s.counts) * 100) : "--";
            return `${hits}hits: ${v} (${pct})`;
          });
        if (lines.length > 0) {
          muTip = "Multi-hit\n" + lines.join("\n");
        }
      }
      const avgDmg = s.counts > 0 ? Math.floor(s.totalDamage / s.counts) : 0;
      const pct = totalSkillDmg > 0 ? ((s.totalDamage / totalSkillDmg) * 100).toFixed(1) : "0.0";
      const dots = specDots(skillSpecMap[s.skillId]);
      const icon = skillIcon(s.skillId);
      const name = skillName(s.skillId);
      const skillLabel = `${name} (${s.skillId})`;

      html += `<div class="skill-row">`;
      html += `<span class="skill-name-cell" data-tooltip="${esc(skillLabel)}">${icon ? `<img class="skill-icon" src="${icon}" alt="" onerror="this.style.display='none'"/>` : ""}<span class="skill-name-text">${esc(name)}</span></span>`;
      html += `<span class="spec-dots">${dots.map((a) => `<span class="spec-dot${a ? " active" : ""}"></span>`).join("")}</span>`;
      html += `<span class="color-slate">${s.counts}</span>`;
      html += `<span class="color-rose">${s.counts > 0 ? fmtPct((sc / s.counts) * 100) : "--"}</span>`;
      html += `<span class="color-emerald">${s.counts > 0 ? fmtPct((pf / s.counts) * 100) : "--"}</span>`;
      html += `<span class="color-yellow">${s.counts > 0 ? fmtPct((db / s.counts) * 100) : "--"}</span>`;
      html += `<span class="color-cyan">${s.counts > 0 ? fmtPct((fr / s.counts) * 100) : "--"}</span>`;
      html += `<span class="color-indigo">${s.counts > 0 ? fmtPct((bk / s.counts) * 100) : "--"}</span>`;
      html += `<span class="color-slate">${s.counts > 0 ? fmtPct((pa / s.counts) * 100) : "--"}</span>`;
      html += `<span class="color-rose">${s.counts > 0 ? fmtPct((mu / s.counts) * 100) : "--"}</span>`;
      html += `<span class="color-slate"${muTip ? ` data-tooltip="${esc(muTip)}"` : ""}>${muDmg > 0 ? fmtFull(muDmg) : "--"}</span>`;
      html += `<span class="color-slate">${s.minDamage > 0 ? fmtFull(s.minDamage) : "--"}</span>`;
      html += `<span class="color-slate">${fmtFull(s.maxDamage)}</span>`;
      html += `<span class="color-slate">${fmtFull(avgDmg)}</span>`;
      html += `<span class="skill-total-cell"><div class="skill-total-bar" style="width:${pct}%"></div><span class="color-amber" style="position:relative;z-index:1">${fmtFull(s.totalDamage)} (${pct}%)</span></span>`;
      html += `</div>`;
    }
    html += `</div></div></div>`;
  } else {
    html += `<div class="empty">${t("dps-detail.noData")}</div>`;
  }
  html += `<div class="buff-panels">`;
  html += renderBuffPanel({
    id: "player",
    title: t("dps-detail.playerBuffs"),
    open: isPlayerBuffPanelOpen,
    buffs: getTargetBuffs(dataSource, selectedActorId),
    dataSource,
    fightStart,
  });
  html += renderBuffPanel({
    id: "boss",
    title: t("dps-detail.bossBuffs"),
    open: isBossBuffPanelOpen,
    buffs: getTargetBuffs(dataSource, currentTargetId),
    dataSource,
    fightStart,
  });
  html += `</div>`;
  const scope = `${mode}:${selectedActorId}:${currentTargetId ?? ""}:${frozenRecord?.id ?? ""}`;
  if (scope === lastContentScope && html === lastContentMarkup && $content.firstChild) return;
  const keepScroll = scope === lastContentScope;
  const scrollTop = keepScroll ? $content.scrollTop : 0;
  const oldSkillScroll = $content.querySelector(".skill-table-scroll");
  const skillTop = keepScroll ? oldSkillScroll?.scrollTop || 0 : 0;
  const skillLeft = keepScroll ? oldSkillScroll?.scrollLeft || 0 : 0;
  $content.innerHTML = html;
  lastContentMarkup = html;
  lastContentScope = scope;
  $content.scrollTop = scrollTop;
  const newSkillScroll = $content.querySelector(".skill-table-scroll");
  if (newSkillScroll) { newSkillScroll.scrollTop = skillTop; newSkillScroll.scrollLeft = skillLeft; }
  $content.querySelectorAll(".buff-panel").forEach((panel) => {
    panel.addEventListener("toggle", (event) => {
      const panelId = event.currentTarget.dataset.buffPanel;
      if (panelId === "player") {
        isPlayerBuffPanelOpen = event.currentTarget.open;
      } else if (panelId === "boss") {
        isBossBuffPanelOpen = event.currentTarget.open;
      }
    });
  });
}

// Register listeners before any IPC awaits so a selection that arrives during
// startup cannot be lost. Snapshot handlers only keep the newest value; render
// work is coalesced below and never delays close/drag controls.
const listenersReady = Promise.all([
  listen("language-changed", (event) => {
    currentLanguage = event.payload.language;
    setLanguage(currentLanguage);
    currentSkills = SKILLS[currentLanguage] || skillsEn;
    document.getElementById("close-btn").title = t("dps-detail.close");
    document.getElementById("close-btn").setAttribute("aria-label", t("dps-detail.close"));
    requestRender(true);
  }),
  listen("dps-snapshot", (event) => {
    snapshotGeneration += 1;
    lastSnapshot = event.payload;
    if (mode === "live" && selectedActorId) requestRender();
  }),
  listen("select-player-detail", async (event) => {
    const payload = event.payload;
    if (payload?.actorId) {
      applySelection(payload);
      return;
    }
    const generation = ++selectionGeneration;
    try {
      const latest = await invoke("get_detail_selection");
      if (generation === selectionGeneration) applySelection(latest);
    } catch (error) {
      console.error("[dps-detail] get_detail_selection failed:", error);
    }
  }),
]).catch((error) => {
  console.error("[dps-detail] event listener setup failed:", error);
});

// ── Init ──
(async function init() {
  document.getElementById("close-btn").title = t("dps-detail.close");
  document.getElementById("close-btn").setAttribute("aria-label", t("dps-detail.close"));
  const $emptyEl = document.querySelector(".empty");
  if ($emptyEl) $emptyEl.textContent = t("dps-detail.empty");

  try {
    await listenersReady;
    try {
      currentLanguage = await invoke("get_language");
      setLanguage(currentLanguage);
      currentSkills = SKILLS[currentLanguage] || skillsEn;
    } catch (error) {
      console.error("[dps-detail] get_language failed:", error);
    }

    const generation = selectionGeneration;
    try {
      const selection = await invoke("get_detail_selection");
      if (selectionGeneration === generation) applySelection(selection);
    } catch (error) {
      console.error("[dps-detail] initial selection failed:", error);
    }
    if (mode === "live") {
      const capturedGeneration = snapshotGeneration;
      try {
        const snap = await invoke("get_last_snapshot");
        if (snap && capturedGeneration === snapshotGeneration) lastSnapshot = snap;
      } catch (error) {
        console.error("[dps-detail] get_last_snapshot failed:", error);
      }
    }
    requestRender(true);
  } catch (error) {
    console.error("[dps-detail] initialization failed:", error);
  }
})();
