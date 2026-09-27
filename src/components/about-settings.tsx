import { useCallback, useEffect, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";
import { ExternalLink, Github, RefreshCw, ScrollText, Trash2 } from "lucide-react";
import { openUrl } from "@tauri-apps/plugin-opener";

import packageJson from "../../package.json";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { SettingsGroup, SettingsRow, SettingsSectionHeader } from "@/components/settings-layout";
import { useAppTranslation } from "@/hooks/use-app-translation";
import { formatStorageSize, getLocalStorageSummary } from "@/lib/storage-summary";
import { unregisterAllShortcut } from "@/lib/shortcut";

const TECH_VERSIONS = [
  {
    name: "Tauri",
    version: packageJson.dependencies["@tauri-apps/api"].replace(/^\^/, "v"),
  },
  {
    name: "React",
    version: packageJson.dependencies.react.replace(/^\^/, "v"),
  },
  {
    name: "TypeScript",
    version: packageJson.devDependencies.typescript.replace(/^~/, "v"),
  },
  {
    name: "Vite",
    version: packageJson.devDependencies.vite.replace(/^\^/, "v"),
  },
];

const SOURCE_URL = "https://github.com/rrew22802-del/aion2-dps";
const GPL_URL = "https://www.gnu.org/licenses/gpl-3.0.html";
const DATABASE_URL = "https://dbaion2.ru/";
const TRACKER_WEB_URL = "https://dbaion2.ru/tracker/";

export function AboutSettings() {
  const [appVersion, setAppVersion] = useState("");
  const [clearStorageOpen, setClearStorageOpen] = useState(false);
  const [storageSummary, setStorageSummary] = useState(() => getLocalStorageSummary());
  const [openingTracker, setOpeningTracker] = useState(false);
  const { t } = useAppTranslation();

  const refreshStorageSummary = useCallback(() => {
    setStorageSummary(getLocalStorageSummary());
  }, []);

  useEffect(() => {
    void getVersion().then(setAppVersion);
  }, []);

  useEffect(() => {
    refreshStorageSummary();

    const handleFocus = () => refreshStorageSummary();
    const handleStorage = () => refreshStorageSummary();

    window.addEventListener("focus", handleFocus);
    window.addEventListener("storage", handleStorage);

    return () => {
      window.removeEventListener("focus", handleFocus);
      window.removeEventListener("storage", handleStorage);
    };
  }, [refreshStorageSummary]);

  const handleOpenGithub = useCallback(() => {
    void openUrl(SOURCE_URL);
  }, []);

  const handleOpenDatabase = useCallback(() => {
    void openUrl(DATABASE_URL);
  }, []);

  // Farm Tracker Pro is a separate, closed-source process (the paid part of
  // dbaion2's AION 2 tools) — this app never embeds it or shares code with it,
  // it only offers to start it if it finds it already installed, or sends the
  // person to the download page otherwise. Best-effort: it looks at a
  // registry App Paths entry and a couple of common install folders, none of
  // which have been confirmed yet against a real Farm Tracker Pro install.
  const handleOpenTracker = useCallback(async () => {
    setOpeningTracker(true);
    try {
      const launched = await invoke<boolean>("launch_farm_tracker_pro");
      if (!launched) void openUrl(TRACKER_WEB_URL);
    } catch (error) {
      console.error("[about] launch_farm_tracker_pro failed:", error);
      void openUrl(TRACKER_WEB_URL);
    } finally {
      setOpeningTracker(false);
    }
  }, []);

  const handleClearStorage = useCallback(async () => {
    try {
      await unregisterAllShortcut();
    } catch (error) {
      console.error("Failed to unregister global shortcuts before clearing cache:", error);
    }

    localStorage.clear();
    setClearStorageOpen(false);
    refreshStorageSummary();
    window.location.reload();
  }, [refreshStorageSummary]);

  return (
    <div className="flex flex-col gap-8">
      <SettingsSectionHeader title={t("about.appName")} description={t("about.description")} />

      <SettingsGroup title="Application">
        <SettingsRow label={t("about.appName")} description={t("about.basedOn")} control={null} />
        <SettingsRow
          label={t("about.version")}
          control={<span className="text-sm font-medium">{appVersion || "-"}</span>}
        />
        {TECH_VERSIONS.map((item) => (
          <SettingsRow
            key={item.name}
            label={item.name}
            control={<span className="text-sm font-medium">{item.version}</span>}
          />
        ))}
        <SettingsRow
          label={t("about.license")}
          description={t("about.noWarranty")}
          control={
            <Button variant="outline" size="sm" onClick={() => void openUrl(GPL_URL)}>
              <ScrollText data-icon="inline-start" />
              GPL-3.0
            </Button>
          }
        />
        <SettingsRow
          label={t("about.source")}
          description={t("about.sourceDesc")}
          control={
            <Button variant="outline" size="sm" onClick={handleOpenGithub}>
              <Github data-icon="inline-start" />
              GitHub
            </Button>
          }
        />
      </SettingsGroup>

      <SettingsGroup title={t("about.dbaion2Group")}>
        <SettingsRow
          label={t("about.database")}
          description={t("about.databaseDesc")}
          control={
            <Button variant="outline" size="sm" onClick={handleOpenDatabase}>
              <ExternalLink data-icon="inline-start" />
              dbaion2.ru
            </Button>
          }
        />
        <SettingsRow
          label={t("about.tracker")}
          description={t("about.trackerDesc")}
          control={
            <Button
              variant="outline"
              size="sm"
              onClick={() => void handleOpenTracker()}
              disabled={openingTracker}
            >
              <ExternalLink data-icon="inline-start" />
              Farm Tracker Pro
            </Button>
          }
        />
      </SettingsGroup>

      <SettingsGroup title="Local cache">
        <SettingsRow
          label="Current usage"
          description="Settings, search history, and runtime data are stored on this machine."
          control={
            <div className="flex items-center gap-2">
              <span className="text-sm font-semibold">
                {formatStorageSize(storageSummary.totalBytes)}
              </span>
              <Button variant="outline" size="sm" onClick={refreshStorageSummary}>
                <RefreshCw data-icon="inline-start" />
                Refresh
              </Button>
            </div>
          }
        />

        <SettingsRow
          label="Clear cache"
          description="Clear locally stored settings, search history, and cached data, then reload the window."
          control={
            <Button variant="destructive" size="sm" onClick={() => setClearStorageOpen(true)}>
              <Trash2 data-icon="inline-start" />
              Clear and reload
            </Button>
          }
        />

        {storageSummary.entries.length === 0 ? (
          <div className="text-muted-foreground px-5 py-6 text-sm">No local cache data.</div>
        ) : (
          storageSummary.entries.map((entry) => (
            <SettingsRow
              key={entry.key}
              label={entry.key}
              description={`${entry.bytes.toLocaleString()} bytes`}
              control={
                <span className="text-sm font-medium">{formatStorageSize(entry.bytes)}</span>
              }
            />
          ))
        )}
      </SettingsGroup>

      <Dialog open={clearStorageOpen} onOpenChange={setClearStorageOpen}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>Clear local cache?</DialogTitle>
            <DialogDescription>
              This clears the settings, search history, and cached data stored on this machine, then
              reloads the app window.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setClearStorageOpen(false)}>
              Cancel
            </Button>
            <Button
              variant="destructive"
              onClick={() => {
                void handleClearStorage();
              }}
            >
              Clear and reload
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
