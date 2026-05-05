// macOS-specific platform implementations — stub.
// macOS support is not on the v0.1 roadmap, but the platform abstraction
// keeps the door open if a future milestone adds it.

use crate::setup::checks::CheckResult;
use crate::setup::installs::{InstallOutcome, ProgressEvent};

pub fn check_vs_build_tools() -> CheckResult {
    unsupported()
}

pub fn check_wsl2() -> CheckResult {
    unsupported()
}

pub fn check_usbipd() -> CheckResult {
    unsupported()
}

pub fn check_toolchain() -> CheckResult {
    unsupported()
}

pub fn check_xtool() -> CheckResult {
    unsupported()
}

pub fn install_wsl2<F>(_on_event: F) -> InstallOutcome
where
    F: FnMut(ProgressEvent),
{
    install_unsupported()
}

pub fn install_usbipd<F>(_on_event: F) -> InstallOutcome
where
    F: FnMut(ProgressEvent),
{
    install_unsupported()
}

pub fn install_toolchain<F>(_on_event: F) -> InstallOutcome
where
    F: FnMut(ProgressEvent),
{
    install_unsupported()
}

pub fn install_xtool<F>(_on_event: F) -> InstallOutcome
where
    F: FnMut(ProgressEvent),
{
    install_unsupported()
}

fn unsupported() -> CheckResult {
    CheckResult {
        found: false,
        message: Some("Platform not supported in v0.1 (macOS not on roadmap).".to_string()),
        ..Default::default()
    }
}

fn install_unsupported() -> InstallOutcome {
    InstallOutcome::Failed {
        exit_code: -1,
        stderr: "Platform not supported in v0.1 (macOS not on roadmap).".to_string(),
    }
}
