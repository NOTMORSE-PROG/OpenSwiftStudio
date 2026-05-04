// Cross-platform check dispatch. Bodies live in `crate::platform::<os>`.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckResult {
    pub found: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub install_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

pub fn check_vs_build_tools() -> CheckResult {
    crate::platform::check_vs_build_tools()
}

pub fn check_wsl2() -> CheckResult {
    crate::platform::check_wsl2()
}

pub fn check_usbipd() -> CheckResult {
    crate::platform::check_usbipd()
}

pub fn check_toolchain() -> CheckResult {
    crate::platform::check_toolchain()
}
