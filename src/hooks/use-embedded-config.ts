import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

/** Mirrors `embedded::EmbeddedConfig` on the Rust side (TASK-11). */
export type EmbeddedConfig = {
  embedded: boolean;
  theme: string | null;
  lang: string | null;
};

const STANDALONE: EmbeddedConfig = { embedded: false, theme: null, lang: null };

/**
 * Whether this launch came from the farm tracker (`--embedded`, see
 * `docs/DBAION2.md`), and with what theme/language it asked for.
 *
 * `null` while the one-time round trip to the backend is still in flight —
 * callers that must not flash the wrong chrome (the sidebar, the title bar)
 * should wait for a non-null value before deciding what to render.
 */
export function useEmbeddedConfig(): EmbeddedConfig | null {
  const [config, setConfig] = useState<EmbeddedConfig | null>(null);

  useEffect(() => {
    let alive = true;
    void invoke<EmbeddedConfig>("get_embedded_config")
      .then((result) => {
        if (alive) setConfig(result);
      })
      .catch((error) => {
        console.error("[embedded] get_embedded_config failed:", error);
        if (alive) setConfig(STANDALONE);
      });
    return () => {
      alive = false;
    };
  }, []);

  return config;
}
