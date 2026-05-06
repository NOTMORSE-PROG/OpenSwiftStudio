// Windows-specific platform implementations.
// M3 will add: emulator HWND embedding via Win32 SetParent.
// M9 will add: WSL2 + usbipd-win bridge to xtool.
// M0.5 (current): real detection bodies for VS Build Tools, WSL2,
// usbipd-win, and Swift toolchain. Each follows the same pattern: a primary
// authoritative probe, then less-specific fallbacks only when the primary
// can't give an answer (absent / errored / unparseable).

use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;

use crate::setup::checks::CheckResult;

pub(crate) const CREATE_NO_WINDOW: u32 = 0x0800_0000;

const BUILD_TOOLS_MIN_MAJOR: u32 = 16; // Build Tools 2019+

const VS_BUILD_TOOLS_INSTALL_URL: &str =
    "https://visualstudio.microsoft.com/visual-cpp-build-tools/";
const WSL_INSTALL_URL: &str = "https://learn.microsoft.com/windows/wsl/install";
const USBIPD_INSTALL_URL: &str =
    "https://github.com/dorssel/usbipd-win/releases/latest";
const SWIFT_INSTALL_URL: &str = "https://www.swift.org/install/windows/";

/// Outcome of a primary authoritative probe — distinguishes "the probe ran
/// and confirmed not present" (no fallback) from "the probe couldn't tell us"
/// (fall through to less-specific probes).
enum ProbeOutcome<T> {
    Found(T),
    AuthoritativeMissing,
    Inconclusive,
}

