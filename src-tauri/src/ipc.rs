// Tauri IPC command surface (M0.5-9).
//
// Every #[tauri::command] in the project lives here. Setup-wizard commands
// have real bodies; project/run/debug/settings commands are typed stubs that
// return Err("not implemented; lands in <milestone>") so frontend code in
// future milestones has a stable surface to wire to ahead of the Rust impl.
//
// Errors as Result<_, String> — Tauri-friendly, sidesteps serde::Serialize
// constraints that anyhow / thiserror enums would impose at the IPC boundary.

use std::path::PathBuf;
use std::sync::Mutex;

use chrono::Utc;
use serde_json::Value;
use tauri::Emitter;

use crate::auth::credential_store::{self, APPLE_ID_KEY};
use crate::project::run::{self, BuildConfig};
use crate::project::session::{self, SessionState};
use crate::project::{self, FileTreeNode, PackageDescription, ProjectState, RunState};
use crate::setup::checks::{self, CheckResult};
use crate::setup::installs::{self, InstallOutcome, ProgressEvent, ProgressPhase};
use crate::setup::selftest;
use crate::setup::state::{self, SetupState};
use crate::setup::xtool;

/// Tauri-managed slot holding the currently-open project (or `None`). Disk
/// persistence (session.json) lands in M1 chunk 3; chunk 1 only needs the
/// running process to remember which project is active.
pub type CurrentProject = Mutex<Option<ProjectState>>;

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

#[tauri::command]
pub fn setup_check_xtool() -> CheckResult {
    checks::check_xtool()
}

// ---------- Setup wizard installs (M0.5-3, M0.5-4, M0.5-5) ----------
//
// Async because the underlying subprocess + download can take 10 s – several
// minutes (Swift toolchain is ~900 MB). Each emits a `setup-install-progress`
// event per ProgressEvent so the wizard can render both a live log preview
// and a download progress bar.
//
// Wire format is a tagged union that mirrors the Rust ProgressEvent: lines
// arrive as `{ id, kind: "line", line }`; download/verify/install progress
// arrives as `{ id, kind: "progress", phase, received, total }`.

#[derive(serde::Serialize, Clone)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
enum InstallProgressBody {
    Line {
        line: String,
    },
    Progress {
        phase: ProgressPhase,
        received: u64,
        total: u64,
    },
}

#[derive(serde::Serialize, Clone)]
struct InstallProgressPayload {
    id: &'static str,
    #[serde(flatten)]
    body: InstallProgressBody,
}

fn payload_for(id: &'static str, event: ProgressEvent) -> InstallProgressPayload {
    let body = match event {
        ProgressEvent::Line { line } => InstallProgressBody::Line { line },
        ProgressEvent::Progress {
            phase,
            received,
            total,
        } => InstallProgressBody::Progress { phase, received, total },
    };
    InstallProgressPayload { id, body }
}

