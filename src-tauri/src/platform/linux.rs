// Linux-specific platform implementations — stub.
// Linux support arrives in v0.7 (provisional). The platform abstraction is
// kept clean from M0 so the eventual port is incremental, not a rewrite.

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
        message: Some("Platform not supported in v0.1 (Linux planned for v0.7).".to_string()),
        ..Default::default()
    }
}

fn install_unsupported() -> InstallOutcome {
    InstallOutcome::Failed {
        exit_code: -1,
        stderr: "Platform not supported in v0.1 (Linux planned for v0.7).".to_string(),
    }
}
