// OpenSwiftStudio — Tauri backend entry point.
//
// All platform-specific code lives behind clean trait boundaries in
// `platform/`. M0.5 adds the setup wizard backend (`setup/`), the credential
// storage primitive (`auth/`), and the project-wide IPC command surface
// (`ipc.rs`).

mod auth;
mod platform;
mod project;
mod setup;
mod ipc;

/// Payload forwarded to the running instance when a second launch is blocked
/// (M1-11). Carries the second process's argv + cwd; M1-12 consumes it to open
/// a project passed on the command line.
#[derive(Clone, serde::Serialize)]
struct SingleInstancePayload {
    args: Vec<String>,
    cwd: String,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();

    // Single-instance MUST be registered before every other plugin (its
    // callback fires in the already-running instance when a second launch
    // happens; the second process then exits). Desktop-only. It forwards the
    // second launch's argv/cwd via a `single-instance` event (M1-12's
    // transport) and focuses the existing window.
    #[cfg(desktop)]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, argv, cwd| {
        use tauri::{Emitter, Manager};
        let _ = app.emit("single-instance", SingleInstancePayload { args: argv, cwd });
        if let Some(window) = app.webview_windows().values().next() {
            let _ = window.unminimize();
            let _ = window.set_focus();
        }
    }));

    builder
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_http::init())
        .manage::<ipc::CurrentProject>(std::sync::Mutex::new(None))
        .manage::<project::RunState>(project::RunState::default())
        .invoke_handler(tauri::generate_handler![
            ipc::app_info,
            ipc::app_get_toolchain,
            ipc::setup_get_state,
            ipc::setup_mark_complete,
            ipc::setup_reset,
            ipc::setup_check_vs_build_tools,
            ipc::setup_check_wsl2,
            ipc::setup_check_usbipd,
            ipc::setup_check_toolchain,
            ipc::setup_check_xtool,
            ipc::setup_install_wsl2,
            ipc::setup_install_usbipd,
            ipc::setup_install_toolchain,
            ipc::setup_install_xtool,
            ipc::setup_run_xtool,
            ipc::setup_store_apple_id,
            ipc::setup_get_stored_apple_id,
            ipc::project_open,
            ipc::project_close,
            ipc::project_get_meta,
            ipc::project_get_files,
            ipc::run_start,
            ipc::run_stop,
            ipc::session_load,
            ipc::session_save,
            ipc::session_clear,
            ipc::session_mru_push,
            ipc::paths_exist,
            ipc::debug_attach,
            ipc::settings_get,
            ipc::settings_set,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
