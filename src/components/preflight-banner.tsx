import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  CheckCircle2,
  ChevronDown,
  ChevronUp,
  Download,
  Loader2,
  RefreshCw,
  TriangleAlert,
  X,
} from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";

/** Mirrors `dps_meter::preflight` on the Rust side. */
type CheckId = "elevation" | "npcap" | "windivert";
type Weight = "required" | "optional";
type Fix = "install-npcap" | "reinstall-aether";

type Check = {
  id: CheckId;
  label: string;
  detail: string;
  ok: boolean;
  weight: Weight;
  fix: Fix | null;
};

type Report = {
  ready: boolean;
  summary: string;
  checks: Check[];
};

type InstallOutcome = {
  launched: boolean;
  steps: string[];
  error: string | null;
};

// This build's own releases, not upstream Aether's -- a reinstall has to
// fetch the DBAion2 DPS installer, which is what actually bundles the driver
// this check is complaining about.
const RELEASES_URL = "https://github.com/rrew22802-del/aion2-dps/releases/latest";

const POLL_MS = 4000;

function CheckRow({
  check,
  busy,
  onFix,
}: {
  check: Check;
  busy: boolean;
  onFix: (fix: Fix) => void;
}) {
  const blocking = !check.ok && check.weight === "required";

  return (
    <div
      className={
        blocking
          ? "flex items-start gap-2.5 rounded-lg border border-amber-300/25 bg-amber-300/[0.06] px-3 py-2.5"
          : "border-border/55 bg-background/40 flex items-start gap-2.5 rounded-lg border px-3 py-2.5"
      }
    >
      <div className="mt-0.5 flex size-6 shrink-0 items-center justify-center rounded-md bg-white/5">
        {check.ok ? (
          <CheckCircle2 className="size-3.5 text-cyan-300" />
        ) : blocking ? (
          <TriangleAlert className="size-3.5 text-amber-300" />
        ) : (
          <span className="h-px w-2.5 rounded bg-zinc-600" />
        )}
      </div>

      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          <span className="text-foreground text-xs font-medium">{check.label}</span>
          {check.weight === "optional" && !check.ok && (
            <span className="rounded border border-white/10 px-1.5 py-px text-[9px] tracking-wide text-zinc-500 uppercase">
              Optional
            </span>
          )}
        </div>
        <p className="mt-0.5 text-[11px] leading-relaxed text-zinc-400">{check.detail}</p>

        {check.fix && !check.ok && (
          <button
            type="button"
            className="mt-1.5 flex h-6 items-center gap-1.5 rounded-md border border-cyan-200/14 bg-cyan-300/10 px-2 text-[11px] font-medium text-cyan-100 transition-colors hover:bg-cyan-300/16 disabled:cursor-not-allowed disabled:opacity-60"
            disabled={busy}
            onClick={() => onFix(check.fix as Fix)}
          >
            {check.fix === "install-npcap" ? (
              <>
                <Download className="size-3" />
                Install Npcap
              </>
            ) : (
              <>
                <RefreshCw className="size-3" />
                Get the installer
              </>
            )}
          </button>
        )}
      </div>
    </div>
  );
}

/**
 * The startup checks (TASK-11), as a dismissible banner inside the main
 * window instead of a separate splash window that held the whole app closed.
 * A failing *required* check no longer blocks entry -- it just says so here,
 * with the same fixes the old gate offered -- because the meter itself
 * already refuses to start without them (see `start_dps_meter`), so nothing
 * is gained by also refusing to show the rest of the app.
 */
