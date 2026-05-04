// Cross-platform install dispatch for setup-wizard prerequisites. Bodies
// live in `crate::platform::<os>`. Bodies are blocking subprocess waits +
// HTTP downloads; the IPC layer wraps them in async tasks so the long-running
// installer doesn't block Tauri's IPC thread.

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum InstallOutcome {
    /// Installer ran cleanly. `stdout` is a captured tail for diagnostics.
    Success {
        stdout: String,
    },
    /// Installer ran but indicated a reboot is required to complete setup.
    /// The wizard surfaces this as a yellow alert with a "continue anyway" path.
    RebootRequired {
        stdout: String,
    },
    /// Installer failed — surface the exit code and stderr to the user so they
    /// can either retry or follow the install URL manually.
    Failed {
        exit_code: i32,
        stderr: String,
    },
}

/// Phase of an install that streams progress. Currently used by the toolchain
/// install (download → verify → install). WSL2 + usbipd installs only emit
/// `ProgressEvent::Line` since their subprocess output is the progress signal.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProgressPhase {
    Download,
    Verify,
    Install,
}

/// Per-line / per-chunk update streamed from an install in progress. The IPC
/// layer maps each event to a `setup-install-progress` Tauri event so the
/// wizard can render a live preview (log lines + a progress bar).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ProgressEvent {
    /// A subprocess output line (stdout or stderr).
    Line {
        line: String,
    },
    /// A bytes-received update for a phase that has known total size. `total = 0`
    /// is allowed when the server omits Content-Length; the UI shows an
    /// indeterminate state in that case.
    Progress {
        phase: ProgressPhase,
        received: u64,
        total: u64,
    },
}

pub fn install_wsl2<F>(on_event: F) -> InstallOutcome
where
    F: FnMut(ProgressEvent),
{
    crate::platform::install_wsl2(on_event)
}

pub fn install_usbipd<F>(on_event: F) -> InstallOutcome
where
    F: FnMut(ProgressEvent),
{
    crate::platform::install_usbipd(on_event)
}

pub fn install_toolchain<F>(on_event: F) -> InstallOutcome
where
    F: FnMut(ProgressEvent),
{
    crate::platform::install_toolchain(on_event)
}

/// Buffer a subprocess's full stdout/stderr, decode as UTF-16 LE (with optional
/// BOM), then emit lines via `on_event` (as `ProgressEvent::Line`) and return
/// `(exit_code, captured_tail)`. Used for wsl.exe, which always emits UTF-16 LE
/// on Windows. Loses real-time streaming, but install commands are short
/// enough (~10–60 s) that the user sees the whole log when the install
/// finishes — acceptable for v0.1.
pub fn run_capture_utf16le<F>(
    cmd: &mut Command,
    mut on_event: F,
) -> std::io::Result<(i32, String)>
where
    F: FnMut(ProgressEvent),
{
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let output = cmd.spawn()?.wait_with_output()?;
    let combined: Vec<u8> = output.stdout.iter().chain(output.stderr.iter()).copied().collect();

    let bytes: &[u8] = if combined.starts_with(&[0xFF, 0xFE]) {
        &combined[2..]
    } else {
        &combined
    };
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    let decoded = String::from_utf16_lossy(&units);

    for line in decoded.lines() {
        if !line.is_empty() {
            on_event(ProgressEvent::Line { line: line.to_string() });
        }
    }

    let tail = trim_tail(&decoded);
    Ok((output.status.code().unwrap_or(-1), tail))
}

fn trim_tail(text: &str) -> String {
    const TAIL_BUDGET_BYTES: usize = 4096;
    if text.len() <= TAIL_BUDGET_BYTES {
        return text.to_string();
    }
    let drop_to = text.len() - TAIL_BUDGET_BYTES;
    if let Some((idx, _)) = text.char_indices().find(|(i, _)| *i >= drop_to) {
        text[idx..].to_string()
    } else {
        String::new()
    }
}

/// True iff the captured installer output looks like a "reboot to finish" signal.
/// Case-insensitive scan for "reboot" or "restart" — covers Microsoft's standard
/// post-install message ("The requested operation is successful. Changes will
/// not be effective until the system is rebooted.") and most other installer
/// reboot prompts.
pub fn output_indicates_reboot(text: &str) -> bool {
    let lower = text.to_lowercase();
    lower.contains("reboot") || lower.contains("restart")
}

