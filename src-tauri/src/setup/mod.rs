// M0.5 setup wizard backend.
//
// `state` owns the on-disk %APPDATA%/OpenSwiftStudio/setup.json contract.
// `checks` is the dispatch surface for prerequisite probes; bodies live in
// `platform/<os>.rs`.

pub mod state;
pub mod checks;
pub mod installs;
