//! `--embedded` launch mode (TASK-11 part A, `docs/DBAION2.md`).
//!
//! When the closed-source farm tracker starts this app to host it as one of
//! its own tabs, it passes plain command-line flags -- `--embedded`, and
//! optionally `--theme nebula|nebula-light`, `--lang ru|en`,
//! `--parent-hwnd <n>` -- rather than linking against any shared code. That
//! keeps the two programs separate processes with nothing but a command line
//! (and, on Windows, a reparented window handle) between them; see
//! `docs/DBAION2.md` for the licence reasoning.
//!
//! Without `--embedded` this app behaves exactly as a standalone install:
//! its own title bar, sidebar and tray icon, same as before this task.

use std::sync::OnceLock;

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddedConfig {
    pub embedded: bool,
    /// "nebula" or "nebula-light" -- the same two palettes Settings ->
    /// Appearance already offers as dark/light, just named the way the host
    /// spells them in its own UI.
    pub theme: Option<String>,
    pub lang: Option<String>,
}

struct Args {
    config: EmbeddedConfig,
    parent_hwnd: Option<isize>,
}

fn value_of(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|arg| arg == flag)
        .and_then(|index| args.get(index + 1))
        .cloned()
}

fn parse(args: &[String]) -> Args {
    Args {
        config: EmbeddedConfig {
            embedded: args.iter().any(|arg| arg == "--embedded"),
            theme: value_of(args, "--theme"),
            lang: value_of(args, "--lang"),
        },
        parent_hwnd: value_of(args, "--parent-hwnd").and_then(|value| value.parse().ok()),
    }
}

static ARGS: OnceLock<Args> = OnceLock::new();

fn args() -> &'static Args {
    ARGS.get_or_init(|| parse(&std::env::args().collect::<Vec<_>>()))
}

pub fn is_embedded() -> bool {
    args().config.embedded
}

pub fn parent_hwnd() -> Option<isize> {
    args().parent_hwnd
}

#[tauri::command]
pub fn get_embedded_config() -> EmbeddedConfig {
    args().config.clone()
}

/// Reparents `window` under `hwnd` and starts watching that handle: the
/// moment it stops being a window (the host closed, crashed, or tore down
/// the tab), this process exits. That HWND-liveness check is the half of
/// "when the host closes, exit cleanly" this repository can implement on its
/// own; an explicit close *message* over some richer channel would need its
/// wire format agreed with the farm tracker's own repository, which is a
/// separate codebase this fork must not share code with -- see
/// `docs/DBAION2.md` for that split.
#[cfg(windows)]
pub fn embed_in(app: &tauri::AppHandle, window: &tauri::WebviewWindow, hwnd: isize) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, IsWindow, SetParent, SetWindowLongPtrW, SetWindowPos, GWL_STYLE,
        SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, WS_CAPTION,
        WS_CHILD, WS_POPUP, WS_THICKFRAME,
    };

    let Ok(own) = window.hwnd() else {
        eprintln!("[embedded] could not read this window's own handle");
        return;
    };
    let parent = HWND(hwnd as *mut std::ffi::c_void);

    unsafe {
        if !IsWindow(Some(parent)).as_bool() {
            eprintln!("[embedded] --parent-hwnd {hwnd} is not a window; staying standalone");
            return;
        }

        let style = GetWindowLongPtrW(own, GWL_STYLE) as u32;
        let style = (style & !(WS_POPUP.0 | WS_CAPTION.0 | WS_THICKFRAME.0)) | WS_CHILD.0;
        let _ = SetWindowLongPtrW(own, GWL_STYLE, style as i32 as isize);
        if let Err(error) = SetParent(own, Some(parent)) {
            eprintln!("[embedded] SetParent failed: {error}");
            return;
        }
        let _ = SetWindowPos(
            own,
            None,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );
    }

    // HWND wraps a raw pointer and is not Send; the thread reconstructs it
    // from the plain isize each time instead of capturing it directly.
    let app = app.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_millis(500));
        let parent = HWND(hwnd as *mut std::ffi::c_void);
        if !unsafe { IsWindow(Some(parent)).as_bool() } {
            app.exit(0);
            return;
        }
    });
}
