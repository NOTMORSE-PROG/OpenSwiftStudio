// Swift toolchain compile self-test (FU-8).
//
// A pinned toolchain that installs cleanly can still fail to *compile* on a
// given user's machine — either a version bug (the 6.3.x Windows Foundation
// codegen crash, universal on Windows for that build) or a genuine
// unsupported-instruction crash on an old/low-end CPU. Both surface as a
// Windows crash exit code from `swift build`. This self-test builds a throwaway
// minimal package and classifies the outcome so the UI can show a clear,
// actionable message instead of letting the user hit a cryptic
// STATUS_ILLEGAL_INSTRUCTION later on Run.
//
// Empirically (Session 15, this host): `swift --version`, `swiftc -typecheck`
// and `swiftc -emit-object` all PASS on the broken 6.3.3 toolchain — only a
// full compile-to-executable (`swift build`) reproduces the crash. So the
// self-test must actually build an executable, not just typecheck.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use serde::Serialize;

/// Minimal probe package manifest. Written with `fs::write` (no BOM — a BOM on
/// the first line breaks SwiftPM's `swift-tools-version` parse). `6.0` is the
/// lowest tools version every pinned toolchain (6.2.4+) accepts.
const PROBE_MANIFEST: &str = "// swift-tools-version: 6.0\n\
import PackageDescription\n\
let package = Package(name: \"Probe\", targets: [.executableTarget(name: \"Probe\")])\n";
const PROBE_MAIN: &str = "print(\"ok\")\n";

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum SelfTestResult {
    /// `swift build` succeeded — the toolchain compiles on this machine.
    Healthy,
    /// `swift build` crashed (Windows NTSTATUS exception exit code, e.g.
    /// 0xC000001D illegal instruction / 0xC0000005 access violation). The
    /// toolchain cannot compile on this hardware — a clear message beats a
    /// cryptic Run failure.
    Crashed { exit_code: i32 },
    /// `swift build` failed for a non-crash reason (swift missing, unexpected
    /// build error). `detail` carries a truncated stderr for diagnostics.
    Failed { exit_code: i32, detail: String },
}

/// True when a process exit code is a Windows NTSTATUS *exception* (a crash)
/// rather than a normal program exit. Such codes have severity `ERROR`
/// (`0xC0000000`), so the top nibble of the u32 is `0xC`: illegal instruction
/// (`0xC000001D`), access violation (`0xC0000005`), stack overflow
/// (`0xC00000FD`), etc. A normal build error is a small positive code.
/// Shared with the run pipeline (M1-5) so a toolchain that crashes mid-build is
/// labeled distinctly from a compile error (FU-8 AC#2).
pub(crate) fn is_crash_exit(code: i32) -> bool {
    (code as u32) >> 28 == 0xC
}

/// Classify a completed `swift build` into a `SelfTestResult`.
fn classify(exit_code: Option<i32>, stderr: &str) -> SelfTestResult {
    match exit_code {
        Some(0) => SelfTestResult::Healthy,
        Some(code) if is_crash_exit(code) => SelfTestResult::Crashed { exit_code: code },
        Some(code) => SelfTestResult::Failed {
            exit_code: code,
            detail: truncate(stderr.trim(), 512),
        },
        // Killed by a signal with no code (rare on Windows) — treat as a crash.
        None => SelfTestResult::Crashed { exit_code: -1 },
    }
}

/// Build a throwaway minimal package with the `swift` on PATH and classify the
/// result. Runs `swift build` (the only invocation that reproduces the
/// codegen/link crash class — see module docs). Cleans up its temp dir.
pub fn run_toolchain_selftest() -> SelfTestResult {
    let dir = match make_probe_package() {
        Ok(d) => d,
        Err(e) => {
            return SelfTestResult::Failed {
                exit_code: -1,
                detail: format!("could not stage self-test package: {e}"),
            }
        }
    };

    let mut cmd = Command::new("swift");
    cmd.arg("build").current_dir(&dir);
    apply_no_window(&mut cmd);

    let result = match cmd.output() {
        Ok(output) => classify(
            output.status.code(),
            &String::from_utf8_lossy(&output.stderr),
        ),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => SelfTestResult::Failed {
            exit_code: -1,
            detail: "swift was not found on PATH".to_string(),
        },
        Err(e) => SelfTestResult::Failed { exit_code: -1, detail: e.to_string() },
    };

    let _ = fs::remove_dir_all(&dir);
    result
}

fn make_probe_package() -> std::io::Result<PathBuf> {
    let dir = std::env::temp_dir().join(format!(
        "ossw-selftest-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let src = dir.join("Sources").join("Probe");
    fs::create_dir_all(&src)?;
    fs::write(dir.join("Package.swift"), PROBE_MANIFEST)?;
    fs::write(src.join("main.swift"), PROBE_MAIN)?;
    Ok(dir)
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut end = max;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &s[..end])
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

    #[test]
    fn is_crash_exit_recognizes_ntstatus_exception_codes() {
        // Illegal instruction (our 6.3.x Foundation crash).
        assert!(is_crash_exit(-1073741795)); // 0xC000001D
        // Access violation, stack overflow.
        assert!(is_crash_exit(0xC0000005u32 as i32));
        assert!(is_crash_exit(0xC00000FDu32 as i32));
    }

    #[test]
    fn is_crash_exit_rejects_success_and_normal_build_errors() {
        assert!(!is_crash_exit(0)); // success
        assert!(!is_crash_exit(1)); // normal `swift build` error
        assert!(!is_crash_exit(2));
    }

    #[test]
    fn classify_maps_success_crash_and_failure() {
        assert_eq!(classify(Some(0), ""), SelfTestResult::Healthy);
        assert_eq!(
            classify(Some(-1073741795), ""),
            SelfTestResult::Crashed { exit_code: -1073741795 }
        );
        match classify(Some(1), "error: something") {
            SelfTestResult::Failed { exit_code, detail } => {
                assert_eq!(exit_code, 1);
                assert!(detail.contains("something"));
            }
            other => panic!("expected Failed, got {other:?}"),
        }
        assert!(matches!(classify(None, ""), SelfTestResult::Crashed { .. }));
    }

    #[test]
    fn result_serializes_camelcase_tagged() {
        let json = serde_json::to_string(&SelfTestResult::Crashed { exit_code: -1073741795 }).unwrap();
        assert!(json.contains("\"kind\":\"crashed\""));
        assert!(json.contains("\"exitCode\":-1073741795"));
        let healthy = serde_json::to_string(&SelfTestResult::Healthy).unwrap();
        assert!(healthy.contains("\"kind\":\"healthy\""));
    }

    /// Live end-to-end: run the real self-test against whatever `swift` is on
    /// PATH. On a healthy toolchain (6.2.4 here) -> Healthy. Point PATH+SDKROOT
    /// at the broken 6.3.3 toolchain and it returns Crashed. `#[ignore]`-gated
    /// because it invokes the real toolchain.
    #[test]
    #[ignore]
    fn selftest_runs_against_real_toolchain() {
        let result = run_toolchain_selftest();
        // We don't hard-assert Healthy vs Crashed (depends on which toolchain
        // is active), only that we get a decisive classification, never a hang.
        println!("self-test result: {result:?}");
        assert!(matches!(
            result,
            SelfTestResult::Healthy
                | SelfTestResult::Crashed { .. }
                | SelfTestResult::Failed { .. }
        ));
    }
}
