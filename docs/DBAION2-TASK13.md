# TASK-13: main window Nebula styling

- Added opaque CSS Nebula and Nebula Light gradients to the standalone main frame, replacing the see-through home background without binary artwork.
- Restyled the main sidebar as 220 px glass with 42 px icon-and-label rows, a gradient active pill, and Russian navigation labels. The collapse control and all routes remain available.
- Embedded mode still omits the title bar and sidebar; overlay routes and their styles were untouched.

Files: `src/index.css`, `src/components/window-frame.tsx`, `src/game-config.ts`, `src/i18n/locales/en.json`, `src/i18n/locales/ru.json`.

Check: both locale JSON files parse; static checks confirm the Nebula styles and embedded chrome flags. A frontend build was unavailable because this workspace has no installed dependencies or pnpm.