/// Spawn a subprocess, pipe stdout+stderr line-by-line as `ProgressEvent::Line`
/// events (so the frontend can stream a live log preview), and collect a tail
/// of the lines for diagnostics on completion. Returns `(exit_code,
/// captured_tail)`. Tail is capped at the last 4 KiB of bytes so a noisy
/// installer can't blow up the InstallOutcome's serialization size.
pub fn run_streaming<F>(cmd: &mut Command, mut on_event: F) -> std::io::Result<(i32, String)>
where
    F: FnMut(ProgressEvent),
{
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn()?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    // Tail buffer — append every line, truncate from the front when over budget.
    let mut tail = String::new();
    const TAIL_BUDGET_BYTES: usize = 4096;

    let mut emit = |line: &str| {
        on_event(ProgressEvent::Line { line: line.to_string() });
        if !tail.is_empty() {
            tail.push('\n');
        }
        tail.push_str(line);
        if tail.len() > TAIL_BUDGET_BYTES {
            // Drop oldest characters until back under budget. Simple char-aware
            // trim — keeps the most recent diagnostics.
            let drop_to = tail.len() - TAIL_BUDGET_BYTES;
            if let Some((idx, _)) = tail.char_indices().find(|(i, _)| *i >= drop_to) {
                tail.drain(..idx);
            }
        }
    };

    if let Some(out) = stdout {
        for line in BufReader::new(out).lines().map_while(Result::ok) {
            emit(&line);
        }
    }
    if let Some(err) = stderr {
        for line in BufReader::new(err).lines().map_while(Result::ok) {
            emit(&line);
        }
    }

    let status = child.wait()?;
    Ok((status.code().unwrap_or(-1), tail))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_outcome_success_round_trips_via_serde() {
        let original = InstallOutcome::Success {
            stdout: "Installation succeeded.\n".to_string(),
        };
        let json = serde_json::to_string(&original).expect("serialize");
        assert!(json.contains("\"kind\":\"success\""));
        assert!(json.contains("\"stdout\""));
        let parsed: InstallOutcome = serde_json::from_str(&json).expect("deserialize");
        match parsed {
            InstallOutcome::Success { stdout } => assert!(stdout.contains("succeeded")),
            other => panic!("expected Success, got {other:?}"),
        }
    }

    #[test]
    fn install_outcome_reboot_required_round_trips() {
        let original = InstallOutcome::RebootRequired {
            stdout: "The requested operation is successful. Changes will not be effective until the system is rebooted.".to_string(),
        };
        let json = serde_json::to_string(&original).expect("serialize");
        assert!(json.contains("\"kind\":\"rebootRequired\""));
        let parsed: InstallOutcome = serde_json::from_str(&json).expect("deserialize");
        matches!(parsed, InstallOutcome::RebootRequired { .. });
    }

    #[test]
    fn output_indicates_reboot_recognises_microsoft_phrasing() {
        assert!(output_indicates_reboot(
            "The requested operation is successful. Changes will not be effective until the system is rebooted."
        ));
        assert!(output_indicates_reboot("A restart is required to complete the installation."));
        assert!(output_indicates_reboot("Reboot Required"));
    }

    #[test]
    fn output_indicates_reboot_returns_false_on_clean_output() {
        assert!(!output_indicates_reboot(""));
        assert!(!output_indicates_reboot("Successfully installed usbipd-win 5.0.0"));
        assert!(!output_indicates_reboot("Operation completed."));
    }

    #[test]
    fn trim_tail_keeps_recent_chars_when_over_budget() {
        let big = "a".repeat(8000);
        let trimmed = trim_tail(&big);
        assert_eq!(trimmed.len(), 4096);
        assert!(trimmed.chars().all(|c| c == 'a'));
    }

    #[test]
    fn trim_tail_returns_input_when_under_budget() {
        assert_eq!(trim_tail("short"), "short");
        assert_eq!(trim_tail(""), "");
    }

    #[test]
    fn progress_event_line_round_trips_via_serde() {
        let original = ProgressEvent::Line { line: "Downloading...".to_string() };
        let json = serde_json::to_string(&original).expect("serialize");
        assert!(json.contains("\"kind\":\"line\""));
        assert!(json.contains("\"line\":\"Downloading...\""));
        let parsed: ProgressEvent = serde_json::from_str(&json).expect("deserialize");
        match parsed {
            ProgressEvent::Line { line } => assert_eq!(line, "Downloading..."),
            other => panic!("expected Line, got {other:?}"),
        }
    }

    #[test]
    fn progress_event_progress_round_trips_with_phase_and_bytes() {
        let original = ProgressEvent::Progress {
            phase: ProgressPhase::Download,
            received: 12_345_678,
            total: 899_032_040,
        };
        let json = serde_json::to_string(&original).expect("serialize");
        assert!(json.contains("\"kind\":\"progress\""));
        assert!(json.contains("\"phase\":\"download\""));
        assert!(json.contains("\"received\":12345678"));
        assert!(json.contains("\"total\":899032040"));
        let parsed: ProgressEvent = serde_json::from_str(&json).expect("deserialize");
        match parsed {
            ProgressEvent::Progress { received, total, .. } => {
                assert_eq!(received, 12_345_678);
                assert_eq!(total, 899_032_040);
            }
            other => panic!("expected Progress, got {other:?}"),
        }
    }

    #[test]
    fn install_outcome_failed_round_trips() {
        let original = InstallOutcome::Failed {
            exit_code: 1603,
            stderr: "Fatal error during installation.".to_string(),
        };
        let json = serde_json::to_string(&original).expect("serialize");
        assert!(json.contains("\"kind\":\"failed\""));
        assert!(json.contains("\"exitCode\":1603"));
        let parsed: InstallOutcome = serde_json::from_str(&json).expect("deserialize");
        match parsed {
            InstallOutcome::Failed { exit_code, .. } => assert_eq!(exit_code, 1603),
            other => panic!("expected Failed, got {other:?}"),
        }
    }
}
