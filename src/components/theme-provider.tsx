import { createContext, useContext, useEffect, useState } from "react";

type Theme = "dark" | "light" | "system";

type ThemeProviderProps = {
  children: React.ReactNode;
  defaultTheme?: Theme;
  storageKey?: string;
  /**
   * Overrides the stored/system theme without ever reading or writing
   * `storageKey` -- for an `--embedded` launch (TASK-11), where the farm
   * tracker's own `--theme` flag decides, and must not overwrite whatever
   * theme this app remembers from being run standalone.
   */
  forcedTheme?: "light" | "dark";
};

type ThemeProviderState = {
  theme: Theme;
  setTheme: (theme: Theme) => void;
};

const initialState: ThemeProviderState = {
  theme: "system",
  setTheme: () => null,
};

const ThemeProviderContext = createContext<ThemeProviderState>(initialState);

export function ThemeProvider({
  children,
  defaultTheme = "system",
  storageKey = "vite-ui-theme",
  forcedTheme,
  ...props
}: ThemeProviderProps) {
  const [theme, setThemeState] = useState<Theme>(
    () => (localStorage.getItem(storageKey) as Theme) || defaultTheme
  );
  const effectiveTheme = forcedTheme ?? theme;

  useEffect(() => {
    const root = window.document.documentElement;
    root.classList.remove("light", "dark");

    if (effectiveTheme === "system") {
      const systemTheme = window.matchMedia("(prefers-color-scheme: dark)").matches
        ? "dark"
        : "light";
      root.classList.add(systemTheme);
      return;
    }

    root.classList.add(effectiveTheme);
  }, [effectiveTheme]);

  // Listen for localStorage changes to sync theme across windows. Irrelevant
  // when forced: there is nothing stored to have changed.
  useEffect(() => {
    if (forcedTheme) return;
    const handleStorageChange = (e: StorageEvent) => {
      if (e.key === storageKey && e.newValue) {
        setThemeState(e.newValue as Theme);
      }
    };

    window.addEventListener("storage", handleStorageChange);
    return () => window.removeEventListener("storage", handleStorageChange);
  }, [storageKey, forcedTheme]);

  const value = {
    theme: effectiveTheme,
    setTheme: (newTheme: Theme) => {
      if (forcedTheme) return;
      localStorage.setItem(storageKey, newTheme);
      setThemeState(newTheme);
    },
  };

  return (
    <ThemeProviderContext.Provider {...props} value={value}>
      {children}
    </ThemeProviderContext.Provider>
  );
}

export const useTheme = () => {
  const context = useContext(ThemeProviderContext);
  if (context === undefined) throw new Error("useTheme must be used within a ThemeProvider");
  return context;
};
