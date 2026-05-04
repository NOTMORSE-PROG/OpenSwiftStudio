// Linux-specific platform implementations — stub.
// Linux support arrives in v0.7 (provisional). The platform abstraction is
// kept clean from M0 so the eventual port is incremental, not a rewrite.

use crate::setup::checks::CheckResult;

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

fn unsupported() -> CheckResult {
    CheckResult {
        found: false,
        message: Some("Platform not supported in v0.1 (Linux planned for v0.7).".to_string()),
        ..Default::default()
    }
}
