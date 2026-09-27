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

- **Icon**: the task asks for dbaion2's own site-star icon
  (`aion2-farm-tracker/assets/app.ico` / `app-256.png`) in place of Aether's. This session's
  own tooling refused to copy that file from the farm-tracker repository into this one
  (a cross-repository publish guard), so `app-icon.png` and everything under `src-tauri/icons/`
  and `public/icon.png` are still Aether's own icon. Regenerate them locally with:
  ```
  cp <path-to>/aion2-farm-tracker/assets/app-256.png ./app-icon.png
  pnpm tauri icon app-icon.png
  ```
  then also replace `public/icon.png` (the dev favicon) with the same source, scaled down.
- **Farm Tracker Pro detection is unconfirmed.** `farm_tracker.rs` checks a short list of
  plausible install paths (`%ProgramFiles%`, `%ProgramFiles(x86)%`, `%LocalAppData%`, and a
  couple of side-by-side-portable guesses) rather than a registry uninstall key, because the
  tracker does not currently ship an installer that would register one. Nobody has run this
  against a real Farm Tracker Pro install yet. If it doesn't find it, the button falls back to
  opening `https://dbaion2.ru/tracker/`, so a wrong guess here is never silent.
- **Build was verified on Linux only** (`pnpm build`, `cargo check --target
  x86_64-pc-windows-gnu`, both from `src-tauri`) — there is no Windows machine in this sandbox to
  run `pnpm tauri:dev` / `pnpm tauri:build` (packet capture, WinDivert, the NSIS installer, and
  the elevated-terminal requirement are all Windows-only). Please do a real Windows build before
  merging.
