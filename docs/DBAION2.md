# DBAion2 DPS — rebrand notes (TASK-10)

This file is the authoritative record of the second fork step: **Aether → DBAion2 DPS**,
dbaion2.ru's own free, open-source build. [FORK.md](../FORK.md) still covers the first step,
**NOIA2 → Aether**, and stays accurate for that lineage — nothing there was rewritten.

## Why a rebrand, not a new repo

The owner's decision: the DPS meter stays **free and open source**, GPL-3.0, as a public fork
of Aether — this repository. The paid part (the farm tracker, `rrew22802-del/aion2-farm-tracker`,
C#/WPF, closed) stays private and separate. The two are different processes: this app never
embeds the tracker's code or vice versa, and neither repository's source crosses into the other.
The only link between them is `about-settings.tsx`'s "Farm Tracker Pro" button, which looks for
the tracker's own installed `.exe` and launches it, or falls back to opening its download page —
see `src-tauri/src/plugins/farm_tracker.rs`.

## What changed here

- **Branding**: `productName`/`mainBinaryName` → "DBAion2 DPS", `identifier` →
  `ru.dbaion2.dps` (was `com.helveticxa.aether`), window and splashscreen titles, the NSIS
  installer name and release name, `package.json`'s own `name`. The handful of remaining
  "Aether" strings in Rust doc-comments and internal log lines were left as-is — they are not
  user-facing, and this pass focused on what a player actually sees.
- **Theme**: `src/index.css`'s `:root` (light) and `.dark` semantic colour variables now hold
  dbaion2.ru's own Nebula / Nebula Light palette values (copied from
  `aion2-farm-tracker/Themes/Nebula.xaml` and `NebulaLight.xaml` — WPF's 8-digit `#AARRGGBB`
  colours flattened to solid RGB, since this app already layers its own translucency with
  Tailwind opacity utilities on top of these variables, same as it did with the neutral greys
  they replace). Layout and behaviour are untouched — same light/dark/system toggle in Settings
  → Appearance, just recoloured.
- **Russian locale**: `src/i18n/locales/ru.json` (the main UI) and
  `src/i18n/locales/aion2overlay/ru.json` (the overlay windows) were added and registered
  (`src/i18n/index.ts`, `src/games/aion2/i18n.js`, `language-toggle.tsx`). **Not** translated:
  `src/i18n/locales/aion2skills/*.json` — that is game data (skill-name lookups by numeric id,
  ~200 KB of it), not UI wording, and out of scope for "add a `ru` locale". English is the
  fallback language, so nothing breaks where Russian is missing.
- **About**: `src/components/about-settings.tsx` now shows "DBAion2 DPS", "Based on Aether and
  NOIA2 (GPL-3.0)", a GPL-3.0 license link, a no-warranty line, a source-code link to this
  repository, a link to dbaion2.ru's own database, and the Farm Tracker Pro button above.
  `src/components/support-acknowledgements-settings.tsx` (Credits) keeps every existing credit
  (NOIA2, the reference implementations, Kuroukihime's game data) and adds Aether itself as this
  build's own upstream.
- **Auto-update disabled**: Aether shipped a startup check against
  `Helveticxa/Aether-Aion2-DPS-meter-Global`'s own releases (`src/main.tsx`'s
  `AutoUpdaterDialog`, mounted on every page) plus a manual "Check for Updates" button in About.
  Both are gone, along with the `tauri-plugin-updater` Rust plugin, the `@tauri-apps/plugin-updater`
  dependency, the `plugins.updater` block in `tauri.conf.json` (endpoint + pubkey), and the
  `updater` bundle target in `package.json`'s `tauri:build` script and in
  `.github/workflows/release.yml`. This build has no update feed of its own yet — new versions
  are plain GitHub releases, same as any other tag push.
- **Removed non-GPL asset**: Aether's animated home-screen background, "Dune" by R (Vimeo,
  not GPL-licensed), and its still frame and credits thumbnail
  (`public/aion2/bg-dune.mp4`, `background-dune.webp`, `credits/dune.webp`) are deleted, along
  with the credit entry and the `bgVideo`/`bgImage` fields in `src/game-config.ts`. Nothing
  replaces them as a new binary asset: `window-frame.tsx`'s existing background gradients,
  tinted by `--background`, are "our own theme" once that variable holds dbaion2's colour
  instead of upstream's black.
- **CI**: `.github/workflows/release.yml` drops the `updater` bundle target and the
  updater-config verification step, and renames the GitHub release title.

## Known gaps — please check before merging

- **Icon**: done — replaced locally (this sandbox's own tooling had refused the cross-repository
  file copy; see the PR history) and the main title bar now shows `public/dbaion2-mark.png`
  instead of the AION2 logo.
- **Farm Tracker Pro detection is unconfirmed.** `farm_tracker.rs` checks a short list of
  plausible install paths (`%ProgramFiles%`, `%ProgramFiles(x86)%`, `%LocalAppData%`, and a
  couple of side-by-side-portable guesses) rather than a registry uninstall key, because the
  tracker does not currently ship an installer that would register one. Nobody has run this
  against a real Farm Tracker Pro install yet. If it doesn't find it, the button falls back to
  opening `https://dbaion2.ru/tracker/`, so a wrong guess here is never silent.
