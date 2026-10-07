//! Launches Farm Tracker Pro — dbaion2's separate, closed-source, paid app —
//! if it is already installed on this machine.
//!
//! This app and Farm Tracker Pro are two different processes on purpose (see
//! docs/DBAION2.md): DBAion2 DPS is GPL-3.0 and this repo carries none of the
//! tracker's code, so the only thing this module can do is *find* it, the way
//! a shortcut would. Detection is best-effort: it checks a short list of
//! install locations rather than a registry uninstall key, so it has not been
//! confirmed yet against a real Farm Tracker Pro install. `about-settings.tsx`
//! falls back to opening the tracker's own web page whenever this returns
//! `false`, so a wrong guess here never leaves the button silently doing
//! nothing.

use std::path::PathBuf;

const EXE_NAME: &str = "AION 2 Farm Tracker.exe";
const INSTALL_DIR_NAME: &str = "AION 2 Farm Tracker";

fn candidate_paths() -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    for var in ["ProgramFiles", "ProgramFiles(x86)", "LocalAppData"] {
        if let Ok(base) = std::env::var(var) {
            candidates.push(PathBuf::from(base).join(INSTALL_DIR_NAME).join(EXE_NAME));
        }
    }

    // A side-by-side portable install, next to wherever this app itself runs.
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent().and_then(|d| d.parent()) {
            candidates.push(dir.join(INSTALL_DIR_NAME).join(EXE_NAME));
        }
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join(EXE_NAME));
        }
    }

    candidates
}

fn find_installed() -> Option<PathBuf> {
    candidate_paths().into_iter().find(|path| path.is_file())
}

/// Returns `true` if Farm Tracker Pro was found and launched, `false` if it
/// was not found (the caller should offer the web page instead).
#[tauri::command]
pub fn launch_farm_tracker_pro() -> Result<bool, String> {
    let Some(path) = find_installed() else {
        return Ok(false);
    };

    std::process::Command::new(&path)
        .spawn()
        .map_err(|error| format!("failed to launch {}: {error}", path.display()))?;
    Ok(true)
}
