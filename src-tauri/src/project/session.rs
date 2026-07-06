// On-disk IDE session state: %APPDATA%\OpenSwiftStudio\session.json.
//
// Mirrors setup/state.rs: versioned schema, atomic tempfile + rename write,
// corrupt/future-version files degrade to None (launch clean, never crash).
// Persistence is eager (saved on each session-relevant change) rather than
// save-on-quit, so a crash still leaves the last saved state. The file-watcher
// half of M1-7 (live Package.swift re-parse) is tracked separately as M1-15.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const SESSION_SCHEMA_VERSION: u32 = 1;
const APP_DIR_NAME: &str = "OpenSwiftStudio";
const SESSION_FILE_NAME: &str = "session.json";

#[derive(Debug, Error)]
pub enum SessionError {
    #[error("could not resolve config directory")]
    NoConfigDir,
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}

/// Restorable IDE session. Every field past `schemaVersion` is optional / has a
/// serde default so a session.json written by an older build still loads after
/// the schema grows.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionState {
    pub schema_version: u32,
    /// Absolute path of the last-opened project root, restored on launch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_project_path: Option<String>,
    /// "debug" | "release" — the active build configuration (M1-13).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build_config: Option<String>,
    /// Sidebar activity view selection ("files" | "search" | ...).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_view: Option<String>,
    /// Forward-compat slot for open editor tabs (populated when M2-5 lands).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub open_files: Vec<String>,
}

impl Default for SessionState {
    fn default() -> Self {
        SessionState {
            schema_version: SESSION_SCHEMA_VERSION,
            last_project_path: None,
            build_config: None,
            active_view: None,
            open_files: Vec::new(),
        }
    }
}

fn app_dir() -> Result<PathBuf, SessionError> {
    let mut dir = dirs::config_dir().ok_or(SessionError::NoConfigDir)?;
    dir.push(APP_DIR_NAME);
    Ok(dir)
}

pub fn session_file_path() -> Result<PathBuf, SessionError> {
    Ok(app_dir()?.join(SESSION_FILE_NAME))
}

/// Load the session from `%APPDATA%`. Returns `Ok(None)` when the file is
/// absent, unparseable (corrupt), or written by a newer schema — the caller
/// launches to the welcome view rather than crashing.
pub fn read_session() -> Result<Option<SessionState>, SessionError> {
    read_session_from(&session_file_path()?)
}

/// Save the session atomically to `%APPDATA%` (tempfile + rename).
pub fn write_session(state: &SessionState) -> Result<(), SessionError> {
    let dir = app_dir()?;
    fs::create_dir_all(&dir)?;
    write_session_at(&dir, state)
}

/// Delete the session file. Missing file is not an error.
pub fn clear_session() -> Result<(), SessionError> {
    match fs::remove_file(session_file_path()?) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(SessionError::Io(e)),
    }
}

// ---- Path-injectable cores (so tests exercise real atomic write/read against
// a tempdir without touching the real %APPDATA%) ----

fn read_session_from(path: &Path) -> Result<Option<SessionState>, SessionError> {
    if !path.exists() {
        return Ok(None);
    }
    let bytes = fs::read(path)?;
    let state: SessionState = match serde_json::from_slice(&bytes) {
        Ok(s) => s,
        Err(_) => return Ok(None), // Corrupt -> launch clean.
    };
    if state.schema_version > SESSION_SCHEMA_VERSION {
        return Ok(None); // Newer build wrote it -> don't guess at unknown fields.
    }
    Ok(Some(state))
}

fn write_session_at(dir: &Path, state: &SessionState) -> Result<(), SessionError> {
    let final_path = dir.join(SESSION_FILE_NAME);
    let tmp_path = final_path.with_extension("json.tmp");
    let bytes = serde_json::to_vec_pretty(state)?;
    {
        let mut f = fs::File::create(&tmp_path)?;
        f.write_all(&bytes)?;
        f.sync_all()?;
    }
    fs::rename(&tmp_path, &final_path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> SessionState {
        SessionState {
            schema_version: SESSION_SCHEMA_VERSION,
            last_project_path: Some("C:\\code\\HelloWorld".to_string()),
            build_config: Some("release".to_string()),
            active_view: Some("files".to_string()),
            open_files: Vec::new(),
        }
    }

    fn tempdir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ossw-session-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&dir).expect("mkdir tempdir");
        dir
    }

    #[test]
    fn schema_serializes_camelcase_and_round_trips() {
        let json = serde_json::to_string_pretty(&sample()).expect("serialize");
        assert!(json.contains("\"schemaVersion\""));
        assert!(json.contains("\"lastProjectPath\""));
        assert!(json.contains("\"buildConfig\""));
        assert!(!json.contains("\"openFiles\""), "empty openFiles should be skipped");
        let parsed: SessionState = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed, sample());
    }

    #[test]
    fn atomic_write_then_read_round_trips_on_disk() {
        let dir = tempdir();
        write_session_at(&dir, &sample()).expect("write");
        let loaded = read_session_from(&dir.join(SESSION_FILE_NAME))
            .expect("read")
            .expect("some");
        assert_eq!(loaded, sample());
        // A .tmp must not be left behind after the rename.
        assert!(!dir.join("session.json.tmp").exists(), "temp file should be gone");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_file_reads_as_none() {
        let dir = tempdir();
        let got = read_session_from(&dir.join(SESSION_FILE_NAME)).expect("read");
        assert!(got.is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn corrupt_file_reads_as_none_not_error() {
        let dir = tempdir();
        fs::write(dir.join(SESSION_FILE_NAME), b"{ this is : not json").expect("write junk");
        let got = read_session_from(&dir.join(SESSION_FILE_NAME)).expect("must not error");
        assert!(got.is_none(), "corrupt session must degrade to None, not crash");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn future_schema_version_reads_as_none() {
        let dir = tempdir();
        let mut future = sample();
        future.schema_version = SESSION_SCHEMA_VERSION + 3;
        write_session_at(&dir, &future).expect("write");
        let got = read_session_from(&dir.join(SESSION_FILE_NAME)).expect("read");
        assert!(got.is_none(), "newer schema should be treated as missing");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn older_file_without_new_fields_still_loads() {
        // Simulate a session.json from a build that only had lastProjectPath.
        let legacy = r#"{ "schemaVersion": 1, "lastProjectPath": "C:\\p" }"#;
        let parsed: SessionState = serde_json::from_str(legacy).expect("legacy must parse");
        assert_eq!(parsed.last_project_path, Some("C:\\p".to_string()));
        assert_eq!(parsed.build_config, None);
        assert!(parsed.open_files.is_empty());
    }

    #[test]
    fn write_overwrites_existing_atomically() {
        let dir = tempdir();
        write_session_at(&dir, &sample()).expect("write 1");
        let mut updated = sample();
        updated.build_config = Some("debug".to_string());
        write_session_at(&dir, &updated).expect("write 2");
        let loaded = read_session_from(&dir.join(SESSION_FILE_NAME)).expect("read").expect("some");
        assert_eq!(loaded.build_config, Some("debug".to_string()));
        let _ = fs::remove_dir_all(&dir);
    }
}
