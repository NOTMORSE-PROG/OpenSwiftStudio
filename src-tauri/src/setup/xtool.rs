// Two-stage `xtool auth login` + `xtool sdk install` driver.
//
// `xtool auth login` and `xtool sdk install` accept real flags, so we drive
// them directly rather than piping stdin to the interactive `xtool setup`
// wrapper. That keeps the implementation free of a prompt parser and
// resilient to prompt-string changes across xtool releases.
//
// On non-Windows targets the body is stubbed out with a Failed outcome since
// xtool only runs inside the user's WSL2 distro on Windows in v0.1.

use crate::setup::installs::{InstallOutcome, ProgressEvent};

/// Run `xtool auth login` followed (only on success) by `xtool sdk install`.
/// Streams subprocess lines via `on_event` and emits an Install-phase progress
/// event after each stage so the wizard's progress bar advances.
pub fn run_xtool_setup<F>(
    email: &str,
    password: &str,
    xip_windows_path: &str,
    on_event: F,
) -> InstallOutcome
where
    F: FnMut(ProgressEvent),
{
    #[cfg(target_os = "windows")]
    {
        windows_impl::run(email, password, xip_windows_path, on_event)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (email, password, xip_windows_path, on_event);
        InstallOutcome::Failed {
            exit_code: -1,
            stderr: "xtool setup is only supported on Windows in v0.1.".to_string(),
        }
    }
}

/// POSIX single-quote escape: wraps `s` in single quotes and replaces any
/// embedded single quote with `'\''`. The result is safe to splice into a
/// `/bin/sh -c "..."` argument — the only character single quotes can't
/// escape is the single quote itself, hence the close-reopen trick.
pub(crate) fn shell_single_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for c in s.chars() {
        if c == '\'' {
            out.push_str("'\\''");
        } else {
            out.push(c);
        }
    }
    out.push('\'');
    out
}

/// Replace literal occurrences of `password` in `text` with a fixed mask.
/// xtool generally doesn't echo the password back, but if a future version
/// does, the captured tail is sanitized before it reaches the wizard's UI.
pub(crate) fn sanitize_stderr(text: &str, password: &str) -> String {
    if password.is_empty() {
        return text.to_string();
    }
    text.replace(password, "********")
}

#[cfg(target_os = "windows")]
mod windows_impl {
    use std::os::windows::process::CommandExt;
    use std::path::PathBuf;
    use std::process::Command;

    use crate::platform::windows::{
        windows_path_to_wsl_mnt, wsl_user_distro, CREATE_NO_WINDOW,
    };
    use crate::setup::installs::{
        run_capture_utf16le, InstallOutcome, ProgressEvent, ProgressPhase,
    };

    use super::{sanitize_stderr, shell_single_quote};

