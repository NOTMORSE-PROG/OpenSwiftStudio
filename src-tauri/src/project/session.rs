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
    /// Most-recent-first list of opened project roots (M1-8), deduped
    /// case-insensitively and capped at `RECENT_PROJECTS_CAP`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recent_projects: Vec<String>,
}

/// Cap on the Recent Projects list (M1-8).
pub const RECENT_PROJECTS_CAP: usize = 10;

/// Return a new most-recent-first list with `path` promoted to the front,
/// any case-insensitive duplicate of it removed, and the result capped at
/// `cap`. Windows filesystems are case-insensitive, so `C:\Foo` and `c:\foo`
/// collapse to one entry (the newest spelling wins). Missing paths are NOT
/// pruned here — the UI marks stale entries and offers removal (M1-8 AC).
pub fn mru_push(existing: &[String], path: &str, cap: usize) -> Vec<String> {
    let key = path.to_lowercase();
    let mut out: Vec<String> = Vec::with_capacity(existing.len() + 1);
    out.push(path.to_string());
    for e in existing {
        if e.to_lowercase() != key {
            out.push(e.clone());
        }
    }
    out.truncate(cap);
    out
}

impl Default for SessionState {
    fn default() -> Self {
        SessionState {
            schema_version: SESSION_SCHEMA_VERSION,
            last_project_path: None,
            build_config: None,
            active_view: None,
            open_files: Vec::new(),
            recent_projects: Vec::new(),
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
            recent_projects: Vec::new(),
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
    fn recent_projects_survive_a_write_read_round_trip() {
        let dir = tempdir();
        let mut s = sample();
        s.recent_projects = vec!["C:\\a".to_string(), "C:\\b".to_string()];
        write_session_at(&dir, &s).expect("write");
        let loaded = read_session_from(&dir.join(SESSION_FILE_NAME)).expect("read").expect("some");
        assert_eq!(loaded.recent_projects, vec!["C:\\a".to_string(), "C:\\b".to_string()]);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn mru_push_promotes_to_front_and_dedupes_case_insensitively() {
        let existing = vec!["C:\\a".to_string(), "C:\\b".to_string()];
        // Re-open "a" (different case) -> moves to front, no duplicate.
        let out = mru_push(&existing, "c:\\A", RECENT_PROJECTS_CAP);
        assert_eq!(out, vec!["c:\\A".to_string(), "C:\\b".to_string()]);
    }

    #[test]
    fn mru_push_prepends_new_entry() {
        let existing = vec!["C:\\a".to_string()];
        let out = mru_push(&existing, "C:\\b", RECENT_PROJECTS_CAP);
        assert_eq!(out, vec!["C:\\b".to_string(), "C:\\a".to_string()]);
    }

    #[test]
    fn mru_push_caps_length_keeping_newest() {
        let existing: Vec<String> = (0..12).map(|i| format!("C:\\p{i}")).collect();
        let out = mru_push(&existing, "C:\\new", 10);
        assert_eq!(out.len(), 10);
        assert_eq!(out[0], "C:\\new");
        // The two oldest (p10, p11) drop off the end.
        assert!(!out.contains(&"C:\\p11".to_string()));
    }

    #[test]
    fn mru_push_does_not_prune_missing_paths() {
        // A path that doesn't exist on disk is still kept (UI marks stale, not us).
        let existing = vec!["C:\\this\\does\\not\\exist".to_string()];
        let out = mru_push(&existing, "C:\\also\\gone", 10);
        assert_eq!(out.len(), 2);
        assert!(out.contains(&"C:\\this\\does\\not\\exist".to_string()));
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
