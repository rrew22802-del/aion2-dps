# TASK-14-A result

Implemented `--service` in Rust with no main/splash window or tray. Capture starts during setup; the in-game overlay starts unless `--no-overlay` is passed. The service skips the standalone single-instance plugin so it can run beside the desktop app. It binds `127.0.0.1` only, checks `x-dbaion2-token` on every request, watches the optional parent PID, and exits through Tauri on authenticated quit.

The v1 routes expose live snapshots, per-player skills, persisted history, reset, and health. They use the existing meter, history store, preflight checks, game-traffic detector, and ping tracker. New history records keep a stable fight ID from the first hit and preserve summon-owner flags. See [SERVICE-API.md](SERVICE-API.md).

Validation: no Rust or pnpm toolchain is installed on this machine, so compilation and runtime testing remain for `.github/workflows/build.yml`. A small Rust unit test covers service argument parsing, query parsing, class mapping, and the bundled skill catalogue lookup. No git commands were run.

Known data limit: upstream bundles English and Korean skill catalogues and an English NPC catalogue, with no Russian equivalents. Russian class names are returned; missing Russian target and skill names are `null` as required for unknown values.
