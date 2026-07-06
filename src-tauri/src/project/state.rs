// In-memory currently-open project state.
//
// Disk persistence (session.json — last project, open tabs, breakpoints) is
// M1-7 in chunk 3. Chunk 1 only needs the IDE to know which project is open
// for the duration of the running process, which a Mutex<Option<_>> wrapped
// in tauri::State<> handles cleanly.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::parser::PackageDescription;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectState {
    pub package: PackageDescription,
    pub opened_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::parser::PackageDescription;

    fn sample_package() -> PackageDescription {
        PackageDescription {
            name: "HelloWorld".to_string(),
            manifest_path: "/x/Package.swift".to_string(),
            root_path: "/x".to_string(),
            products: vec![],
            targets: vec![],
            degraded: false,
            degraded_reason: None,
        }
    }

    #[test]
    fn project_state_round_trips_via_serde_camelcase() {
        let state = ProjectState {
            package: sample_package(),
            opened_at: Utc::now(),
        };
        let json = serde_json::to_string(&state).expect("serialize");
        assert!(json.contains("\"openedAt\""), "expected camelCase openedAt");
        assert!(json.contains("\"package\""), "expected package field");
        let parsed: ProjectState = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.package.name, "HelloWorld");
    }
}
