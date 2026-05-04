// Windows-specific platform implementations.
// M3 will add: emulator HWND embedding via Win32 SetParent.
// M9 will add: WSL2 + usbipd-win bridge to xtool.
// M0.5 (this chunk): VS Build Tools detection. WSL2/usbipd/toolchain stubs;
// real bodies arrive in the next M0.5 chunk.

use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;

use crate::setup::checks::CheckResult;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

const NOT_IMPLEMENTED_MSG: &str =
    "Not implemented in M0.5 foundation chunk; lands in next session.";

const BUILD_TOOLS_MIN_MAJOR: u32 = 16; // Build Tools 2019+

// Returned by `vswhere.exe -format json -utf8`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VsWhereInstall {
    #[serde(default)]
    installation_version: String,
    #[serde(default)]
    installation_path: String,
    #[serde(default)]
    display_name: String,
}

pub fn check_vs_build_tools() -> CheckResult {
    // vswhere is the authoritative source when present: it knows which workloads
    // are installed, including the C++ tools that Swift on Windows requires.
    // Registry / filesystem fallbacks only kick in when vswhere itself is
    // unavailable or errored — they cannot filter by workload, so accepting them
    // when vswhere has already said "no match" would be a false positive.
    if let Some(exe) = vswhere_path() {
        match probe_via_vswhere(&exe) {
            VsWhereOutcome::Found(result) => return result,
            VsWhereOutcome::AuthoritativeMissing => {
                return CheckResult {
                    found: false,
                    message: Some(
                        "Visual Studio (any edition) is installed, but the C++ build-tools \
                         workload (Microsoft.VisualStudio.Workload.VCTools) Swift on Windows \
                         requires is missing. Install via the Visual Studio Installer (modify \
                         your install and add 'Desktop development with C++') or get the \
                         standalone Build Tools at \
                         https://visualstudio.microsoft.com/visual-cpp-build-tools/ ."
                            .to_string(),
                    ),
                    ..Default::default()
                };
            }
            VsWhereOutcome::Inconclusive => {
                // vswhere errored / produced unparseable output. Fall through to the
                // less-specific probes so we still try to report something useful.
            }
        }
    }
    if let Some(result) = probe_via_registry() {
        return result;
    }
    if let Some(result) = probe_via_filesystem() {
        return result;
    }
    CheckResult {
        found: false,
        message: Some(
            "Visual Studio Build Tools 2019+ not detected. Install the C++ workload from \
             https://visualstudio.microsoft.com/visual-cpp-build-tools/ ."
                .to_string(),
        ),
        ..Default::default()
    }
}

fn vswhere_path() -> Option<PathBuf> {
    let pf86 = std::env::var("ProgramFiles(x86)").ok()?;
    let path = Path::new(&pf86)
        .join("Microsoft Visual Studio")
        .join("Installer")
        .join("vswhere.exe");
    if path.exists() {
        Some(path)
    } else {
        None
    }
}

enum VsWhereOutcome {
    Found(CheckResult),
    AuthoritativeMissing,
    Inconclusive,
}

