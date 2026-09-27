import { installDevBrowserShim } from "@/lib/dev-browser-shim";

installDevBrowserShim();

import React, { lazy, useEffect } from "react";
import { ThemeProvider } from "./components/theme-provider";
import ReactDOM from "react-dom/client";
import { BrowserRouter, Navigate, Outlet, Route, Routes } from "react-router-dom";

import { TooltipProvider } from "@/components/ui/tooltip";

import { WindowFrame } from "./components/window-frame";
import { MainTitleBar } from "./components/main-title-bar";
import { PreflightBanner } from "./components/preflight-banner";
import { useSettings } from "@/hooks/use-settings";
import { useEmbeddedConfig } from "@/hooks/use-embedded-config";

const SettingsViewPage = lazy(() => import("./pages/settings"));
const Aion2HomePage = lazy(() => import("./games/aion2/pages/home"));
const Aion2OnTopPage = lazy(() => import("./games/aion2/pages/always-on-top"));

const Aion2OverlaySettingPage = lazy(() => import("./games/aion2/overlay/setting/page"));

import "./index.css";
import "./i18n";
import i18n from "./i18n";

function AppWrapper({ embedded }: { embedded: boolean }) {
  useSettings(); // trigger initial sync on app start (shortcuts, config, etc.)

  return (
    <Routes>
      {/* Main window for aion2*/}
      <Route
        element={
          <WindowFrame
            titleBar={embedded ? null : <MainTitleBar />}
            showSidebar={!embedded}
            embedded={embedded}
            contentClassName="overflow-auto"
          >
            <PreflightBanner />
            <Outlet />
          </WindowFrame>
        }
      >
        {/* Shared pages */}
        <Route path="/" element={<Navigate to="/aion2" replace />} />
        <Route path="/settings-view" element={<SettingsViewPage />} />

        {/* AION */}
        <Route path="/aion2" element={<Aion2HomePage />} />
        <Route path="/aion2/on-top" element={<Aion2OnTopPage />} />
      </Route>

      {/* Overlay windows (no main frame) */}
      <Route element={<Outlet />}>
        <Route path="/aion2/overlay_setting" element={<Aion2OverlaySettingPage />} />
      </Route>
    </Routes>
  );
}

/**
 * TASK-11: the one round trip that decides whether this launch is a normal
 * standalone app or a panel hosted by the farm tracker. `config` stays
 * `null` for the brief window this takes, and nothing renders meanwhile --
 * showing the sidebar/title bar even for a frame before hiding them again
 * would be worse than a blank window for a moment.
 */
function Root() {
  const config = useEmbeddedConfig();

  useEffect(() => {
    if (config?.embedded && config.lang) void i18n.changeLanguage(config.lang);
  }, [config]);

  if (config === null) return null;

  // "nebula"/"nebula-light" are the host's own names for the same two
  // palettes this app's Settings -> Appearance already offers as dark/light
  // (see docs/DBAION2.md) -- an unrecognised or absent --theme just falls
  // back to this app's own remembered choice.
  const forcedTheme =
    config.embedded && config.theme === "nebula"
      ? "dark"
      : config.embedded && config.theme === "nebula-light"
        ? "light"
        : undefined;

  return (
    <ThemeProvider defaultTheme="dark" storageKey="tauri-ui-theme" forcedTheme={forcedTheme}>
      <BrowserRouter>
        <TooltipProvider>
          <AppWrapper embedded={config.embedded} />
        </TooltipProvider>
      </BrowserRouter>
    </ThemeProvider>
  );
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <Root />
  </React.StrictMode>
);
