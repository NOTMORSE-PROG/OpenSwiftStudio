// Project model.
//
// `parser` reads a SwiftPM project's manifest via a two-tier fallback chain:
// authoritative `swift package describe --type json`, with a regex extraction
// fallback when the toolchain isn't on PATH or the manifest fails to compile.
// `state` holds the currently-open project in memory; disk persistence
// (session.json — last project, open tabs, etc.) lands in M1 chunk 3.

pub mod parser;
pub mod run;
pub mod session;
pub mod state;
pub mod watcher;

pub use parser::{FileTreeNode, PackageDescription, parse_package, read_project_files};
pub use run::RunState;
pub use state::ProjectState;
pub use watcher::{ManifestWatch, watch_manifest};
