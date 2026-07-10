// Run pipeline: `swift build` then execute the built binary (M1-5), streaming
// both stdout and stderr into the Console pane (M1-6), under a Debug/Release
// build configuration (M1-13).
//
// Design (see tickets M1-5 / M1-6 research notes, 2026-07-06):
//   * Build progress goes to swift's stdout, diagnostics to stderr, and both
//     can be written concurrently, so each pipe is drained on its own thread —
//     reading one to completion before the other risks a pipe-buffer deadlock.
//   * The built executable's directory is resolved with
//     `swift build --show-bin-path [-c release]`, never hardcoded: on Windows
//     the convenience `.build\debug` symlink often can't be created (needs
//     Developer Mode), so the real path is the triple-nested
//     `.build\<triple>\debug`.
//   * Output is UTF-8 with CRLF line endings; decode is lossy so invalid bytes
//     never panic, and lines are batched before emit to bound IPC event volume
//     under a runaway process.
//   * Stop kills the whole process tree via `taskkill /PID <pid> /T /F` — the
//     build spawns swiftc/clang/lld grandchildren that a single-process kill
//     would orphan.

use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::{Emitter, Window};

use super::parser::PackageDescription;

/// Tauri event name every run emits under. Payload is a `RunEvent`.
pub const PROJECT_RUN_EVENT: &str = "project-run-progress";

/// Flush a batch of console lines once it reaches this many lines...
const BATCH_MAX_LINES: usize = 128;
/// ...or once this long has elapsed since the last flush, whichever comes
/// first. Slow/normal output flushes near-per-line (the elapsed check trips on
/// the next read); a flood coalesces into >=128-line batches so a 1M-line run
/// emits ~8k events instead of 1M.
const BATCH_FLUSH_INTERVAL: Duration = Duration::from_millis(30);

// ---------- Build configuration (M1-13) ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BuildConfig {
    Debug,
    Release,
}

impl BuildConfig {
    /// Extra args appended to `swift build` for this config. Debug is SwiftPM's
    /// default (no flag); Release passes `-c release`.
    fn config_args(self) -> &'static [&'static str] {
        match self {
            BuildConfig::Debug => &[],
            BuildConfig::Release => &["-c", "release"],
        }
    }
}

