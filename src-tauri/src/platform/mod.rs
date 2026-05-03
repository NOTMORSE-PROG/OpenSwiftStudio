// Platform abstraction layer — see ADR-006.
//
// Each platform-specific concern (window embedding, USB iPhone deploy, setup
// wizard prerequisites, package format) gets a trait here, with the impl
// living in the per-platform module. M0 has no real implementations yet.

#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "linux")]
mod linux;

#[cfg(target_os = "macos")]
mod macos;
