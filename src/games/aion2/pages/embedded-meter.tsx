import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "react-i18next";

type Player = {
  actorId: number;
  actorName: string;
  totalDamage: number;
  dps: number;
  damageShare: number;
};
type Snapshot = {
  byTargetPlayerStats: Record<string, Record<string, Player>>;
  combatInfos: {
    lastTargetByMainActor?: number;
    lastTarget?: number;
    targetInfos: Record<string, { targetName?: string }>;
  };
};

export default function EmbeddedMeterPage() {
  const { t, i18n } = useTranslation();
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);
  const [error, setError] = useState("");
  const [opening, setOpening] = useState<number | null>(null);

  useEffect(() => {
    let alive = true;
    const refresh = async () => {
      try {
        const value = await invoke<Snapshot | null>("get_dps_snapshot");
        if (alive) setSnapshot(value);
      } catch (cause) {
        if (alive) setError(String(cause));
      }
    };
    void invoke("start_dps_meter")
      .then(() => {
        if (alive) setError("");
      })
      .catch((cause) => {
        if (alive) setError(String(cause));
      });
    void refresh();
    const timer = window.setInterval(() => void refresh(), 250);
    return () => {
      alive = false;
      window.clearInterval(timer);
    };
  }, []);

  const byTarget = snapshot?.byTargetPlayerStats ?? {};
  const preferred = snapshot?.combatInfos.lastTargetByMainActor ?? snapshot?.combatInfos.lastTarget;
  const targetId =
    preferred != null && byTarget[preferred] ? preferred : Number(Object.keys(byTarget)[0]);
  const players = Object.values(byTarget[targetId] ?? {}).sort(
    (a, b) => b.totalDamage - a.totalDamage
  );
  const targetName = snapshot?.combatInfos.targetInfos?.[targetId]?.targetName;
  const number = new Intl.NumberFormat(i18n.language, { maximumFractionDigits: 0 });

  const openDetail = async (actorId: number) => {
    setOpening(actorId);
    try {
      await invoke("set_detail_selection", { value: { actorId, targetId, mode: "live" } });
      await invoke("create_dps_detail");
      setError("");
    } catch (cause) {
      setError(String(cause));
    } finally {
      setOpening(null);
    }
  };

  return (
    <div className="h-full overflow-auto bg-background p-6 text-foreground">
      <div className="mx-auto max-w-4xl">
        <h1 className="text-xl font-semibold">{t("embeddedMeter.title")}</h1>
        <p className="mt-1 text-sm text-muted-foreground">
          {targetName ||
            (Number.isFinite(targetId)
              ? `${t("embeddedMeter.target")} #${targetId}`
              : t("embeddedMeter.waiting"))}
        </p>
        {error && (
          <p role="alert" className="mt-4 text-sm text-destructive">
            {error}
          </p>
        )}
        <div className="mt-6 overflow-hidden rounded-xl border border-border bg-card">
          <div className="grid grid-cols-[minmax(0,1fr)_repeat(3,minmax(5rem,auto))] gap-3 border-b border-border px-4 py-3 text-xs font-semibold text-muted-foreground">
            <span>{t("embeddedMeter.player")}</span>
            <span className="text-right">{t("embeddedMeter.damage")}</span>
            <span className="text-right">DPS</span>
            <span className="text-right">%</span>
          </div>
          {players.length === 0 ? (
            <p className="p-6 text-sm text-muted-foreground">{t("embeddedMeter.empty")}</p>
          ) : (
            players.map((player) => (
              <button
                key={player.actorId}
                type="button"
                disabled={opening !== null}
                onClick={() => void openDetail(player.actorId)}
                className="grid w-full grid-cols-[minmax(0,1fr)_repeat(3,minmax(5rem,auto))] gap-3 border-b border-border/50 px-4 py-3 text-left text-sm tabular-nums hover:bg-accent focus-visible:outline-2 focus-visible:outline-offset-[-2px] focus-visible:outline-ring disabled:opacity-60"
              >
                <span className="truncate">{player.actorName || `#${player.actorId}`}</span>
                <span className="text-right">{number.format(player.totalDamage)}</span>
                <span className="text-right">{number.format(player.dps)}</span>
                <span className="text-right">{number.format(player.damageShare * 100)}%</span>
              </button>
            ))
          )}
        </div>
      </div>
    </div>
  );
}