#[tauri::command]
pub async fn setup_install_wsl2(window: tauri::Window) -> Result<InstallOutcome, String> {
    // tauri::Window isn't Send across the await point, so we run the blocking
    // install on a dedicated thread and pump progress events back via the
    // window handle (which is cheaply cloneable).
    let win = window.clone();
    let outcome = tauri::async_runtime::spawn_blocking(move || {
        installs::install_wsl2(|event| {
            let _ = win.emit(INSTALL_PROGRESS_EVENT, payload_for("wsl2", event));
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
        installs::install_usbipd(|event| {
            let _ = win.emit(INSTALL_PROGRESS_EVENT, payload_for("usbipd", event));
        })
    })
    .await
    .map_err(|e| format!("install task panicked: {e}"))?;
    Ok(outcome)
}

#[tauri::command]
pub async fn setup_install_toolchain(window: tauri::Window) -> Result<InstallOutcome, String> {
    let win = window.clone();
    let outcome = tauri::async_runtime::spawn_blocking(move || {
        installs::install_toolchain(|event| {
            let _ = win.emit(INSTALL_PROGRESS_EVENT, payload_for("toolchain", event));
        })
    })
    .await
    .map_err(|e| format!("install task panicked: {e}"))?;
    Ok(outcome)
}

#[tauri::command]
pub async fn setup_install_xtool(window: tauri::Window) -> Result<InstallOutcome, String> {
    let win = window.clone();
    let outcome = tauri::async_runtime::spawn_blocking(move || {
        installs::install_xtool(|event| {
            let _ = win.emit(INSTALL_PROGRESS_EVENT, payload_for("xtool", event));
        })
    })
    .await
    .map_err(|e| format!("install task panicked: {e}"))?;
    Ok(outcome)
}

// ---------- xtool auth login + sdk install (M0.5-6) ----------
//
// Two-stage subprocess flow that drives `xtool auth login` then
// `xtool sdk install` against the user's WSL2 distro. Streams progress events
// under id "xtool-setup" so the wizard can render a single combined log +
// progress bar. Email is the Apple ID; password is an app-specific password
// generated at appleid.apple.com → Sign-In and Security → App-Specific
// Passwords (cleanest 2FA bypass for tooling).

#[tauri::command]
pub async fn setup_run_xtool(
    window: tauri::Window,
    email: String,
    password: String,
    xip_path: String,
) -> Result<InstallOutcome, String> {
    let win = window.clone();
    let outcome = tauri::async_runtime::spawn_blocking(move || {
        xtool::run_xtool_setup(&email, &password, &xip_path, |event| {
            let _ = win.emit(INSTALL_PROGRESS_EVENT, payload_for("xtool-setup", event));
        })
    })
    .await
    .map_err(|e| format!("xtool setup task panicked: {e}"))?;
    Ok(outcome)
}

#[tauri::command]
pub fn setup_store_apple_id(email: String) -> Result<(), String> {
    credential_store::store(APPLE_ID_KEY, &email).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn setup_get_stored_apple_id() -> Result<Option<String>, String> {
    credential_store::retrieve(APPLE_ID_KEY).map_err(|e| e.to_string())
}

// ---------- Project (M1 chunk 1) ----------

/// Open a SwiftPM project folder. Parses Package.swift via the two-tier
/// `swift package describe` → regex-fallback chain in `project::parser` and
/// stores the resulting state in the Tauri-managed slot. Returns the parsed
/// description so the frontend can render targets/products without a follow-up
/// round-trip.
#[tauri::command]
pub fn project_open(
    path: String,
    app: tauri::AppHandle,
    current: tauri::State<'_, CurrentProject>,
    watch: tauri::State<'_, project::ManifestWatch>,
) -> Result<PackageDescription, String> {
    let root = PathBuf::from(&path);
    let package = project::parse_package(&root).map_err(|e| e.message)?;
    let state = ProjectState {
        package: package.clone(),
        opened_at: Utc::now(),
    };
    *current.lock().map_err(|e| format!("project state lock poisoned: {e}"))? = Some(state);

    // Watch Package.swift for external edits (M1-15). Replacing the slot drops
    // any previous project's watcher. A watcher failure is non-fatal: the
    // project still opens, only live re-parse is lost.
    let manifest_watcher = project::watch_manifest(&root, {
        let app = app.clone();
        let root = root.clone();
        move || refresh_manifest(&app, &root)
    });
    let mut watch_slot = watch
        .lock()
        .map_err(|e| format!("manifest watch lock poisoned: {e}"))?;
    *watch_slot = match manifest_watcher {
        Ok(w) => Some(w),
        Err(err) => {
            eprintln!("manifest watcher unavailable for {}: {err}", root.display());
            None
        }
    };

    Ok(package)
}

#[tauri::command]
pub fn project_close(
    current: tauri::State<'_, CurrentProject>,
    watch: tauri::State<'_, project::ManifestWatch>,
) -> Result<(), String> {
    *current.lock().map_err(|e| format!("project state lock poisoned: {e}"))? = None;
    // Dropping the watcher stops its thread.
    *watch
        .lock()
        .map_err(|e| format!("manifest watch lock poisoned: {e}"))? = None;
    Ok(())
}

/// Wire payload for `project-manifest-changed` (M1-15).
#[derive(Clone, serde::Serialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ManifestChange {
    /// Re-parse succeeded; `meta` is the fresh project model.
    Updated { meta: PackageDescription },
    /// Package.swift is gone/unreadable; the previous model is kept so the
    /// user can restore the file without losing the open project.
    Missing,
}

const MANIFEST_CHANGED_EVENT: &str = "project-manifest-changed";

/// Watcher callback: re-parse the manifest and publish the result. Runs on the
/// watcher's thread. The root guard skips stale events that were in flight
/// while the user switched projects (the new watcher already replaced ours,
/// but a debounced batch may still land after the swap).
fn refresh_manifest(app: &tauri::AppHandle, root: &std::path::Path) {
    use tauri::Manager;

    let current = app.state::<CurrentProject>();
    let event = match project::parse_package(root) {
        Ok(package) => {
            let mut guard = match current.lock() {
                Ok(g) => g,
                Err(_) => return,
            };
            match guard.as_mut() {
                Some(state) if PathBuf::from(&state.package.root_path) == root => {
                    state.package = package.clone();
                }
                _ => return, // project switched or closed since the event fired
            }
            ManifestChange::Updated { meta: package }
        }
        Err(_) => {
            let still_current = current
                .lock()
                .ok()
                .and_then(|g| {
                    g.as_ref()
                        .map(|s| PathBuf::from(&s.package.root_path) == root)
                })
                .unwrap_or(false);
            if !still_current {
                return;
            }
            ManifestChange::Missing
        }
    };
    let _ = app.emit(MANIFEST_CHANGED_EVENT, event);
}

/// Returns the parsed manifest of the currently-open project, or `None` when
/// no project is open. Used by the frontend on app start (after restoring
/// session.json in chunk 3) and after focus changes.
#[tauri::command]
pub fn project_get_meta(
    current: tauri::State<'_, CurrentProject>,
) -> Result<Option<PackageDescription>, String> {
    let guard = current
        .lock()
        .map_err(|e| format!("project state lock poisoned: {e}"))?;
    Ok(guard.as_ref().map(|s| s.package.clone()))
}

/// Read the project root's direct children (one level), filtered against the
/// blocklist. Recursive expansion alongside Monaco lands in M2.
#[tauri::command]
pub fn project_get_files(path: String) -> Result<Vec<FileTreeNode>, String> {
    let root = PathBuf::from(&path);
    project::read_project_files(&root).map_err(|e| e.message)
}

/// Returns the live Swift toolchain detection. Status bar consumes this on
/// mount to show the active version. We don't cache here — a `swift --version`
/// subprocess returns in well under a second and the result is the source of
/// truth (cached `setup.json` records can be stale if the user uninstalled
/// Swift between sessions).
#[tauri::command]
pub fn app_get_toolchain() -> CheckResult {
    checks::check_toolchain()
}

/// Compile self-test (FU-8): build a throwaway minimal package and report
/// whether the toolchain actually compiles on this machine. Async because
/// `swift build` takes ~1-30 s. The setup wizard runs this after install so a
/// toolchain that crashes on this hardware (version bug or unsupported-CPU
/// instruction) surfaces a clear message instead of a cryptic Run failure.
#[tauri::command]
pub async fn app_toolchain_selftest() -> Result<selftest::SelfTestResult, String> {
    tauri::async_runtime::spawn_blocking(selftest::run_toolchain_selftest)
        .await
        .map_err(|e| format!("self-test task panicked: {e}"))
}

// ---------- Run (M1-5 / M1-6 / M1-13) ----------

/// Build the open project under `config` and execute its binary, streaming
/// output as `project-run-progress` events. Returns immediately after spawning
/// the pipeline on a blocking thread — the frontend drives its UI off the
/// events, not this promise. Fails synchronously (before spawning) when no
/// project is open, the project has no executable product, or a run is already
/// active.
#[tauri::command]
pub fn run_start(
    window: tauri::Window,
    config: BuildConfig,
    current: tauri::State<'_, CurrentProject>,
    run_state: tauri::State<'_, RunState>,
) -> Result<(), String> {
    let (root, product) = {
        let guard = current
            .lock()
            .map_err(|e| format!("project state lock poisoned: {e}"))?;
        let project = guard.as_ref().ok_or("no project is open")?;
        let product = run::executable_name(&project.package)
            .ok_or("this project has no executable product to run")?;
        (project.package.root_path.clone(), product)
    };

    {
        let mut g = run_state
            .lock()
            .map_err(|e| format!("run state lock poisoned: {e}"))?;
        if g.active {
            return Err("a run is already in progress".to_string());
        }
        g.active = true;
        g.cancelled = false;
        g.child_pid = None;
    }

    let win = window.clone();
    let state = run_state.inner().clone();
    let root = PathBuf::from(root);
    tauri::async_runtime::spawn_blocking(move || {
        run::run_pipeline(&win, config, root, product, &state);
    });
    Ok(())
}

/// Stop the active run: kill the subprocess tree and mark it cancelled. No-op
/// when nothing is running.
#[tauri::command]
pub fn run_stop(run_state: tauri::State<'_, RunState>) -> Result<(), String> {
    run::stop(run_state.inner());
    Ok(())
}

// ---------- Session persistence (M1-7) ----------
//
// The frontend saves a snapshot eagerly on each session-relevant change
// (project open/close, build-config toggle) and loads it once on startup to
// restore the last-opened project + build config. Save is atomic (tempfile +
// rename); a corrupt or future-schema file loads as None so the IDE launches
// clean. The Package.swift file-watcher half of M1-7 is tracked as M1-15.

#[tauri::command]
pub fn session_load() -> Result<Option<SessionState>, String> {
    session::read_session().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn session_save(state: SessionState) -> Result<(), String> {
    session::write_session(&state).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn session_clear() -> Result<(), String> {
    session::clear_session().map_err(|e| e.to_string())
}

/// Pure MRU update (M1-8): promote `path` to the front of `recent`, dedupe
/// case-insensitively, cap at the recent-projects cap. No I/O — the frontend
/// persists the returned list via the normal session autosave (single writer).
#[tauri::command]
pub fn session_mru_push(recent: Vec<String>, path: String) -> Vec<String> {
    session::mru_push(&recent, &path, session::RECENT_PROJECTS_CAP)
}

/// Existence check for each path (M1-8): lets the welcome/palette mark recent
/// entries whose folder was deleted so the user can remove instead of hitting a
/// raw open error.
#[tauri::command]
pub fn paths_exist(paths: Vec<String>) -> Vec<bool> {
    paths.iter().map(|p| PathBuf::from(p).is_dir()).collect()
}

// ---------- Forward-looking stubs ----------
//
// These define the IPC surface for future milestones. Bodies return an error
// so any UI that wires them prematurely fails loudly. Each names its owning
// milestone so the next contributor knows where to fill it in.

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
