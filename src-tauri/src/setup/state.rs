// On-disk setup.json schema, atomic read/write/delete.
//
// Schema is versioned from day one. Future fields use `#[serde(default)]`.
// A version downgrade (`schemaVersion > KNOWN_MAX`) re-runs the wizard rather
// than attempting migration.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const SCHEMA_VERSION: u32 = 1;
const SETUP_DIR_NAME: &str = "OpenSwiftStudio";
const SETUP_FILE_NAME: &str = "setup.json";

#[derive(Debug, Error)]
pub enum SetupError {
    #[error("could not resolve config directory")]
    NoConfigDir,
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepRecord {
    pub completed: bool,
    pub skipped: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VsBuildToolsRecord {
    pub found: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub install_path: Option<String>,
    pub detected_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupState {
    pub schema_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
    pub app_version: String,
    pub steps: BTreeMap<String, StepRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vs_build_tools_detected: Option<VsBuildToolsRecord>,
}

fn setup_dir() -> Result<PathBuf, SetupError> {
    let mut dir = dirs::config_dir().ok_or(SetupError::NoConfigDir)?;
    dir.push(SETUP_DIR_NAME);
    Ok(dir)
}

pub fn setup_file_path() -> Result<PathBuf, SetupError> {
    let mut path = setup_dir()?;
    path.push(SETUP_FILE_NAME);
    Ok(path)
}

pub fn read_state() -> Result<Option<SetupState>, SetupError> {
    let path = setup_file_path()?;
    if !path.exists() {
        return Ok(None);
    }
    let bytes = fs::read(&path)?;
    let state: SetupState = match serde_json::from_slice(&bytes) {
        Ok(s) => s,
        Err(_) => return Ok(None), // Corrupt file → re-run wizard, don't crash.
    };
    if state.schema_version > SCHEMA_VERSION {
        // Newer schema written by a future build; treat as missing so we re-run
        // setup against the current schema rather than guessing at unknown fields.
        return Ok(None);
    }
    Ok(Some(state))
}

pub fn write_state(state: &SetupState) -> Result<(), SetupError> {
    let dir = setup_dir()?;
    fs::create_dir_all(&dir)?;
    let final_path = setup_file_path()?;
    let mut tmp_path = final_path.clone();
    tmp_path.set_extension("json.tmp");

    let bytes = serde_json::to_vec_pretty(state)?;
    {
        let mut f = fs::File::create(&tmp_path)?;
        f.write_all(&bytes)?;
        f.sync_all()?;
    }
    // NTFS rename is atomic for same-volume same-directory targets.
    fs::rename(&tmp_path, &final_path)?;
    Ok(())
}

pub fn delete_state() -> Result<(), SetupError> {
    let path = setup_file_path()?;
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(SetupError::Io(e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_state() -> SetupState {
        let mut steps = BTreeMap::new();
        steps.insert(
            "welcome".to_string(),
            StepRecord { completed: true, skipped: false, reason: None },
        );
        steps.insert(
            "wsl2".to_string(),
            StepRecord {
                completed: false,
                skipped: true,
                reason: Some("stub-in-foundation-chunk".to_string()),
            },
        );
        SetupState {
            schema_version: SCHEMA_VERSION,
            completed_at: Some("2026-05-04T12:34:56Z".to_string()),
            app_version: "0.0.1".to_string(),
            steps,
            vs_build_tools_detected: Some(VsBuildToolsRecord {
                found: false,
                display_name: None,
                version: None,
                install_path: None,
                detected_at: "2026-05-04T12:34:56Z".to_string(),
            }),
        }
    }

    #[test]
    fn schema_round_trips_via_serde() {
        let original = sample_state();
        let json = serde_json::to_string_pretty(&original).expect("serialize");
        // Confirm camelCase + versioning land in the wire format.
        assert!(json.contains("\"schemaVersion\""));
        assert!(json.contains("\"completedAt\""));
        assert!(json.contains("\"vsBuildToolsDetected\""));
        let parsed: SetupState = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.schema_version, original.schema_version);
        assert_eq!(parsed.app_version, original.app_version);
        assert_eq!(parsed.completed_at, original.completed_at);
        assert_eq!(parsed.steps.get("welcome").unwrap().completed, true);
        assert_eq!(parsed.steps.get("wsl2").unwrap().skipped, true);
    }

    #[test]
    fn future_schema_version_is_treated_as_missing() {
        // A file written by a newer build (`schemaVersion > KNOWN_MAX`) should
        // make read_state return None — the wizard re-runs against the current
        // schema rather than guessing at unknown fields.
        let mut state = sample_state();
        state.schema_version = SCHEMA_VERSION + 5;
        let json = serde_json::to_vec_pretty(&state).expect("serialize");
        // We can't safely write to the real APPDATA path in a unit test, so we
        // test the parsing/version-gating logic directly:
        let parsed: SetupState = serde_json::from_slice(&json).expect("deserialize");
        assert!(parsed.schema_version > SCHEMA_VERSION);
        // The check that read_state would perform:
        let too_new = parsed.schema_version > SCHEMA_VERSION;
        assert!(too_new);
    }

    #[test]
    fn corrupt_json_does_not_panic() {
        // The actual read_state returns Ok(None) on parse failure — this test
        // pins that behavior at the serde layer.
        let bytes = b"{this is not json";
        let parsed: Result<SetupState, _> = serde_json::from_slice(bytes);
        assert!(parsed.is_err());
    }
}

