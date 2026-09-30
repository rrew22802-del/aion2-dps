import { cn } from "@/lib/utils";
import { useEffect, useRef, useState, type ReactNode } from "react";
import { NavLink, useLocation } from "react-router-dom";
import { ALL_GAMES, getGameByPath, type NavItem } from "@/game-config";
import { PanelLeftClose, PanelLeftOpen, Settings } from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useTranslation } from "react-i18next";

type WindowFrameProps = {
  titleBar: ReactNode;
  children: ReactNode;
  className?: string;
  contentClassName?: string;
  showSidebar?: boolean;
  /** Hosted inside the farm tracker's own window (TASK-11, `--embedded`):
   * an opaque themed background instead of the transparent-over-desktop
   * look, and no rounded corners or drop shadow of its own -- the tracker's
   * window frame is the only outer border now. */
  embedded?: boolean;
};

function SidebarNavItem({
  path,
  label,
  icon: Icon,
  expanded,
  activePaths,
}: NavItem & { expanded: boolean }) {
  const { t } = useTranslation();
  const location = useLocation();
  const paths = activePaths ?? [path];
  const isActive = paths.includes(location.pathname);
  const translatedLabel = t(label);

  return (
    <NavLink
      to={path}
      className={cn("sidebar-nav-link", isActive && "sidebar-nav-active", !expanded && "justify-center")}
      title={expanded ? undefined : translatedLabel}
      aria-label={translatedLabel}
    >
      <Icon className="size-4 shrink-0" />
      {expanded && <span className="truncate">{translatedLabel}</span>}
    </NavLink>
  );
}

function WindowSidebar() {
  const { t } = useTranslation();
  const location = useLocation();
  const [expanded, setExpanded] = useState(true);
  const gameConfig = getGameByPath(location.pathname) ?? ALL_GAMES[0];
  const navItems = gameConfig?.navItems ?? [];
  const SIDEBAR_STORAGE_KEY = "noia-main-sidebar-expanded";

  useEffect(() => {
    const stored = window.localStorage.getItem(SIDEBAR_STORAGE_KEY);
    if (stored !== null) setExpanded(stored === "true");
  }, []);

  const toggleExpanded = () => {
    setExpanded((current) => {
      const next = !current;
      window.localStorage.setItem(SIDEBAR_STORAGE_KEY, String(next));
      return next;
    });
  };

  return (
    <aside
      className={cn(
        "main-window-sidebar relative z-10 shrink-0 overflow-hidden transition-[width] duration-200 ease-out",
        expanded ? "w-[220px]" : "w-16"
      )}
    >
      <div className="flex h-full flex-col px-3 pt-3 pb-3">
        <nav className="flex flex-col gap-1.5">
          {navItems.map((item) => (
            <SidebarNavItem key={item.path} {...item} expanded={expanded} />
          ))}
        </nav>

        <div className="mt-auto flex flex-col gap-1.5">
          <SidebarNavItem
            path="/settings-view"
            label="nav.settings"
            icon={Settings}
            expanded={expanded}
          />
          <button
            type="button"
            className={cn(
              "sidebar-nav-link w-full",
              expanded ? "justify-start" : "justify-center"
            )}
            onClick={toggleExpanded}
            aria-label={t(expanded ? "nav.collapse" : "nav.expand")}
            title={expanded ? undefined : t("nav.expand")}
          >
            {expanded ? (
              <PanelLeftClose className="size-4 shrink-0" />
            ) : (
              <PanelLeftOpen className="size-4 shrink-0" />
            )}
            {expanded && <span className="truncate">{t("nav.collapse")}</span>}
          </button>
        </div>
      </div>
    </aside>
  );
}

export function WindowFrame({
  titleBar,
  children,
  className,
  contentClassName,
  showSidebar = true,
  embedded = false,
}: WindowFrameProps) {
  const location = useLocation();
  const gameConfig = getGameByPath(location.pathname);
  const isHomePage =
    !embedded &&
    (location.pathname === "/" ||
      (gameConfig != null && location.pathname === gameConfig.rootPath));
  const bgVideoRef = useRef<HTMLVideoElement>(null);

  // Pause the background video whenever nobody can see it. Focus covers
  // minimising and alt-tabbing; the Page Visibility API additionally covers
  // states the webview reports but the window API does not.
  useEffect(() => {
    const setPlaying = (playing: boolean) => {
      const video = bgVideoRef.current;
      if (!video) return;
      if (playing) {
        void video.play().catch(() => {});
      } else {
        video.pause();
      }
    };

    const onVisibilityChange = () => setPlaying(!document.hidden);
    document.addEventListener("visibilitychange", onVisibilityChange);

    let unlisten: (() => void) | undefined;
    void (async () => {
      try {
        unlisten = await getCurrentWindow().onFocusChanged(({ payload: focused }) => {
          setPlaying(focused && !document.hidden);
        });
      } catch (_) {
        /* window API unavailable; visibility handling still applies */
      }
    })();

    return () => {
      document.removeEventListener("visibilitychange", onVisibilityChange);
      unlisten?.();
    };
  }, []);

  return (
    <div
      className={cn(
        "flex h-screen w-screen flex-col overflow-hidden",
        embedded ? "bg-background rounded-none" : "main-window-frame rounded-2xl",
        className
      )}
    >
      {gameConfig?.bgImage && (
        <div
          className="bg-artwork absolute inset-0 bg-cover bg-center bg-no-repeat opacity-70"
          style={{ backgroundImage: `url("${gameConfig.bgImage}")` }}
        />
      )}
      <div className="via-background/35 to-background/45 pointer-events-none absolute inset-0 bg-gradient-to-b from-transparent" />
      <div className="from-background/55 to-background/60 pointer-events-none absolute inset-0 bg-gradient-to-r via-transparent" />
      <div className="bg-background/20 pointer-events-none absolute inset-0" />

      {isHomePage && gameConfig?.bgVideo ? (
        <video
          ref={bgVideoRef}
          className="bg-artwork absolute inset-0 h-full w-full object-cover"
          // The still is a frame of the same video, so the hand-over from
          // poster to playback is not a black flash.
          poster={gameConfig.bgImage}
          autoPlay
          muted
          loop
          playsInline
          disablePictureInPicture
          preload="auto"
          aria-hidden
        >
          <source src={gameConfig.bgVideo} type="video/mp4" />
        </video>
      ) : null}
      {/* {titleBar} */}
      <div className={cn("relative z-20", isHomePage ? "bg-transparent" : "bg-background/62")}>
        {titleBar}
      </div>
      <main className="min-h-0 flex-1">
        <div className="relative flex h-full min-h-0">
          {showSidebar && <WindowSidebar />}

          <section className="relative z-10 min-h-0 min-w-0 flex-1 p-0 pt-0">
            <div
              className={cn(
                "relative h-full min-h-0 overflow-hidden",
                embedded
                  ? "bg-transparent shadow-none ring-0"
                  : isHomePage
                    ? "rounded-2xl bg-transparent shadow-none ring-0"
                    : "bg-background/52 rounded-2xl shadow-[0_20px_70px_rgba(0,0,0,0.2)]",
                contentClassName
              )}
            >
              <div className="relative flex h-full flex-col">
                <div className="min-h-0 flex-1 overflow-hidden px-0 pb-0">
                  <div className="scrollbar-thumb-only h-full overflow-auto ring-black/5">
                    {children}
                  </div>
                </div>
              </div>
            </div>
          </section>
        </div>
      </main>
    </div>
  );
}