export function PreflightBanner() {
  const [report, setReport] = useState<Report | null>(null);
  const [checking, setChecking] = useState(true);
  const [installing, setInstalling] = useState(false);
  const [steps, setSteps] = useState<string[]>([]);
  const [failure, setFailure] = useState<string | null>(null);
  const [dismissed, setDismissed] = useState(false);
  const [expanded, setExpanded] = useState(false);

  const alive = useRef(true);
  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);

  const runChecks = useCallback(async () => {
    setChecking(true);
    setFailure(null);
    try {
      const next = await invoke<Report>("run_preflight");
      if (!alive.current) return null;
      setReport(next);
      if (next.ready) setDismissed(false);
      return next;
    } catch (error) {
      if (!alive.current) return null;
      setReport(null);
      setFailure(error instanceof Error ? error.message : String(error));
      return null;
    } finally {
      if (alive.current) setChecking(false);
    }
  }, []);

  // Polls only while something is actually wrong -- once ready, this stops
  // on its own rather than checking forever in the background.
  useEffect(() => {
    let cancelled = false;
    let timer = 0;

    const tick = async () => {
      const next = await runChecks();
      if (cancelled) return;
      if (!next?.ready) timer = window.setTimeout(() => void tick(), POLL_MS);
    };
    void tick();

    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [runChecks]);

  const handleFix = useCallback(
    async (fix: Fix) => {
      if (fix === "reinstall-aether") {
        void openUrl(RELEASES_URL);
        return;
      }

      setInstalling(true);
      setSteps(["Fetching the official Npcap installer"]);
      let reason: string | null = null;
      try {
        const outcome = await invoke<InstallOutcome>("install_npcap");
        setSteps(outcome.steps);
        reason = outcome.error;
      } catch (error) {
        reason = error instanceof Error ? error.message : String(error);
      } finally {
        setInstalling(false);
      }

      await runChecks();
      if (alive.current && reason) setFailure(reason);
    },
    [runChecks]
  );

  if (dismissed) return null;
  if (checking && !report && !failure) return null; // no flash on the very first check
  if (report?.ready) return null;

  const busy = checking || installing;

  return (
    <div className="border-border/60 bg-background/85 relative z-30 mx-3 mt-3 shrink-0 rounded-xl border shadow-lg backdrop-blur-sm">
      <div className="flex items-center gap-2.5 px-3.5 py-2.5">
        {busy ? (
          <Loader2 className="size-4 shrink-0 animate-spin text-amber-300" />
        ) : (
          <TriangleAlert className="size-4 shrink-0 text-amber-300" />
        )}
        <div className="min-w-0 flex-1">
          <p className="text-foreground truncate text-xs font-medium">
            {checking
              ? "Checking what DBAion2 DPS needs"
              : (report?.summary ?? failure ?? "The environment check could not run")}
          </p>
        </div>
        <button
          type="button"
          className="flex size-6 shrink-0 items-center justify-center rounded-md text-zinc-500 transition-colors hover:bg-white/8 hover:text-zinc-100"
          onClick={() => setExpanded((current) => !current)}
          aria-label={expanded ? "Collapse" : "Details"}
        >
          {expanded ? <ChevronUp className="size-3.5" /> : <ChevronDown className="size-3.5" />}
        </button>
        <button
          type="button"
          className="flex size-6 shrink-0 items-center justify-center rounded-md text-zinc-500 transition-colors hover:bg-white/8 hover:text-zinc-100"
          onClick={() => setDismissed(true)}
          aria-label="Dismiss"
        >
          <X className="size-3.5" />
        </button>
      </div>

      {expanded && (
        <div className="border-border/60 flex flex-col gap-1.5 border-t px-3.5 py-3">
          {report?.checks.map((check) => (
            <CheckRow
              key={check.id}
              check={check}
              busy={busy}
              onFix={(fix) => void handleFix(fix)}
            />
          ))}

          {failure && report && (
            <p className="text-xs leading-relaxed text-amber-200/80">{failure}</p>
          )}

          {installing && steps.length > 0 && (
            <div className="mt-1 max-h-16 overflow-y-auto text-[11px] leading-relaxed text-zinc-400">
              {steps.map((step, index) => (
                <div key={`${index}-${step}`} className="truncate" title={step}>
                  {index + 1}. {step}
                </div>
              ))}
            </div>
          )}

          <button
            type="button"
            className="mt-0.5 flex h-6 w-fit items-center gap-1.5 rounded-md border border-white/10 px-2 text-[11px] font-medium text-zinc-400 transition-colors hover:bg-white/8 hover:text-zinc-100 disabled:cursor-not-allowed disabled:opacity-60"
            disabled={busy}
            onClick={() => void runChecks()}
          >
            {checking ? (
              <Loader2 className="size-3 animate-spin" />
            ) : (
              <RefreshCw className="size-3" />
            )}
            Re-check
          </button>
        </div>
      )}
    </div>
  );
}