    pub fn run<F>(
        email: &str,
        password: &str,
        xip_windows_path: &str,
        mut on_event: F,
    ) -> InstallOutcome
    where
        F: FnMut(ProgressEvent),
    {
        let Some(distro) = wsl_user_distro() else {
            return InstallOutcome::Failed {
                exit_code: -1,
                stderr:
                    "No suitable WSL distro found. Docker Desktop's helper distros don't count — \
                     install Ubuntu via the WSL2 step (or `wsl --install -d Ubuntu` from a terminal) and try again."
                        .to_string(),
            };
        };

        let xip_path = PathBuf::from(xip_windows_path);
        match xip_path.extension().and_then(|s| s.to_str()) {
            Some(ext) if ext.eq_ignore_ascii_case("xip") => {}
            _ => {
                return InstallOutcome::Failed {
                    exit_code: -1,
                    stderr: format!(
                        "Expected an Xcode .xip archive; got {xip_windows_path}."
                    ),
                };
            }
        }
        let Some(xip_mnt) = windows_path_to_wsl_mnt(&xip_path) else {
            return InstallOutcome::Failed {
                exit_code: -1,
                stderr: format!(
                    "Could not map {xip_windows_path} to /mnt/<drive>/. \
                     The .xip needs to live on a local drive WSL can see."
                ),
            };
        };

        // Stage 1: xtool auth login. Single-quote-escape every user-supplied
        // value so an embedded "'" in (say) a generated app-specific password
        // doesn't break out of the shell argument.
        let auth_script = format!(
            "$HOME/.local/bin/xtool auth login -u {} -p {} -m password",
            shell_single_quote(email),
            shell_single_quote(password),
        );
        let mut auth_cmd = Command::new("wsl.exe");
        auth_cmd
            .args(["-d", &distro, "--", "/bin/sh", "-c", auth_script.as_str()])
            .creation_flags(CREATE_NO_WINDOW);
        let (auth_code, auth_tail) = match run_capture_utf16le(&mut auth_cmd, &mut on_event) {
            Ok(pair) => pair,
            Err(e) => {
                return InstallOutcome::Failed {
                    exit_code: -1,
                    stderr: format!("Could not invoke wsl.exe for xtool auth login: {e}"),
                };
            }
        };
        if auth_code != 0 {
            let safe = sanitize_stderr(&auth_tail, password);
            return InstallOutcome::Failed {
                exit_code: auth_code,
                stderr: if safe.trim().is_empty() {
                    format!(
                        "xtool auth login failed (exit {auth_code}). \
                         Check the Apple ID + app-specific password and try again."
                    )
                } else {
                    safe
                },
            };
        }

        on_event(ProgressEvent::Progress {
            phase: ProgressPhase::Install,
            received: 1,
            total: 2,
        });

        // Stage 2: xtool sdk install <xip-path>.
        let sdk_script = format!(
            "$HOME/.local/bin/xtool sdk install {}",
            shell_single_quote(&xip_mnt),
        );
        let mut sdk_cmd = Command::new("wsl.exe");
        sdk_cmd
            .args(["-d", &distro, "--", "/bin/sh", "-c", sdk_script.as_str()])
            .creation_flags(CREATE_NO_WINDOW);
        let (sdk_code, sdk_tail) = match run_capture_utf16le(&mut sdk_cmd, &mut on_event) {
            Ok(pair) => pair,
            Err(e) => {
                return InstallOutcome::Failed {
                    exit_code: -1,
                    stderr: format!("Could not invoke wsl.exe for xtool sdk install: {e}"),
                };
            }
        };
        if sdk_code != 0 {
            return InstallOutcome::Failed {
                exit_code: sdk_code,
                stderr: if sdk_tail.trim().is_empty() {
                    format!(
                        "xtool sdk install failed (exit {sdk_code}). \
                         Confirm the .xip is a valid Xcode archive downloaded from developer.apple.com."
                    )
                } else {
                    sdk_tail
                },
            };
        }

        on_event(ProgressEvent::Progress {
            phase: ProgressPhase::Install,
            received: 2,
            total: 2,
        });
        InstallOutcome::Success {
            stdout: format!("xtool auth login + sdk install succeeded for {email}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_single_quote_wraps_simple_string() {
        assert_eq!(shell_single_quote("hello"), "'hello'");
    }

    #[test]
    fn shell_single_quote_handles_empty_string() {
        assert_eq!(shell_single_quote(""), "''");
    }

    #[test]
    fn shell_single_quote_escapes_embedded_single_quote() {
        // POSIX trick: close the quote, escape the literal quote, reopen.
        assert_eq!(shell_single_quote("can't"), "'can'\\''t'");
    }

    #[test]
    fn shell_single_quote_handles_multiple_single_quotes() {
        assert_eq!(
            shell_single_quote("'a'b'c'"),
            "''\\''a'\\''b'\\''c'\\'''"
        );
    }

    #[test]
    fn shell_single_quote_preserves_special_chars_other_than_quote() {
        // Spaces, $, backticks, backslashes, ampersands — all literal inside
        // single quotes. Only ' itself needs escaping.
        let raw = r#"path with spaces & $vars `cmd` \n"#;
        let quoted = shell_single_quote(raw);
        assert_eq!(quoted, format!("'{raw}'"));
    }

    #[test]
    fn sanitize_stderr_replaces_password_substring() {
        let text = "auth failed: bad password 'hunter2'";
        assert_eq!(
            sanitize_stderr(text, "hunter2"),
            "auth failed: bad password '********'"
        );
    }

    #[test]
    fn sanitize_stderr_no_op_for_empty_password() {
        let text = "auth failed: bad password";
        assert_eq!(sanitize_stderr(text, ""), text);
    }

    #[test]
    fn sanitize_stderr_replaces_all_occurrences() {
        let text = "secret123 was rejected because secret123 expired";
        let scrubbed = sanitize_stderr(text, "secret123");
        assert!(!scrubbed.contains("secret123"));
        assert_eq!(scrubbed, "******** was rejected because ******** expired");
    }
}
