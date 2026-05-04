// macOS-specific platform implementations — stub.
// macOS support is not on the v0.1 roadmap, but the platform abstraction
// keeps the door open if a future milestone adds it.

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
        message: Some("Platform not supported in v0.1 (macOS not on roadmap).".to_string()),
        ..Default::default()
    }
}