- **Build was verified on Linux only** (`pnpm build`, `cargo check --target
  x86_64-pc-windows-gnu`, both from `src-tauri`) — there is no Windows machine in this sandbox to
  run `pnpm tauri:dev` / `pnpm tauri:build` (packet capture, WinDivert, the NSIS installer, and
  the elevated-terminal requirement are all Windows-only). `.github/workflows/build.yml` (added
  after this doc's first version) does run a real `windows-latest` build on every pull request,
  which is the actual verification for anything below this line too.

## TASK-11 part A + A+ — one app for the user: no splash, `--embedded` mode, overlays stay Aether

`docs/TASK-11-one-app.md` (in `aion2-farm-tracker`) makes the farm tracker the single program the
user opens; this app becomes a hosted panel inside it, started as a second process the tracker
launches and reparents into one of its own tabs (Win32 `SetParent`, no shared code — see "Why a
rebrand, not a new repo" above, same rule). Part B (the tracker doing the hosting) is a different
repository's own task, done separately; this section covers only part A (this repo) and part A+
(an owner follow-up the same day).

- **No more splash window.** `tauri.conf.json`'s `"splashscreen"` window, `src/pages/preflight-gate.tsx`,
  the `enter_app` command, and `preflight::passed()`/`mark_passed()` are all gone. The main window
  shows itself immediately (`lib.rs`'s `setup()` calls `main.show()` directly), and
  `src/components/preflight-banner.tsx` — a small dismissible banner mounted in `main.tsx`, not a
  blocking screen — polls the same `run_preflight` command and shows what's missing (with the same
  "Install Npcap" / "Get the installer" fixes as before, the latter now pointing at *this* repo's
  releases instead of Aether's). A failing required check no longer holds the app closed: the
  meter itself already refuses to start without a working capture backend, so the gate was only
  ever a presentation layer, and `plugins::system_tray::show_main_window` now always shows the
  main window rather than redirecting to a splash window that no longer exists.
- **`--embedded` launch mode** (`src-tauri/src/embedded.rs`): started with `--embedded
  [--theme nebula|nebula-light] [--lang ru|en] [--parent-hwnd <n>]`. No tray icon (the tray plugin
  is conditionally skipped in `lib.rs`'s `run()`), no title bar or sidebar of its own
  (`main.tsx` passes `titleBar={null}` / `showSidebar={false}` to `WindowFrame`), and the window
  background is the flat, opaque theme colour instead of the transparent-over-desktop look
  (`WindowFrame`'s new `embedded` prop). `--theme`/`--lang` map onto this app's *existing*
  dark/light and language settings rather than adding new ones: `nebula` is this app's own dark
  palette, `nebula-light` its light one (both already dbaion2's colours since TASK-10), applied
  through a new `forcedTheme` prop on `ThemeProvider` that never touches `localStorage` — a
  standalone user's own theme choice must survive being overwritten by whatever `--theme` the
  tracker happens to pass on its next embedded launch. Without `--embedded` nothing here changes
  behaviour at all.
- **Exit when the host goes away.** `embedded::embed_in` reparents the window (`SetParent` +
  `WS_CHILD`, mirroring the existing pattern in `plugins/on_top/win32.rs`) and starts a background
  thread polling `IsWindow` on the parent handle every 500 ms; the moment it stops being a window,
  this process calls `app.exit(0)`. That is the half of "close when the host closes" this repo can
  implement unilaterally. The task also mentions an explicit close *message* over "a named pipe or
  localhost" as an alternative signal — that wire format has to be agreed with the farm tracker's
  own code, a separate repository this fork must not share code with, so it is intentionally not
  designed from this side alone; HWND-liveness polling is the complete, working mechanism for now.
  Separately, skipping the tray plugin when embedded also fixes what would otherwise have been a
  real bug: the app's normal "close to tray" behaviour (`RunEvent::ExitRequested` calling
  `api.prevent_exit()`) reads an `AppLifecycleState` that only the tray plugin manages, so without
  that guard rewritten to default to "allow exit" when the state doesn't exist, an embedded launch
  would neither exit *nor* stop the DPS meter's capture thread on a close request.
- **A+ — the in-game overlay windows needed no changes at all.** The owner asked that the meter
  panel, detail/breakdown, history, PvP overlay, and toast/summary cards keep exactly upstream
  Aether's look, reverting any Nebula recolouring. Checking `git diff` for every commit since
  TASK-10 against `src/games/aion2/overlay/` turns up exactly three one-line changes, none of them
  colour: two system-notification title strings ("Aether" → "DBAion2 DPS", left as-is — that's the
  OS toast identifying which app sent it, not the overlay's own visual style) and one `ru` locale
  registration. TASK-10's theme work only ever touched `src/index.css`'s Tailwind variables, which
  the overlay windows never consume in the first place: each one is a standalone HTML document
  with its own `style.css` and its own hardcoded `:root` design tokens (`--overlay-bg`, `--text`,
  `--muted`, ...), completely separate from the main app's theme system. So "revert the
  recolouring" had nothing left to revert — worth stating plainly here so it doesn't look skipped.
