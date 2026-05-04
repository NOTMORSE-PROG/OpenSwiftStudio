// Tauri IPC command surface (M0.5-9).
//
// Every #[tauri::command] in the project lives here. Setup-wizard commands
// have real bodies; project/run/debug/settings commands are typed stubs that
// return Err("not implemented; lands in <milestone>") so frontend code in
// future milestones has a stable surface to wire to ahead of the Rust impl.
//
// Errors as Result<_, String> — Tauri-friendly, sidesteps serde::Serialize
// constraints that anyhow / thiserror enums would impose at the IPC boundary.

use chrono::Utc;
use serde_json::Value;
use tauri::Emitter;

use crate::setup::checks::{self, CheckResult};
use crate::setup::installs::{self, InstallOutcome};
use crate::setup::state::{self, SetupState};

const INSTALL_PROGRESS_EVENT: &str = "setup-install-progress";

#[tauri::command]
pub fn app_info() -> Value {
    serde_json::json!({
        "name": "OpenSwiftStudio",
        "version": env!("CARGO_PKG_VERSION"),
        "milestone": "M0.5",
        "build": if cfg!(debug_assertions) { "dev" } else { "release" },
    })
}

// ---------- Setup wizard (M0.5-2, M0.5-12) ----------

#[tauri::command]
pub fn setup_get_state() -> Result<Option<SetupState>, String> {
    state::read_state().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn setup_mark_complete(mut state: SetupState) -> Result<(), String> {
    if state.completed_at.is_none() {
        state.completed_at = Some(Utc::now().to_rfc3339());
    }
    state::write_state(&state).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn setup_reset() -> Result<(), String> {
    state::delete_state().map_err(|e| e.to_string())
}

// ---------- Setup wizard checks (M0.5-3/4/5/7) ----------

#[tauri::command]
pub fn setup_check_vs_build_tools() -> CheckResult {
    checks::check_vs_build_tools()
}

#[tauri::command]
pub fn setup_check_wsl2() -> CheckResult {
    checks::check_wsl2()
}

#[tauri::command]
pub fn setup_check_usbipd() -> CheckResult {
    checks::check_usbipd()
}

#[tauri::command]
pub fn setup_check_toolchain() -> CheckResult {
    checks::check_toolchain()
}

// ---------- Setup wizard installs (M0.5-3, M0.5-4) ----------
//
// Async because the underlying subprocess can take 10–60 s. Each emits a
// `setup-install-progress` event per output line so the wizard can render a
// live log preview while the install runs. The payload is `{ id, line }`
// where `id` distinguishes which install the line belongs to.

#[derive(serde::Serialize, Clone)]
struct InstallProgressPayload<'a> {
    id: &'a str,
    line: &'a str,
}

#[tauri::command]
pub async fn setup_install_wsl2(window: tauri::Window) -> Result<InstallOutcome, String> {
    // tauri::Window isn't Send across the await point, so we run the blocking
    // install on a dedicated thread and pump progress events back via the
    // window handle (which is cheaply cloneable).
    let win = window.clone();
    let outcome = tauri::async_runtime::spawn_blocking(move || {
        installs::install_wsl2(|line| {
            let _ = win.emit(
                INSTALL_PROGRESS_EVENT,
                InstallProgressPayload { id: "wsl2", line },
            );
        })
    })
    .await
    .map_err(|e| format!("install task panicked: {e}"))?;
    Ok(outcome)
}

#[tauri::command]
pub async fn setup_install_usbipd(window: tauri::Window) -> Result<InstallOutcome, String> {
    let win = window.clone();
    let outcome = tauri::async_runtime::spawn_blocking(move || {
        installs::install_usbipd(|line| {
            let _ = win.emit(
                INSTALL_PROGRESS_EVENT,
                InstallProgressPayload { id: "usbipd", line },
            );
        })
    })
    .await
    .map_err(|e| format!("install task panicked: {e}"))?;
    Ok(outcome)
}

// ---------- Forward-looking stubs ----------
//
// These define the IPC surface for future milestones. Bodies return an error
// so any UI that wires them prematurely fails loudly. Each names its owning
// milestone so the next contributor knows where to fill it in.

#[tauri::command]
pub fn project_open(_path: String) -> Result<(), String> {
    Err("not implemented; lands in M1".to_string())
}

#[tauri::command]
pub fn project_close() -> Result<(), String> {
    Err("not implemented; lands in M1".to_string())
}

#[tauri::command]
pub fn run_start(_scheme: String) -> Result<u32, String> {
    Err("not implemented; lands in M1/M3".to_string())
}

#[tauri::command]
pub fn run_stop(_pid: u32) -> Result<(), String> {
    Err("not implemented; lands in M3".to_string())
}

#[tauri::command]
pub fn debug_attach(_pid: u32) -> Result<(), String> {
    Err("not implemented; lands in M5".to_string())
}

#[tauri::command]
pub fn settings_get(_key: String) -> Result<Value, String> {
    Err("not implemented; lands in M1-9".to_string())
}

#[tauri::command]
pub fn settings_set(_key: String, _value: Value) -> Result<(), String> {
    Err("not implemented; lands in M1-9".to_string())
}