// ---------- VS Build Tools ----------

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
    if let Some(exe) = vswhere_path() {
        match probe_vs_via_vswhere(&exe) {
            ProbeOutcome::Found(result) => return result,
            ProbeOutcome::AuthoritativeMissing => {
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
            ProbeOutcome::Inconclusive => {}
        }
    }
    if let Some(result) = probe_vs_via_registry() {
        return result;
    }
    if let Some(result) = probe_vs_via_filesystem() {
        return result;
    }
    not_detected(
        "Visual Studio Build Tools 2019+ not detected. Install the C++ workload from",
        VS_BUILD_TOOLS_INSTALL_URL,
    )
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

fn probe_vs_via_vswhere(exe: &Path) -> ProbeOutcome<CheckResult> {
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
        return ProbeOutcome::Inconclusive;
    };
    if !output.status.success() {
        return ProbeOutcome::Inconclusive;
    }
    let Ok(installs) = serde_json::from_slice::<Vec<VsWhereInstall>>(&output.stdout) else {
        return ProbeOutcome::Inconclusive;
    };
    let chosen = installs.into_iter().find(|i| {
        parse_major(&i.installation_version)
            .map(|m| m >= BUILD_TOOLS_MIN_MAJOR)
            .unwrap_or(false)
    });
    match chosen {
        Some(install) => ProbeOutcome::Found(CheckResult {
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
        None => ProbeOutcome::AuthoritativeMissing,
    }
}

fn probe_vs_via_registry() -> Option<CheckResult> {
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

            if parse_major(&version)
                .map(|m| m >= BUILD_TOOLS_MIN_MAJOR)
                .unwrap_or(false)
            {
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

fn probe_vs_via_filesystem() -> Option<CheckResult> {
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
        let msbuild = root
            .join("MSBuild")
            .join("Current")
            .join("Bin")
            .join("MSBuild.exe");
        if msbuild.exists() {
            return Some(CheckResult {
                found: true,
                display_name: Some("Visual Studio Build Tools (filesystem-detected)".to_string()),
                version: None,
                install_path: Some(root.to_string_lossy().to_string()),
                message: Some(
                    "Detected via filesystem probe; version unknown (vswhere + registry unavailable)."
                        .to_string(),
                ),
            });
        }
    }
    None
}

// ---------- WSL2 ----------

pub fn check_wsl2() -> CheckResult {
    // Try `wsl --version` first (cleaner output, newer wsl.exe). Fall back to
    // `wsl --status` (older wsl.exe; localized strings — exit code 0 is the
    // primary signal). Then registry for the Lxss feature key. Then a simple
    // filesystem probe of wsl.exe.
    match probe_wsl_via_subprocess() {
        ProbeOutcome::Found(r) => return r,
        ProbeOutcome::AuthoritativeMissing => {
            return not_detected(
                "WSL2 not detected. Install via 'wsl --install' from an elevated terminal, or follow",
                WSL_INSTALL_URL,
            );
        }
        ProbeOutcome::Inconclusive => {}
    }
    if let Some(r) = probe_wsl_via_registry() {
        return r;
    }
    if let Some(r) = probe_wsl_via_filesystem() {
        return r;
    }
    not_detected(
        "WSL2 not detected. Install via 'wsl --install' from an elevated terminal, or follow",
        WSL_INSTALL_URL,
    )
}

fn probe_wsl_via_subprocess() -> ProbeOutcome<CheckResult> {
    let Ok(output) = Command::new("wsl.exe")
        .arg("--version")
        .creation_flags(CREATE_NO_WINDOW)
        .output()
    else {
        return ProbeOutcome::Inconclusive;
    };
    if !output.status.success() {
        // `wsl --version` may not exist on older wsl.exe builds; try --status.
        return probe_wsl_via_status();
    }
    let text = decode_utf16le_lossy(&output.stdout);
    if let Some(version) = parse_wsl_version_line(&text) {
        return ProbeOutcome::Found(CheckResult {
            found: true,
            display_name: Some("Windows Subsystem for Linux".to_string()),
            version: Some(version),
            install_path: None,
            message: None,
        });
    }
    // Output didn't match — could be a localized variant or unexpected build.
    // Fall through to less-specific probes.
    ProbeOutcome::Inconclusive
}

fn probe_wsl_via_status() -> ProbeOutcome<CheckResult> {
    let Ok(output) = Command::new("wsl.exe")
        .arg("--status")
        .creation_flags(CREATE_NO_WINDOW)
        .output()
    else {
        return ProbeOutcome::Inconclusive;
    };
    if output.status.success() {
        // wsl.exe is present and answered. Treat as found even if the localized
        // text doesn't parse for a default-distro line — the exit code alone
        // proves the feature is installed.
        return ProbeOutcome::Found(CheckResult {
            found: true,
            display_name: Some("Windows Subsystem for Linux".to_string()),
            version: Some("unknown (wsl --status answered)".to_string()),
            install_path: None,
            message: Some(
                "wsl.exe present; couldn't read --version (older build or localized output).".to_string(),
            ),
        });
    }
    // wsl.exe ran but exited non-zero — typical when the feature isn't installed
    // or no distros exist. Treat as authoritative missing.
    ProbeOutcome::AuthoritativeMissing
}

fn probe_wsl_via_registry() -> Option<CheckResult> {
    use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_READ};
    use winreg::RegKey;

    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let lxss = hklm
        .open_subkey_with_flags(r"SOFTWARE\Microsoft\Windows\CurrentVersion\Lxss", KEY_READ)
        .ok()?;
    if lxss.enum_keys().flatten().next().is_some() {
        Some(CheckResult {
            found: true,
            display_name: Some("Windows Subsystem for Linux".to_string()),
            version: None,
            install_path: None,
            message: Some(
                "Detected via registry (WSL feature key present; couldn't query wsl.exe).".to_string(),
            ),
        })
    } else {
        None
    }
}

fn probe_wsl_via_filesystem() -> Option<CheckResult> {
    let system_root = std::env::var("SystemRoot").ok()?;
    let wsl = Path::new(&system_root).join("System32").join("wsl.exe");
    if wsl.exists() {
        Some(CheckResult {
            found: true,
            display_name: Some("Windows Subsystem for Linux".to_string()),
            version: None,
            install_path: Some(wsl.to_string_lossy().to_string()),
            message: Some(
                "Detected via filesystem probe (wsl.exe present; subprocess + registry probes failed)."
                    .to_string(),
            ),
        })
    } else {
        None
    }
}

/// Decode a byte buffer as UTF-16 LE (with optional BOM stripping). WSL.exe
/// emits its console output in UTF-16 LE because of its Windows-Linux interop
/// layer; reading it as UTF-8 yields garbled text.
fn decode_utf16le_lossy(bytes: &[u8]) -> String {
    let bytes = if bytes.starts_with(&[0xFF, 0xFE]) {
        &bytes[2..]
    } else {
        bytes
    };
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    String::from_utf16_lossy(&units)
}

/// Extract the version from `wsl --version` output. Looks for a line like
/// "WSL version: 2.0.9.0" (English) and pulls the trailing version triple.
fn parse_wsl_version_line(text: &str) -> Option<String> {
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // English form: "WSL version: 2.0.9.0"
        if let Some(rest) = line.strip_prefix("WSL version:") {
            let v = rest.trim();
            if !v.is_empty() {
                return Some(v.to_string());
            }
        }
    }
    None
}

// ---------- usbipd-win ----------

pub fn check_usbipd() -> CheckResult {
    match probe_usbipd_via_subprocess() {
        ProbeOutcome::Found(r) => return r,
        ProbeOutcome::AuthoritativeMissing => {
            return not_detected(
                "usbipd-win not detected. Install via 'winget install dorssel.usbipd-win' or download the MSI from",
                USBIPD_INSTALL_URL,
            );
        }
        ProbeOutcome::Inconclusive => {}
    }
    if let Some(r) = probe_usbipd_via_filesystem() {
        return r;
    }
    not_detected(
        "usbipd-win not detected. Install via 'winget install dorssel.usbipd-win' or download the MSI from",
        USBIPD_INSTALL_URL,
    )
}

fn probe_usbipd_via_subprocess() -> ProbeOutcome<CheckResult> {
    let Ok(output) = Command::new("usbipd")
        .arg("--version")
        .creation_flags(CREATE_NO_WINDOW)
        .output()
    else {
        return ProbeOutcome::Inconclusive;
    };
    if !output.status.success() {
        return ProbeOutcome::AuthoritativeMissing;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let version = parse_usbipd_version(&text)
        .unwrap_or_else(|| text.trim().to_string());
    ProbeOutcome::Found(CheckResult {
        found: true,
        display_name: Some("usbipd-win".to_string()),
        version: Some(version),
        install_path: None,
        message: None,
    })
}

fn probe_usbipd_via_filesystem() -> Option<CheckResult> {
    let candidates = [
        std::env::var("ProgramFiles").ok(),
        std::env::var("ProgramW6432").ok(),
    ];
    for base in candidates.into_iter().flatten() {
        let exe = Path::new(&base).join("usbipd-win").join("usbipd.exe");
        if exe.exists() {
            return Some(CheckResult {
                found: true,
                display_name: Some("usbipd-win".to_string()),
                version: None,
                install_path: Some(exe.to_string_lossy().to_string()),
                message: Some(
                    "Detected via filesystem probe (usbipd.exe present but not on PATH)."
                        .to_string(),
                ),
            });
        }
    }
    None
}

/// Pull "5.0.0" out of "usbipd-win 5.0.0" or similar.
fn parse_usbipd_version(text: &str) -> Option<String> {
    let line = text.lines().next()?.trim();
    // Take the last whitespace-delimited token; usbipd's output is typically
    // "<name> <version>". Anything else, return None and let the caller use
    // the raw string.
    let token = line.split_whitespace().last()?;
    if token.chars().any(|c| c.is_ascii_digit()) {
        Some(token.to_string())
    } else {
        None
    }
}

// ---------- Swift toolchain ----------

pub fn check_toolchain() -> CheckResult {
    match probe_swift_via_subprocess() {
        ProbeOutcome::Found(r) => return r,
        ProbeOutcome::AuthoritativeMissing => {
            return not_detected(
                "Swift toolchain not detected. Download Swift 6.2.0+ for Windows from",
                SWIFT_INSTALL_URL,
            );
        }
        ProbeOutcome::Inconclusive => {}
    }
    if let Some(r) = probe_swift_via_filesystem() {
        return r;
    }
    not_detected(
        "Swift toolchain not detected. Download Swift 6.2.0+ for Windows from",
        SWIFT_INSTALL_URL,
    )
}

fn probe_swift_via_subprocess() -> ProbeOutcome<CheckResult> {
    let Ok(output) = Command::new("swift")
        .arg("--version")
        .creation_flags(CREATE_NO_WINDOW)
        .output()
    else {
        return ProbeOutcome::Inconclusive;
    };
    if !output.status.success() {
        return ProbeOutcome::AuthoritativeMissing;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let version = parse_swift_version(&text)
        .unwrap_or_else(|| text.lines().next().unwrap_or("").trim().to_string());
    ProbeOutcome::Found(CheckResult {
        found: true,
        display_name: Some("Swift".to_string()),
        version: Some(version),
        install_path: None,
        message: None,
    })
}

fn probe_swift_via_filesystem() -> Option<CheckResult> {
    let candidates = [
        std::env::var("LOCALAPPDATA").ok().map(|p| {
            Path::new(&p)
                .join("Programs")
                .join("Swift")
                .to_path_buf()
        }),
        std::env::var("ProgramFiles")
            .ok()
            .map(|p| Path::new(&p).join("Swift").to_path_buf()),
    ];
    for root in candidates.into_iter().flatten() {
        if root.exists() {
            return Some(CheckResult {
                found: true,
                display_name: Some("Swift (filesystem-detected)".to_string()),
                version: None,
                install_path: Some(root.to_string_lossy().to_string()),
                message: Some(
                    "Detected via filesystem probe (swift.exe present but not on PATH).".to_string(),
                ),
            });
        }
    }
    None
}

/// Extract a version triple like "6.2.0" from the first line of `swift --version`.
/// Typical output: "Swift version 6.2.0 (swift-6.2.0-RELEASE)".
fn parse_swift_version(text: &str) -> Option<String> {
    let first = text.lines().next()?;
    // Find the substring after "Swift version ".
    let after = first.split_once("Swift version ")?.1;
    // Take up to the next whitespace.
    let token = after.split_whitespace().next()?;
    if token.chars().any(|c| c.is_ascii_digit()) {
        Some(token.to_string())
    } else {
        None
    }
}

// ---------- xtool ----------
//
// xtool is a Linux AppImage that runs inside WSL. We probe + invoke it via
// `wsl -d <distro> <command>` against a chosen "user distro" rather than the
// `wsl` default — on hosts with Docker Desktop installed, the default distro
// is `docker-desktop` (a minimal Alpine env without bash) and isn't suitable
// for hosting xtool. Install location is `~/.local/bin/xtool` inside that
// chosen distro (per-user, no sudo).

const XTOOL_INSTALL_URL: &str =
    "https://github.com/xtool-org/xtool/releases/latest";

/// Distros that must be skipped — Docker Desktop's WSL integration distros
/// are minimal Alpine-based environments meant only for the Docker daemon.
const WSL_DISTRO_BLOCKLIST: &[&str] = &["docker-desktop", "docker-desktop-data"];

/// Pick the first installed WSL distro that isn't on the blocklist. Returns
/// `None` if WSL is absent OR if the only installed distros are Docker
/// Desktop's helpers. The wizard surfaces that as an actionable
/// "install Ubuntu first" message.
pub(crate) fn wsl_user_distro() -> Option<String> {
    let output = Command::new("wsl.exe")
        .args(["-l", "-q"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = decode_utf16le_lossy(&output.stdout);
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .find(|line| !WSL_DISTRO_BLOCKLIST.contains(line))
        .map(|s| s.to_string())
}

pub fn check_xtool() -> CheckResult {
    let Some(distro) = wsl_user_distro() else {
        return not_detected(
            "No suitable WSL distro found (only Docker Desktop's helpers don't count). \
             Install Ubuntu first via the WSL2 step, then come back to install xtool from",
            XTOOL_INSTALL_URL,
        );
    };
    let Ok(output) = Command::new("wsl.exe")
        .args(["-d", &distro, "--", "/bin/sh", "-c", "$HOME/.local/bin/xtool --version"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
    else {
        return not_detected(
            "Could not invoke wsl.exe. Confirm WSL2 is installed, then install xtool from",
            XTOOL_INSTALL_URL,
        );
    };
    if !output.status.success() {
        return not_detected(
            "xtool not installed in the WSL2 distro. The Install button below will download and install it from",
            XTOOL_INSTALL_URL,
        );
    }
    // wsl.exe sometimes wraps subprocess output in UTF-16 LE, sometimes not
    // (depends on whether it's the absolute-path form vs the bare command).
    // Try both decodings; whichever yields a parseable version line wins.
    let text_utf8 = String::from_utf8_lossy(&output.stdout).to_string();
    let text_utf16 = decode_utf16le_lossy(&output.stdout);
    let version = parse_xtool_version(&text_utf8)
        .or_else(|| parse_xtool_version(&text_utf16))
        .unwrap_or_else(|| text_utf8.lines().next().unwrap_or("").trim().to_string());
    CheckResult {
        found: true,
        display_name: Some("xtool".to_string()),
        version: Some(version),
        install_path: Some("~/.local/bin/xtool".to_string()),
        message: None,
    }
}

/// Extract "1.16.1" from xtool's `--version` output. Typical line:
/// `xtool 1.16.1` (and possibly extra trailing tokens). Falls back to the
/// last whitespace-delimited token of the first non-empty line if the
/// "xtool " prefix isn't found.
fn parse_xtool_version(text: &str) -> Option<String> {
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("xtool ") {
            let token = rest.split_whitespace().next()?;
            if token.chars().any(|c| c.is_ascii_digit()) {
                return Some(token.to_string());
            }
        }
        // Fallback: last token on the first non-empty line, if it looks numeric.
        if let Some(token) = line.split_whitespace().last() {
            if token.chars().any(|c| c.is_ascii_digit())
                && token.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false)
            {
                return Some(token.to_string());
            }
        }
        break;
    }
    None
}

/// Convert a Windows path like `C:\Users\<name>\AppData\Local\OpenSwiftStudio\foo`
/// to its WSL `/mnt/<drive>/...` equivalent. Lowercases the drive letter and
/// flips backslashes to forward slashes. Only meaningful for paths on a local
/// drive that WSL has auto-mounted (the typical case).
pub(crate) fn windows_path_to_wsl_mnt(p: &Path) -> Option<String> {
    let s = p.to_string_lossy().to_string();
    let mut chars = s.chars();
    let drive = chars.next()?;
    if !drive.is_ascii_alphabetic() {
        return None;
    }
    if chars.next()? != ':' {
        return None;
    }
    let rest = chars.collect::<String>();
    let rest = rest.replace('\\', "/");
    let rest = rest.trim_start_matches('/');
    Some(format!("/mnt/{}/{}", drive.to_ascii_lowercase(), rest))
}

// ---------- Installs ----------

use std::fs::File;
use std::io::{Read, Write};

use sha2::{Digest, Sha256};

use crate::setup::installs::{
    output_indicates_reboot, run_capture_utf16le, run_streaming, InstallOutcome,
    ProgressEvent, ProgressPhase,
};

/// Pinned Swift toolchain. URL and SHA256 are kept together so a rebuild
/// against a new Swift release is a two-line edit followed by an empirical
/// re-run of the `verify_swift_download_hash` test.
const SWIFT_DOWNLOAD_URL: &str =
    "https://download.swift.org/swift-6.2.4-release/windows10/swift-6.2.4-RELEASE/swift-6.2.4-RELEASE-windows10.exe";
const SWIFT_EXPECTED_SHA256: &str =
    "222501d4a0ef6ec3b2f08b3e0055140bb3a5136527542239bb925f979689f4ad";
const SWIFT_INSTALLER_FILENAME: &str = "swift-6.2.4-RELEASE-windows10.exe";

/// Pinned xtool AppImage. Bumping the version is a three-line edit followed
/// by an empirical re-run of `verify_xtool_download_hash`.
const XTOOL_DOWNLOAD_URL: &str =
    "https://github.com/xtool-org/xtool/releases/download/1.16.1/xtool-x86_64.AppImage";
const XTOOL_EXPECTED_SHA256: &str =
    "56aac91372980d2c37fdeb25cdb7e6f82d95dea439d5a6a66b974da4804d2d09";
const XTOOL_INSTALLER_FILENAME: &str = "xtool-x86_64.AppImage";

/// Spawn `wsl --install --no-launch`, capture its UTF-16 LE output, and
/// classify the result. Windows handles its own UAC prompt — we don't try to
/// elevate ourselves. `--no-launch` skips auto-starting the new distro, which
/// would otherwise spawn another elevation prompt from inside our subprocess.
pub fn install_wsl2<F>(mut on_event: F) -> InstallOutcome
where
    F: FnMut(ProgressEvent),
{
    let mut cmd = Command::new("wsl.exe");
    cmd.args(["--install", "--no-launch"])
        .creation_flags(CREATE_NO_WINDOW);

    match run_capture_utf16le(&mut cmd, &mut on_event) {
        Ok((exit_code, captured)) => {
            if output_indicates_reboot(&captured) {
                InstallOutcome::RebootRequired { stdout: captured }
            } else if exit_code == 0 {
                InstallOutcome::Success { stdout: captured }
            } else {
                InstallOutcome::Failed {
                    exit_code,
                    stderr: captured,
                }
            }
        }
        Err(e) => InstallOutcome::Failed {
            exit_code: -1,
            stderr: format!("Could not invoke wsl.exe: {e}"),
        },
    }
}

/// Try `winget install --id dorssel.usbipd-win --silent` first; if winget is
/// absent, fall back to downloading the latest release MSI from the
/// `tauri-plugin-http`-scoped GitHub URL and invoking `msiexec /i <path> /quiet`.
/// The MSI fallback path lands alongside the Swift toolchain download (same
/// download infrastructure); for now we emit a clear "winget required" error
/// if winget is absent so the user has an actionable next step.
pub fn install_usbipd<F>(mut on_event: F) -> InstallOutcome
where
    F: FnMut(ProgressEvent),
{
    if !winget_present() {
        return InstallOutcome::Failed {
            exit_code: -1,
            stderr:
                "winget not detected. Install App Installer from the Microsoft Store, or download \
                 the usbipd-win MSI directly from \
                 https://github.com/dorssel/usbipd-win/releases/latest ."
                    .to_string(),
        };
    }

    let mut cmd = Command::new("winget");
    cmd.args([
        "install",
        "--id",
        "dorssel.usbipd-win",
        "--silent",
        "--accept-source-agreements",
        "--accept-package-agreements",
    ])
    .creation_flags(CREATE_NO_WINDOW);

    match run_streaming(&mut cmd, &mut on_event) {
        Ok((0, captured)) => InstallOutcome::Success { stdout: captured },
        Ok((exit_code, captured)) => {
            // winget exit code 0x8A150011 (-1978335215) = no applicable update found
            // (i.e. already at latest); treat as success.
            if exit_code == -1_978_335_215_i32 {
                InstallOutcome::Success { stdout: captured }
            } else {
                InstallOutcome::Failed {
                    exit_code,
                    stderr: captured,
                }
            }
        }
        Err(e) => InstallOutcome::Failed {
            exit_code: -1,
            stderr: format!("Could not invoke winget: {e}"),
        },
    }
}

fn winget_present() -> bool {
    Command::new("winget")
        .arg("--version")
        .creation_flags(CREATE_NO_WINDOW)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Download Swift 6.2.4, verify its SHA256 against the pinned value, strip
/// Mark-of-the-Web (defensive — SmartScreen can block elevated launches of
/// internet-zone downloads), then run the installer with `/passive`. Per-user
/// install — no `-Verb RunAs` needed; Windows still shows the installer's own
/// progress UI thanks to `/passive`.
pub fn install_toolchain<F>(mut on_event: F) -> InstallOutcome
where
    F: FnMut(ProgressEvent),
{
    // Resolve download dest under %LOCALAPPDATA%\OpenSwiftStudio\downloads.
    let downloads_dir = match local_app_data_downloads_dir() {
        Ok(p) => p,
        Err(e) => {
            return InstallOutcome::Failed {
                exit_code: -1,
                stderr: format!("Could not resolve downloads dir: {e}"),
            };
        }
    };
    if let Err(e) = std::fs::create_dir_all(&downloads_dir) {
        return InstallOutcome::Failed {
            exit_code: -1,
            stderr: format!("Could not create {}: {e}", downloads_dir.display()),
        };
    }
    let dest = downloads_dir.join(SWIFT_INSTALLER_FILENAME);

    // Phase 1: download
    on_event(ProgressEvent::Progress {
        phase: ProgressPhase::Download,
        received: 0,
        total: 0,
    });
    let computed_hash = match download_with_progress(SWIFT_DOWNLOAD_URL, &dest, &mut on_event) {
        Ok(hex) => hex,
        Err(e) => {
            return InstallOutcome::Failed {
                exit_code: -1,
                stderr: format!("Download failed: {e}"),
            };
        }
    };

    // Phase 2: verify
    on_event(ProgressEvent::Progress {
        phase: ProgressPhase::Verify,
        received: 0,
        total: 0,
    });
    if !computed_hash.eq_ignore_ascii_case(SWIFT_EXPECTED_SHA256) {
        return InstallOutcome::Failed {
            exit_code: -1,
            stderr: format!(
                "SHA256 mismatch — expected {SWIFT_EXPECTED_SHA256}, got {computed_hash}. \
                 The downloaded file at {} may be corrupt or tampered. Delete it and try again.",
                dest.display()
            ),
        };
    }

    // Phase 3: strip MotW (defensive; reqwest doesn't add it but AVs sometimes do).
    strip_mark_of_the_web(&dest);

    // Phase 4: install
    on_event(ProgressEvent::Progress {
        phase: ProgressPhase::Install,
        received: 0,
        total: 0,
    });
    let mut cmd = Command::new(&dest);
    cmd.arg("/passive").creation_flags(CREATE_NO_WINDOW);
    let outcome = match run_streaming(&mut cmd, &mut on_event) {
        Ok((0, captured)) => InstallOutcome::Success { stdout: captured },
        Ok((exit_code, captured)) => InstallOutcome::Failed {
            exit_code,
            stderr: captured,
        },
        Err(e) => InstallOutcome::Failed {
            exit_code: -1,
            stderr: format!("Could not invoke installer: {e}"),
        },
    };

    // Cleanup on success — saves ~900 MB. Leave the file on failure so the
    // user can retry without re-downloading.
    if matches!(outcome, InstallOutcome::Success { .. }) {
        let _ = std::fs::remove_file(&dest);
    }

    outcome
}

/// Download xtool's x86_64 AppImage to the Windows-side downloads dir, verify
/// its SHA256 against the pinned value, then copy it into the WSL default
/// distro at `~/.local/bin/xtool` (per-user; no sudo). Verifies via
/// `wsl ~/.local/bin/xtool --version`. Cleans up the Windows-side AppImage on
/// success.
pub fn install_xtool<F>(mut on_event: F) -> InstallOutcome
where
    F: FnMut(ProgressEvent),
{
    let downloads_dir = match local_app_data_downloads_dir() {
        Ok(p) => p,
        Err(e) => {
            return InstallOutcome::Failed {
                exit_code: -1,
                stderr: format!("Could not resolve downloads dir: {e}"),
            };
        }
    };
    if let Err(e) = std::fs::create_dir_all(&downloads_dir) {
        return InstallOutcome::Failed {
            exit_code: -1,
            stderr: format!("Could not create {}: {e}", downloads_dir.display()),
        };
    }
    let dest = downloads_dir.join(XTOOL_INSTALLER_FILENAME);

    // Phase 1: download
    on_event(ProgressEvent::Progress {
        phase: ProgressPhase::Download,
        received: 0,
        total: 0,
    });
    let computed_hash = match download_with_progress(XTOOL_DOWNLOAD_URL, &dest, &mut on_event) {
        Ok(hex) => hex,
        Err(e) => {
            return InstallOutcome::Failed {
                exit_code: -1,
                stderr: format!("Download failed: {e}"),
            };
        }
    };

    // Phase 2: verify
    on_event(ProgressEvent::Progress {
        phase: ProgressPhase::Verify,
        received: 0,
        total: 0,
    });
    if !computed_hash.eq_ignore_ascii_case(XTOOL_EXPECTED_SHA256) {
        return InstallOutcome::Failed {
            exit_code: -1,
            stderr: format!(
                "SHA256 mismatch — expected {XTOOL_EXPECTED_SHA256}, got {computed_hash}. \
                 The downloaded file at {} may be corrupt or tampered. Delete it and try again.",
                dest.display()
            ),
        };
    }
    strip_mark_of_the_web(&dest);

    // Phase 3: copy into WSL + chmod +x
    on_event(ProgressEvent::Progress {
        phase: ProgressPhase::Install,
        received: 0,
        total: 0,
    });
    let Some(distro) = wsl_user_distro() else {
        return InstallOutcome::Failed {
            exit_code: -1,
            stderr:
                "No suitable WSL distro found. Docker Desktop's helper distros don't count — \
                 install Ubuntu via the WSL2 step (or `wsl --install -d Ubuntu` from a terminal) and try again."
                    .to_string(),
        };
    };
    let Some(mnt_path) = windows_path_to_wsl_mnt(&dest) else {
        return InstallOutcome::Failed {
            exit_code: -1,
            stderr: format!("Could not map {} to a /mnt/<drive>/ path.", dest.display()),
        };
    };
    // /bin/sh works in every distro; bash isn't guaranteed (e.g. minimal Alpine).
    let install_script = format!(
        "mkdir -p $HOME/.local/bin && cp '{mnt_path}' $HOME/.local/bin/xtool && chmod +x $HOME/.local/bin/xtool"
    );
    let mut cp_cmd = Command::new("wsl.exe");
    cp_cmd
        .args(["-d", &distro, "--", "/bin/sh", "-c", install_script.as_str()])
        .creation_flags(CREATE_NO_WINDOW);
    let outcome = match run_capture_utf16le(&mut cp_cmd, &mut on_event) {
        Ok((0, _captured)) => {
            // Verify by running xtool --version inside the same distro.
            let mut verify = Command::new("wsl.exe");
            verify
                .args([
                    "-d",
                    &distro,
                    "--",
                    "/bin/sh",
                    "-c",
                    "$HOME/.local/bin/xtool --version",
                ])
                .creation_flags(CREATE_NO_WINDOW);
            match run_capture_utf16le(&mut verify, &mut on_event) {
                Ok((0, version_out)) => {
                    let version_out_utf8 =
                        String::from_utf8_lossy(version_out.as_bytes()).to_string();
                    let line = parse_xtool_version(&version_out_utf8)
                        .map(|v| format!("xtool {v}"))
                        .unwrap_or_else(|| version_out_utf8.lines().next().unwrap_or("").to_string());
                    InstallOutcome::Success { stdout: line }
                }
                Ok((code, captured)) => InstallOutcome::Failed {
                    exit_code: code,
                    stderr: format!(
                        "xtool installed in {distro} but verify failed (exit {code}): {captured}"
                    ),
                },
                Err(e) => InstallOutcome::Failed {
                    exit_code: -1,
                    stderr: format!("Could not invoke wsl for verify: {e}"),
                },
            }
        }
        Ok((exit_code, captured)) => InstallOutcome::Failed {
            exit_code,
            stderr: if captured.trim().is_empty() {
                format!(
                    "WSL copy/chmod failed in {distro} (exit {exit_code}). Confirm the distro is healthy."
                )
            } else {
                captured
            },
        },
        Err(e) => InstallOutcome::Failed {
            exit_code: -1,
            stderr: format!("Could not invoke wsl.exe: {e}"),
        },
    };

    if matches!(outcome, InstallOutcome::Success { .. }) {
        let _ = std::fs::remove_file(&dest);
    }

    outcome
}

fn local_app_data_downloads_dir() -> std::io::Result<PathBuf> {
    let mut p = dirs::data_local_dir().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotFound, "LOCALAPPDATA not resolved")
    })?;
    p.push("OpenSwiftStudio");
    p.push("downloads");
    Ok(p)
}

/// Stream the response body to disk while computing SHA256 in the same pass,
/// emitting `Progress` events about every 512 KiB so the wizard's progress bar
/// updates roughly twice per second on a 1 MB/s link. Returns the SHA256 hex
/// of the bytes written. Caller is responsible for comparing against an
/// expected hash and for cleanup on failure.
pub(crate) fn download_with_progress<F>(
    url: &str,
    dest: &Path,
    on_event: &mut F,
) -> Result<String, Box<dyn std::error::Error>>
where
    F: FnMut(ProgressEvent),
{
    let client = reqwest::blocking::Client::builder()
        .user_agent(concat!("OpenSwiftStudio/", env!("CARGO_PKG_VERSION")))
        .build()?;
    let mut response = client.get(url).send()?.error_for_status()?;
    let total = response.content_length().unwrap_or(0);

    let mut file = File::create(dest)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    let mut received: u64 = 0;
    let mut bytes_since_emit: u64 = 0;
    const EMIT_EVERY: u64 = 512 * 1024;

    loop {
        let n = response.read(&mut buf)?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])?;
        hasher.update(&buf[..n]);
        received += n as u64;
        bytes_since_emit += n as u64;
        if bytes_since_emit >= EMIT_EVERY {
            on_event(ProgressEvent::Progress {
                phase: ProgressPhase::Download,
                received,
                total,
            });
            bytes_since_emit = 0;
        }
    }
    file.sync_all()?;
    // Final progress event so the UI shows 100% even if the last chunk was
    // smaller than the emit threshold.
    on_event(ProgressEvent::Progress {
        phase: ProgressPhase::Download,
        received,
        total: if total == 0 { received } else { total },
    });

    let digest = hasher.finalize();
    Ok(hex_lower(&digest))
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// Best-effort removal of the `Zone.Identifier` NTFS alternate data stream that
/// Windows attaches to internet-zone downloads. SmartScreen consults this stream
/// when an elevated launch happens; if it's present, the launch can fail
/// silently. reqwest doesn't add this stream itself, but AV / endpoint-protection
/// software sometimes does.
pub(crate) fn strip_mark_of_the_web(path: &Path) {
    let ads_path = format!("{}:Zone.Identifier", path.display());
    let _ = std::fs::remove_file(ads_path);
}

// ---------- Shared helpers ----------

fn parse_major(version: &str) -> Option<u32> {
    version.split('.').next()?.parse::<u32>().ok()
}

fn not_detected(prefix: &str, url: &str) -> CheckResult {
    CheckResult {
        found: false,
        message: Some(format!("{prefix} {url} .")),
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
    fn parse_wsl_version_line_extracts_triple() {
        let sample = "WSL version: 2.0.9.0\nKernel version: 5.15.133.1\nWSLg version: 1.0.59\n";
        assert_eq!(parse_wsl_version_line(sample), Some("2.0.9.0".to_string()));
    }

    #[test]
    fn parse_wsl_version_line_handles_missing() {
        assert_eq!(parse_wsl_version_line(""), None);
        assert_eq!(parse_wsl_version_line("Some other output"), None);
    }

    #[test]
    fn decode_utf16le_strips_bom_and_decodes() {
        // "WSL\n" as UTF-16 LE with BOM
        let bytes: Vec<u8> = vec![
            0xFF, 0xFE, // BOM
            b'W', 0x00, b'S', 0x00, b'L', 0x00, b'\n', 0x00,
        ];
        assert_eq!(decode_utf16le_lossy(&bytes), "WSL\n");
    }

    #[test]
    fn parse_usbipd_version_handles_typical_output() {
        assert_eq!(
            parse_usbipd_version("usbipd-win 5.0.0\n"),
            Some("5.0.0".to_string())
        );
        assert_eq!(
            parse_usbipd_version("usbipd-win 4.3.0"),
            Some("4.3.0".to_string())
        );
        assert_eq!(parse_usbipd_version(""), None);
    }

    #[test]
    fn parse_swift_version_extracts_triple() {
        let sample = "Swift version 6.2.0 (swift-6.2.0-RELEASE)\nTarget: x86_64-unknown-windows-msvc\n";
        assert_eq!(parse_swift_version(sample), Some("6.2.0".to_string()));
    }

    #[test]
    fn parse_swift_version_handles_missing() {
        assert_eq!(parse_swift_version(""), None);
        assert_eq!(parse_swift_version("not swift output"), None);
    }

    /// Empirical verification that `download_with_progress` streams the real
    /// Swift 6.2.4 installer cleanly, computes a SHA256 matching the pinned
    /// constant, and emits Progress events along the way. `#[ignore]`-gated
    /// because it downloads ~900 MB; run explicitly with
    /// `cargo test -- --ignored verify_swift_download_hash` whenever the
    /// pinned URL or hash changes.
    #[test]
    #[ignore = "downloads ~900 MB; opt-in via --ignored"]
    fn verify_swift_download_hash() {
        let tmp = std::env::temp_dir().join("oss-test-swift-6.2.4.exe");
        // Clean any prior run.
        let _ = std::fs::remove_file(&tmp);

        let mut progress_events = 0u32;
        let mut last_received: u64 = 0;
        let mut last_total: u64 = 0;
        let mut on_event = |e: ProgressEvent| {
            if let ProgressEvent::Progress { received, total, .. } = e {
                progress_events += 1;
                last_received = received;
                last_total = total;
            }
        };

        let hash = download_with_progress(SWIFT_DOWNLOAD_URL, &tmp, &mut on_event)
            .expect("download should succeed");

        assert!(
            hash.eq_ignore_ascii_case(SWIFT_EXPECTED_SHA256),
            "SHA256 mismatch: expected {SWIFT_EXPECTED_SHA256}, got {hash}"
        );
        assert!(progress_events > 10, "expected many progress events, got {progress_events}");
        assert_eq!(last_received, last_total, "final progress should report 100%");
        assert!(last_total > 100_000_000, "Swift installer should be > 100 MB, got {last_total}");

        // Cleanup.
        let _ = std::fs::remove_file(&tmp);
    }

    /// Empirical verification that the xtool 1.16.1 AppImage's SHA256 still
    /// matches the pinned constant (and that `download_with_progress` streams
    /// it correctly). Lighter than the Swift counterpart (~52 MB) so this can
    /// run more freely. Still `#[ignore]`-gated to avoid hammering GitHub on
    /// every `cargo test`.
    #[test]
    #[ignore = "downloads ~52 MB; opt-in via --ignored"]
    fn verify_xtool_download_hash() {
        let tmp = std::env::temp_dir().join("oss-test-xtool-1.16.1.AppImage");
        let _ = std::fs::remove_file(&tmp);

        let mut progress_events = 0u32;
        let mut last_received: u64 = 0;
        let mut last_total: u64 = 0;
        let mut on_event = |e: ProgressEvent| {
            if let ProgressEvent::Progress { received, total, .. } = e {
                progress_events += 1;
                last_received = received;
                last_total = total;
            }
        };

        let hash = download_with_progress(XTOOL_DOWNLOAD_URL, &tmp, &mut on_event)
            .expect("download should succeed");

        assert!(
            hash.eq_ignore_ascii_case(XTOOL_EXPECTED_SHA256),
            "SHA256 mismatch: expected {XTOOL_EXPECTED_SHA256}, got {hash}"
        );
        assert!(progress_events > 0, "expected at least one progress event");
        assert_eq!(last_received, last_total, "final progress should report 100%");
        assert!(last_total > 50_000_000, "xtool AppImage should be > 50 MB, got {last_total}");

        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn parse_xtool_version_extracts_release_triple() {
        assert_eq!(parse_xtool_version("xtool 1.16.1\n"), Some("1.16.1".to_string()));
        assert_eq!(parse_xtool_version("xtool 1.16.1 (build abc)\n"), Some("1.16.1".to_string()));
        // Fallback: last numeric token on the first non-empty line.
        assert_eq!(parse_xtool_version("Version: 1.16.1\n"), Some("1.16.1".to_string()));
    }

    #[test]
    fn parse_xtool_version_handles_missing() {
        assert_eq!(parse_xtool_version(""), None);
        assert_eq!(parse_xtool_version("not xtool"), None);
    }

    #[test]
    fn windows_path_to_wsl_mnt_handles_typical_paths() {
        let p = std::path::PathBuf::from(r"C:\Users\name\AppData\Local\OpenSwiftStudio\downloads\xtool-x86_64.AppImage");
        assert_eq!(
            windows_path_to_wsl_mnt(&p),
            Some(
                "/mnt/c/Users/name/AppData/Local/OpenSwiftStudio/downloads/xtool-x86_64.AppImage"
                    .to_string()
            )
        );
    }

    #[test]
    fn windows_path_to_wsl_mnt_lowercases_drive_letter() {
        let p = std::path::PathBuf::from(r"D:\Foo\Bar.txt");
        assert_eq!(
            windows_path_to_wsl_mnt(&p),
            Some("/mnt/d/Foo/Bar.txt".to_string())
        );
    }

    #[test]
    fn windows_path_to_wsl_mnt_rejects_unc_or_relative() {
        let p = std::path::PathBuf::from(r"\\server\share\file.txt");
        assert_eq!(windows_path_to_wsl_mnt(&p), None);
        let p2 = std::path::PathBuf::from(r"relative\path");
        assert_eq!(windows_path_to_wsl_mnt(&p2), None);
    }

    #[test]
    fn checks_always_return_a_message_or_metadata() {
        for (label, check) in [
            ("wsl2", check_wsl2 as fn() -> CheckResult),
            ("usbipd", check_usbipd as fn() -> CheckResult),
            ("toolchain", check_toolchain as fn() -> CheckResult),
            ("xtool", check_xtool as fn() -> CheckResult),
        ] {
            let r = check();
            if r.found {
                assert!(
                    r.display_name.is_some(),
                    "{label}: found result must include display_name"
                );
            } else {
                let msg = r.message.unwrap_or_default();
                assert!(
                    msg.contains("https://"),
                    "{label}: missing-result message must include an install URL, got: {msg}"
                );
            }
        }
    }
}
