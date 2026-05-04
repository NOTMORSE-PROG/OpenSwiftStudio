// Platform abstraction layer.
//
// Each platform-specific concern (window embedding, USB iPhone deploy, setup
// wizard prerequisites, package format) lives in the per-platform module.
// Re-exports below give callers a single `crate::platform::<fn>` entry that
// resolves to the right impl per target_os without scattering cfg gates.

#[cfg(target_os = "windows")]
pub(crate) mod windows;

#[cfg(target_os = "linux")]
pub(crate) mod linux;

#[cfg(target_os = "macos")]
pub(crate) mod macos;

#[cfg(target_os = "windows")]
pub use windows::{check_vs_build_tools, check_wsl2, check_usbipd, check_toolchain};

#[cfg(target_os = "linux")]
pub use linux::{check_vs_build_tools, check_wsl2, check_usbipd, check_toolchain};

#[cfg(target_os = "macos")]
pub use macos::{check_vs_build_tools, check_wsl2, check_usbipd, check_toolchain};