fn probe_via_vswhere(exe: &Path) -> VsWhereOutcome {
    let Ok(output) = Command::new(exe)
        .args([
            "-products",
            "*",
            "-requires",
            "Microsoft.VisualStudio.Workload.VCTools",
            "-format",
            "json",
            "-utf8",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
    else {
        return VsWhereOutcome::Inconclusive;
    };
    if !output.status.success() {
        return VsWhereOutcome::Inconclusive;
    }
    let Ok(installs) = serde_json::from_slice::<Vec<VsWhereInstall>>(&output.stdout) else {
        return VsWhereOutcome::Inconclusive;
    };
    let chosen = installs.into_iter().find(|i| {
        parse_major(&i.installation_version)
            .map(|m| m >= BUILD_TOOLS_MIN_MAJOR)
            .unwrap_or(false)
    });
    match chosen {
        Some(install) => VsWhereOutcome::Found(CheckResult {
            found: true,
            display_name: Some(if install.display_name.is_empty() {
                "Visual Studio Build Tools".to_string()
            } else {
                install.display_name
            }),
            version: Some(install.installation_version),
            install_path: Some(install.installation_path),
            message: None,
        }),
        None => VsWhereOutcome::AuthoritativeMissing,
    }
}

fn probe_via_registry() -> Option<CheckResult> {
    use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_READ};
    use winreg::RegKey;

    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let candidates = [
        r"SOFTWARE\Microsoft\VisualStudio\Setup\Instances",
        r"SOFTWARE\WOW6432Node\Microsoft\VisualStudio\Setup\Instances",
    ];
    for root in candidates {
        let Ok(instances) = hklm.open_subkey_with_flags(root, KEY_READ) else {
            continue;
        };
        for name in instances.enum_keys().flatten() {
            let Ok(instance) = instances.open_subkey_with_flags(&name, KEY_READ) else {
                continue;
            };
            let version: String = instance
                .get_value("InstallationVersion")
                .or_else(|_| instance.get_value("installationVersion"))
                .unwrap_or_default();
            let install_path: String = instance
                .get_value("InstallationPath")
                .or_else(|_| instance.get_value("installationPath"))
                .unwrap_or_default();
            let display_name: String = instance
                .get_value("DisplayName")
                .or_else(|_| instance.get_value("displayName"))
                .unwrap_or_else(|_| "Visual Studio Build Tools".to_string());

            if parse_major(&version).map(|m| m >= BUILD_TOOLS_MIN_MAJOR).unwrap_or(false) {
                return Some(CheckResult {
                    found: true,
                    display_name: Some(display_name),
                    version: Some(version),
                    install_path: if install_path.is_empty() {
                        None
                    } else {
                        Some(install_path)
                    },
                    message: Some("Detected via registry (vswhere unavailable).".to_string()),
                });
            }
        }
    }
    None
}

fn probe_via_filesystem() -> Option<CheckResult> {
    let pf = std::env::var("ProgramFiles").ok();
    let pf86 = std::env::var("ProgramFiles(x86)").ok();
    let mut roots: Vec<PathBuf> = Vec::new();
    for base in [pf.as_deref(), pf86.as_deref()].into_iter().flatten() {
        for year in ["2022", "2019"] {
            roots.push(
                Path::new(base)
                    .join("Microsoft Visual Studio")
                    .join(year)
                    .join("BuildTools"),
            );
        }
    }
    for root in roots {
        let msbuild = root.join("MSBuild").join("Current").join("Bin").join("MSBuild.exe");
        if msbuild.exists() {
            return Some(CheckResult {
                found: true,
                display_name: Some("Visual Studio Build Tools (filesystem-detected)".to_string()),
                version: None,
                install_path: Some(root.to_string_lossy().to_string()),
                message: Some(
                    "Detected via filesystem probe; version unknown (vswhere + registry unavailable).".to_string(),
                ),
            });
        }
    }
    None
}

fn parse_major(version: &str) -> Option<u32> {
    version.split('.').next()?.parse::<u32>().ok()
}

pub fn check_wsl2() -> CheckResult {
    not_implemented_stub()
}

pub fn check_usbipd() -> CheckResult {
    not_implemented_stub()
}

pub fn check_toolchain() -> CheckResult {
    not_implemented_stub()
}

fn not_implemented_stub() -> CheckResult {
    CheckResult {
        found: false,
        message: Some(NOT_IMPLEMENTED_MSG.to_string()),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_major_handles_typical_version_strings() {
        assert_eq!(parse_major("17.14.37216.2"), Some(17));
        assert_eq!(parse_major("16.0.0"), Some(16));
        assert_eq!(parse_major("garbage"), None);
        assert_eq!(parse_major(""), None);
    }

    #[test]
    fn vs_build_tools_check_returns_a_message_on_this_host() {
        // Whatever the result on a developer's machine, the function must
        // always return a CheckResult with a useful message (either "Detected"
        // metadata or an actionable next-step). The wizard renders this verbatim.
        let result = check_vs_build_tools();
        if result.found {
            assert!(result.display_name.is_some(), "found result needs display_name");
        } else {
            let msg = result.message.unwrap_or_default();
            assert!(
                msg.to_lowercase().contains("c++") || msg.to_lowercase().contains("build tools"),
                "missing-tools message should mention C++ or Build Tools, got: {msg}"
            );
            assert!(
                msg.contains("https://"),
                "missing-tools message should include an install URL, got: {msg}"
            );
        }
    }

    #[test]
    fn stubs_announce_themselves() {
        for stub in [check_wsl2, check_usbipd, check_toolchain] {
            let r = stub();
            assert!(!r.found);
            assert!(r.message.unwrap_or_default().contains("M0.5 foundation chunk"));
        }
    }
}