// ---------- Wire events (M1-5 / M1-6) ----------

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RunPhase {
    Build,
    Run,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum OutputStream {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RunStatus {
    Building,
    Running,
}

/// How a run ended. `exited` carries the program's exit code; `buildFailed`
/// carries swift build's; `stopped` means the user pressed Stop; `spawnError`
/// means a subprocess couldn't start (swift missing, binary vanished);
/// `toolchainCrashed` means `swift build` itself crashed (a Windows exception
/// exit code) rather than failing to compile the user's code — the Swift
/// toolchain can't compile on this machine (FU-8).
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RunOutcome {
    Exited,
    BuildFailed,
    ToolchainCrashed,
    Stopped,
    SpawnError,
}

/// Classify a non-zero `swift build` exit into the right outcome: a Windows
/// crash exit code (top nibble 0xC) means the toolchain itself crashed, not a
/// compile error.
fn build_failure_outcome(build_exit: i32) -> RunOutcome {
    if crate::setup::selftest::is_crash_exit(build_exit) {
        RunOutcome::ToolchainCrashed
    } else {
        RunOutcome::BuildFailed
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum RunEvent {
    /// Phase transition: build started / run started. Frontend uses this to
    /// flip the Run button to Stop and auto-reveal the Console.
    Status { status: RunStatus },
    /// A batch of console lines from one stream of one phase. Ordering is
    /// guaranteed per (phase, stream); global interleaving is not.
    Output {
        phase: RunPhase,
        stream: OutputStream,
        lines: Vec<String>,
    },
    /// Terminal event — the run is over, frontend returns to idle. Exactly one
    /// is emitted per run, after all Output events.
    Exit {
        phase: RunPhase,
        code: i32,
        outcome: RunOutcome,
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
}

// ---------- Shared run state (for Stop) ----------

#[derive(Debug, Default)]
pub struct RunInner {
    /// True from the moment a run is accepted until its Exit event is emitted.
    /// Guards against a second concurrent run.
    pub active: bool,
    /// PID of the currently-spawned subprocess (swift build, then the app),
    /// so Stop can kill its tree. `None` between phases.
    pub child_pid: Option<u32>,
    /// Set by Stop; the pipeline reads it to skip the run phase after a killed
    /// build and to label the outcome as `stopped`.
    pub cancelled: bool,
}

/// Managed by Tauri via `.manage()`. An `Arc` so the blocking pipeline thread
/// and the `run_stop` command can share it (Tauri `State` can't cross into
/// `spawn_blocking`, so the command clones the inner `Arc`).
pub type RunState = Arc<Mutex<RunInner>>;

// ---------- Executable-product gate (M1-5) ----------

/// The name of the executable to run for this package, or `None` when the
/// project has no executable product/target (Run is disabled in that case).
/// Prefers an executable *product*; falls back to an executable *target* (a
/// bare `.executableTarget` with no explicit product still builds a binary
/// named after the target).
pub fn executable_name(pkg: &PackageDescription) -> Option<String> {
    if let Some(p) = pkg
        .products
        .iter()
        .find(|p| p.kind.as_deref() == Some("executable"))
    {
        return Some(p.name.clone());
    }
    pkg.targets
        .iter()
        .find(|t| t.kind.as_deref() == Some("executable"))
        .map(|t| t.name.clone())
}

// ---------- Pipeline ----------

/// Run the full build-then-execute pipeline for `product` in `root` under
/// `config`, emitting `RunEvent`s on `window`. Runs on a blocking thread
/// (`spawn_blocking`); `state.active` must already be set true by the caller.
pub fn run_pipeline(
    window: &Window,
    config: BuildConfig,
    root: PathBuf,
    product: String,
    state: &RunState,
) {
    // --- Build phase ---
    let _ = window.emit(
        PROJECT_RUN_EVENT,
        RunEvent::Status { status: RunStatus::Building },
    );

    let mut build_cmd = Command::new("swift");
    build_cmd.arg("build").args(config.config_args()).current_dir(&root);

    let build_exit = match stream_command(window, RunPhase::Build, build_cmd, state) {
        Ok(code) => code,
        Err(e) => {
            finish(window, state, RunPhase::Build, -1, RunOutcome::SpawnError, Some(swift_spawn_hint(&e)));
            return;
        }
    };

    if is_cancelled(state) {
        finish(window, state, RunPhase::Build, build_exit, RunOutcome::Stopped, None);
        return;
    }
    if build_exit != 0 {
        finish(window, state, RunPhase::Build, build_exit, build_failure_outcome(build_exit), None);
        return;
    }

    // --- Resolve the built binary (after a successful build, so it exists) ---
    let bin_dir = match resolve_bin_path(config, &root) {
        Ok(dir) => dir,
        Err(e) => {
            finish(window, state, RunPhase::Run, -1, RunOutcome::SpawnError, Some(e));
            return;
        }
    };
    let exe = bin_dir.join(executable_file_name(&product));

    // --- Run phase ---
    let _ = window.emit(
        PROJECT_RUN_EVENT,
        RunEvent::Status { status: RunStatus::Running },
    );

    let mut run_cmd = Command::new(&exe);
    run_cmd.current_dir(&root);

    match stream_command(window, RunPhase::Run, run_cmd, state) {
        Ok(code) => {
            let outcome = if is_cancelled(state) {
                RunOutcome::Stopped
            } else {
                RunOutcome::Exited
            };
            finish(window, state, RunPhase::Run, code, outcome, None);
        }
        Err(e) => {
            let msg = format!("could not launch {}: {e}", exe.display());
            finish(window, state, RunPhase::Run, -1, RunOutcome::SpawnError, Some(msg));
        }
    }
}

/// Spawn `cmd`, register its PID for Stop, drain stdout+stderr concurrently
/// (batched into `RunEvent::Output`), wait, and return the exit code.
fn stream_command(
    window: &Window,
    phase: RunPhase,
    mut cmd: Command,
    state: &RunState,
) -> std::io::Result<i32> {
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    apply_no_window(&mut cmd);

    let mut child = cmd.spawn()?;
    if let Ok(mut g) = state.lock() {
        g.child_pid = Some(child.id());
    }

    let out_handle = spawn_reader(window.clone(), phase, OutputStream::Stdout, child.stdout.take());
    let err_handle = spawn_reader(window.clone(), phase, OutputStream::Stderr, child.stderr.take());

    // Join readers first: they run until their pipe hits EOF (which happens
    // when the child exits or is killed), so all Output events are emitted
    // before we wait + emit Exit. No output can leak into a later run.
    let _ = out_handle.join();
    let _ = err_handle.join();

    let status = child.wait()?;
    if let Ok(mut g) = state.lock() {
        g.child_pid = None;
    }
    Ok(status.code().unwrap_or(-1))
}

fn spawn_reader<R: Read + Send + 'static>(
    window: Window,
    phase: RunPhase,
    stream: OutputStream,
    reader: Option<R>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let Some(reader) = reader else { return };
        read_stream(reader, |lines| {
            let _ = window.emit(
                PROJECT_RUN_EVENT,
                RunEvent::Output { phase, stream, lines },
            );
        });
    })
}

/// Drain `reader` line-by-line, normalizing each line (strip CRLF, lossy UTF-8
/// decode, strip ANSI), and hand batches to `on_batch`. A batch flushes at
/// `BATCH_MAX_LINES` or `BATCH_FLUSH_INTERVAL`, with a final flush at EOF
/// (which also emits a partial trailing line that had no newline).
fn read_stream<R: Read>(reader: R, mut on_batch: impl FnMut(Vec<String>)) {
    let mut buf = BufReader::new(reader);
    let mut batch: Vec<String> = Vec::new();
    let mut last_flush = Instant::now();
    let mut line_bytes: Vec<u8> = Vec::new();

    loop {
        line_bytes.clear();
        match buf.read_until(b'\n', &mut line_bytes) {
            Ok(0) => break, // EOF
            Ok(_) => {}
            Err(_) => break,
        }
        batch.push(normalize_line(&line_bytes));

        if batch.len() >= BATCH_MAX_LINES || last_flush.elapsed() >= BATCH_FLUSH_INTERVAL {
            on_batch(std::mem::take(&mut batch));
            last_flush = Instant::now();
        }
    }

    if !batch.is_empty() {
        on_batch(batch);
    }
}

/// Strip the trailing newline (LF or CRLF), lossy-decode as UTF-8, strip ANSI.
fn normalize_line(bytes: &[u8]) -> String {
    let mut end = bytes.len();
    while end > 0 && (bytes[end - 1] == b'\n' || bytes[end - 1] == b'\r') {
        end -= 1;
    }
    strip_ansi(&String::from_utf8_lossy(&bytes[..end]))
}

/// Remove ANSI CSI escape sequences (`ESC [ ... final`). Piped swift output
/// carries none (it colorizes only on a TTY), but a future toolchain could
/// force color; this keeps raw escapes out of the console defensively.
fn strip_ansi(s: &str) -> String {
    if !s.contains('\x1b') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' && chars.peek() == Some(&'[') {
            chars.next(); // consume '['
            // Consume until a final byte in the range @A-Z[\]^_`a-z{|}~ (0x40..=0x7E).
            for f in chars.by_ref() {
                if ('\u{40}'..='\u{7e}').contains(&f) {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn resolve_bin_path(config: BuildConfig, root: &Path) -> Result<PathBuf, String> {
    let mut cmd = Command::new("swift");
    cmd.arg("build")
        .arg("--show-bin-path")
        .args(config.config_args())
        .current_dir(root);
    apply_no_window(&mut cmd);

    let output = cmd
        .output()
        .map_err(|e| format!("could not resolve build path: {}", swift_spawn_hint(&e)))?;
    if !output.status.success() {
        return Err(format!(
            "swift build --show-bin-path exited {}: {}",
            output.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() {
        return Err("swift build --show-bin-path returned no path".to_string());
    }
    Ok(PathBuf::from(path))
}

#[cfg(target_os = "windows")]
fn executable_file_name(product: &str) -> String {
    format!("{product}.exe")
}

#[cfg(not(target_os = "windows"))]
fn executable_file_name(product: &str) -> String {
    product.to_string()
}

/// Emit the terminal Exit event and reset run state to idle.
fn finish(
    window: &Window,
    state: &RunState,
    phase: RunPhase,
    code: i32,
    outcome: RunOutcome,
    message: Option<String>,
) {
    if let Ok(mut g) = state.lock() {
        g.active = false;
        g.child_pid = None;
    }
    let _ = window.emit(
        PROJECT_RUN_EVENT,
        RunEvent::Exit { phase, code, outcome, message },
    );
}

fn is_cancelled(state: &RunState) -> bool {
    state.lock().map(|g| g.cancelled).unwrap_or(false)
}

/// Turn a spawn error into a user-facing hint. The common case is swift not on
/// PATH (`NotFound`), which points at the setup wizard.
fn swift_spawn_hint(e: &std::io::Error) -> String {
    if e.kind() == std::io::ErrorKind::NotFound {
        "swift was not found on PATH. Install the Swift toolchain via the setup wizard.".to_string()
    } else {
        e.to_string()
    }
}

// ---------- Stop (M1-5) ----------

/// Kill the currently-running subprocess tree and mark the run cancelled.
/// Idempotent — a no-op if nothing is running.
pub fn stop(state: &RunState) {
    let pid = {
        let Ok(mut g) = state.lock() else { return };
        g.cancelled = true;
        g.child_pid
    };
    if let Some(pid) = pid {
        kill_tree(pid);
    }
}

#[cfg(target_os = "windows")]
fn kill_tree(pid: u32) {
    let mut cmd = Command::new("taskkill");
    cmd.arg("/PID").arg(pid.to_string()).arg("/T").arg("/F");
    apply_no_window(&mut cmd);
    let _ = cmd.output();
}

#[cfg(not(target_os = "windows"))]
fn kill_tree(pid: u32) {
    // Best-effort single-process kill on non-Windows (v0.1 ships Windows only;
    // platform/{linux,macos} are stubs). A full process-group kill lands with
    // those ports.
    let _ = Command::new("kill").arg("-KILL").arg(pid.to_string()).output();
}

#[cfg(target_os = "windows")]
fn apply_no_window(cmd: &mut Command) {
    use std::os::windows::process::CommandExt;
    cmd.creation_flags(crate::platform::windows::CREATE_NO_WINDOW);
}

#[cfg(not(target_os = "windows"))]
fn apply_no_window(_cmd: &mut Command) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::parser::{PackageDescription, PackageProduct, PackageTarget};
    use std::io::Cursor;

    fn pkg(products: Vec<PackageProduct>, targets: Vec<PackageTarget>) -> PackageDescription {
        PackageDescription {
            name: "P".to_string(),
            manifest_path: "/p/Package.swift".to_string(),
            root_path: "/p".to_string(),
            products,
            targets,
            degraded: false,
            degraded_reason: None,
        }
    }

    fn product(name: &str, kind: &str) -> PackageProduct {
        PackageProduct { name: name.to_string(), kind: Some(kind.to_string()), targets: vec![] }
    }
    fn target(name: &str, kind: &str) -> PackageTarget {
        PackageTarget { name: name.to_string(), kind: Some(kind.to_string()), path: None }
    }

    #[test]
    fn build_failure_outcome_distinguishes_toolchain_crash_from_compile_error() {
        // Windows illegal-instruction exit (the 6.3.x toolchain crash) -> toolchain crash.
        assert!(matches!(
            build_failure_outcome(-1073741795),
            RunOutcome::ToolchainCrashed
        ));
        // Normal `swift build` compile error -> build failed.
        assert!(matches!(build_failure_outcome(1), RunOutcome::BuildFailed));
    }

    #[test]
    fn build_config_release_passes_c_release_and_debug_passes_nothing() {
        assert_eq!(BuildConfig::Debug.config_args(), &[] as &[&str]);
        assert_eq!(BuildConfig::Release.config_args(), &["-c", "release"]);
    }

    #[test]
    fn build_config_deserializes_from_lowercase() {
        assert_eq!(
            serde_json::from_str::<BuildConfig>("\"debug\"").unwrap(),
            BuildConfig::Debug
        );
        assert_eq!(
            serde_json::from_str::<BuildConfig>("\"release\"").unwrap(),
            BuildConfig::Release
        );
    }

    #[test]
    fn executable_name_prefers_executable_product() {
        let p = pkg(
            vec![product("Lib", "library"), product("Tool", "executable")],
            vec![],
        );
        assert_eq!(executable_name(&p), Some("Tool".to_string()));
    }

    #[test]
    fn executable_name_falls_back_to_executable_target() {
        let p = pkg(vec![], vec![target("App", "executable"), target("Core", "library")]);
        assert_eq!(executable_name(&p), Some("App".to_string()));
    }

    #[test]
    fn executable_name_none_when_library_only() {
        let p = pkg(vec![product("Lib", "library")], vec![target("Lib", "library")]);
        assert_eq!(executable_name(&p), None);
    }

    #[test]
    fn normalize_line_strips_crlf_and_lf() {
        assert_eq!(normalize_line(b"hello\r\n"), "hello");
        assert_eq!(normalize_line(b"hello\n"), "hello");
        assert_eq!(normalize_line(b"hello"), "hello");
        assert_eq!(normalize_line(b"\r\n"), "");
    }

    #[test]
    fn normalize_line_lossy_decodes_invalid_utf8_without_panic() {
        let line = normalize_line(b"bad\xFFbyte\n");
        assert!(line.starts_with("bad"));
        assert!(line.contains('\u{FFFD}'));
    }

    #[test]
    fn strip_ansi_removes_color_sequences() {
        assert_eq!(strip_ansi("\x1b[31merror\x1b[0m: boom"), "error: boom");
        assert_eq!(strip_ansi("plain text"), "plain text");
        assert_eq!(strip_ansi("\x1b[1;32mok\x1b[0m"), "ok");
    }

    #[test]
    fn read_stream_splits_crlf_lf_and_flushes_partial_tail() {
        // Two full lines (CRLF, LF), an invalid-UTF8 line, then a partial line
        // with no trailing newline — the tail must still be emitted.
        let data = b"first\r\nsecond\ninva\xFFlid\npartial".to_vec();
        let mut got: Vec<String> = Vec::new();
        read_stream(Cursor::new(data), |lines| got.extend(lines));
        assert_eq!(got.len(), 4);
        assert_eq!(got[0], "first");
        assert_eq!(got[1], "second");
        assert!(got[2].contains('\u{FFFD}'));
        assert_eq!(got[3], "partial");
    }

    #[test]
    fn read_stream_coalesces_a_flood_into_far_fewer_batches() {
        // 50k lines with no read delay: every line must be delivered, but the
        // batching must coalesce them into << 50k emit calls so a runaway
        // process can't flood the IPC channel (M1-6 line-cap requirement).
        let mut data = Vec::new();
        for i in 0..50_000u32 {
            data.extend_from_slice(format!("line {i}\n").as_bytes());
        }
        let mut total_lines = 0usize;
        let mut batches = 0usize;
        read_stream(Cursor::new(data), |lines| {
            batches += 1;
            total_lines += lines.len();
        });
        assert_eq!(total_lines, 50_000, "every line must be delivered");
        assert!(
            batches < 1000,
            "expected heavy coalescing, got {batches} batches"
        );
    }

    #[test]
    fn read_stream_empty_input_emits_nothing() {
        let mut got: Vec<String> = Vec::new();
        read_stream(Cursor::new(Vec::new()), |lines| got.extend(lines));
        assert!(got.is_empty());
    }

    #[test]
    fn run_event_output_serializes_camelcase_tagged() {
        let ev = RunEvent::Output {
            phase: RunPhase::Build,
            stream: OutputStream::Stderr,
            lines: vec!["x".to_string()],
        };
        let json = serde_json::to_string(&ev).unwrap();
        assert!(json.contains("\"kind\":\"output\""));
        assert!(json.contains("\"phase\":\"build\""));
        assert!(json.contains("\"stream\":\"stderr\""));
    }

    #[test]
    fn run_event_exit_omits_none_message() {
        let ev = RunEvent::Exit {
            phase: RunPhase::Run,
            code: 0,
            outcome: RunOutcome::Exited,
            message: None,
        };
        let json = serde_json::to_string(&ev).unwrap();
        assert!(json.contains("\"kind\":\"exit\""));
        assert!(json.contains("\"outcome\":\"exited\""));
        assert!(!json.contains("message"), "None message should be skipped");
    }

    /// End-to-end: build + run examples/hello-world through the real pipeline
    /// and assert "Hello, World!" reaches a captured Output event. Requires a
    /// working Swift toolchain (see M1-5 dev-host caveat / FU-5); gated
    /// `--ignored` like the parser's `parse_hello_world_with_swift_command`.
    ///
    /// This drives the phase logic directly (build command -> stream -> resolve
    /// bin path -> run command -> stream) without a Tauri Window, collecting
    /// emitted lines into a Vec so the assertion is on real captured output.
    #[test]
    #[ignore]
    fn run_hello_world_end_to_end() {
        let repo_root = std::env::current_dir()
            .expect("cwd")
            .parent()
            .expect("parent of src-tauri")
            .to_path_buf();
        let root = repo_root.join("examples").join("hello-world");
        assert!(root.is_dir(), "fixture missing at {}", root.display());

        // Build (debug).
        let mut build = Command::new("swift");
        build.arg("build").current_dir(&root).stdout(Stdio::piped()).stderr(Stdio::piped());
        apply_no_window(&mut build);
        let mut child = build.spawn().expect("swift build spawn");
        let out = child.stdout.take();
        let err = child.stderr.take();
        let mut build_lines: Vec<String> = Vec::new();
        if let Some(o) = out { read_stream(o, |l| build_lines.extend(l)); }
        if let Some(e) = err { read_stream(e, |l| build_lines.extend(l)); }
        let status = child.wait().expect("build wait");
        assert!(status.success(), "build failed: {build_lines:?}");
        assert!(
            build_lines.iter().any(|l| l.contains("Build complete") || l.contains("Compiling")),
            "expected build progress, got {build_lines:?}"
        );

        // Resolve + run.
        let bin = resolve_bin_path(BuildConfig::Debug, &root).expect("show-bin-path");
        let exe = bin.join(executable_file_name("HelloWorld"));
        assert!(exe.is_file(), "built exe missing at {}", exe.display());
        let mut run = Command::new(&exe);
        run.current_dir(&root).stdout(Stdio::piped()).stderr(Stdio::piped());
        apply_no_window(&mut run);
        let mut rchild = run.spawn().expect("run spawn");
        let ro = rchild.stdout.take();
        let mut run_lines: Vec<String> = Vec::new();
        if let Some(o) = ro { read_stream(o, |l| run_lines.extend(l)); }
        let rstatus = rchild.wait().expect("run wait");
        assert_eq!(rstatus.code(), Some(0), "program exit code");
        assert!(
            run_lines.iter().any(|l| l == "Hello, World!"),
            "expected Hello, World! in {run_lines:?}"
        );
    }
}
