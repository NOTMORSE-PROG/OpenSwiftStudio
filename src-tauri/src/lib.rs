// OpenSwiftStudio — Tauri backend entry point.
//
// Per ADR-006, all platform-specific code lives behind clean trait boundaries
// in `platform/`. M0 has no platform-specific work yet; the module exists so
// later milestones (M3 HWND embedding, M9 USB deploy) can drop into place.

mod platform;

#[tauri::command]
fn app_info() -> serde_json::Value {
    serde_json::json!({
        "name": "OpenSwiftStudio",
        "version": env!("CARGO_PKG_VERSION"),
        "milestone": "M0",
        "build": if cfg!(debug_assertions) { "dev" } else { "release" },
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![app_info])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
