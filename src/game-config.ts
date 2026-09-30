import { Home, Pin, type LucideIcon } from "lucide-react";

export type NavItem = {
  label: string;
  path: string;
  icon: LucideIcon;
  activePaths?: string[];
};

export type GameConfig = {
  id: string;
  name: string;
  rootPath: string;
  navItems: NavItem[];
  bgVideo?: string;
  bgImage?: string;
};

export const AION2_GAME: GameConfig = {
  id: "aion2",
  name: "AION2",
  rootPath: "/aion2",
  navItems: [
    { label: "nav.home", path: "/aion2", icon: Home },
    { label: "nav.alwaysOnTop", path: "/aion2/on-top", icon: Pin },
  ],

  // No background video/image: the upstream "Dune" clip is not GPL-licensed
  // and was dropped for this fork (see docs/DBAION2.md). CSS supplies the
  // main window's Nebula background.
};

export const ALL_GAMES: GameConfig[] = [AION2_GAME];

export function getGameByPath(pathname: string): GameConfig | undefined {
  return ALL_GAMES.find((g) => pathname.startsWith(g.rootPath)) ?? ALL_GAMES[0];
}
