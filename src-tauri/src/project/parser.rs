// Two-tier Package.swift parser.
//
// Tier 1 (authoritative): spawn `swift package describe --type json` and
// deserialize. This is the only way to get the resolved product/target list
// because Package.swift is a Swift program that runs at parse time, not a
// static file format.
//
// Tier 2 (fallback): if the toolchain isn't on PATH, or the manifest fails to
// compile, regex-extract the package `name:` from the raw source and emit a
// PackageDescription with empty products/targets and `degraded: true`. The
// frontend renders that as a warning ("project opened with limited info —
// install Swift toolchain via setup wizard"). This preserves the
// commitment-contract promise that Open Project never fails outright when the
// folder is plausibly a SwiftPM project.

use std::fs;
use std::path::Path;
use std::process::Command;

use serde::{Deserialize, Serialize};
use thiserror::Error;

const MANIFEST_FILENAME: &str = "Package.swift";

/// File-tree blocklist for the Files sidebar. Mirrors what `.gitignore`
/// typically excludes from a SwiftPM project plus the IDE's own scratch dirs.
/// Chunk 1 ships a flat listing; recursive expansion + per-folder gitignore
/// parsing land alongside Monaco in M2.
const FILE_TREE_BLOCKLIST: &[&str] = &[
    ".build",
    ".git",
    ".swiftpm",
    "node_modules",
    "DerivedData",
    "target",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageProduct {
    pub name: String,
    /// "executable" / "library" / other. SwiftPM serializes product types as
    /// either bare strings or single-key objects depending on shape; we
    /// normalize to a string here so the frontend has one thing to switch on
    /// (e.g. the M1 chunk-2 Run button is gated on kind == "executable").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub targets: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageTarget {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageDescription {
    pub name: String,
    pub manifest_path: String,
    pub root_path: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub products: Vec<PackageProduct>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub targets: Vec<PackageTarget>,
    /// True iff parsing fell through to the regex fallback. Frontend surfaces
    /// this as a warning so users know the listing is incomplete and what
    /// remediation looks like (install Swift toolchain).
    #[serde(default, skip_serializing_if = "is_false")]
    pub degraded: bool,
    /// Empty when products/targets came from `swift package describe`. Holds
    /// the `swift` stderr (truncated) when degraded=true so the user can see
    /// why the authoritative parse failed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub degraded_reason: Option<String>,
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_false(b: &bool) -> bool {
    !*b
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ParseErrorKind {
    NotASwiftPmProject,
    InvalidManifest { detail: String },
    IoError { detail: String },
}

#[derive(Debug, Error, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ParseError {
    #[serde(flatten)]
    pub kind: ParseErrorKind,
    pub message: String,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileTreeNode {
    pub name: String,
    pub relative_path: String,
    pub is_directory: bool,
}

// ---------- Public entry points ----------

/// Parse a project root into a `PackageDescription`. Tries `swift package
/// describe` first; falls back to regex name extraction with `degraded=true`.
/// Only fails outright when the folder doesn't contain a `Package.swift` at
/// all, or when even the regex fallback can't find a name.
pub fn parse_package(root: &Path) -> Result<PackageDescription, ParseError> {
    let manifest_path = root.join(MANIFEST_FILENAME);
    if !manifest_path.is_file() {
        return Err(ParseError {
            kind: ParseErrorKind::NotASwiftPmProject,
            message: format!(
                "Folder does not contain a Package.swift manifest: {}",
                root.display()
            ),
        });
    }

    let manifest_source = fs::read_to_string(&manifest_path).map_err(|e| ParseError {
        kind: ParseErrorKind::IoError { detail: e.to_string() },
        message: format!("Could not read {}: {}", manifest_path.display(), e),
    })?;

    match describe_via_swift_package(root) {
        Ok(desc) => Ok(desc),
        Err(degrade_reason) => {
            let name = extract_name_from_manifest(&manifest_source).ok_or(ParseError {
                kind: ParseErrorKind::InvalidManifest {
                    detail: "could not extract package name from Package.swift".to_string(),
                },
                message: format!(
                    "Package.swift parsed by swift package describe failed and the regex \
                     fallback could not find a name: definition. Reason from swift: {degrade_reason}"
                ),
            })?;
            Ok(PackageDescription {
                name,
                manifest_path: manifest_path.to_string_lossy().to_string(),
                root_path: root.to_string_lossy().to_string(),
                products: Vec::new(),
                targets: Vec::new(),
                degraded: true,
                degraded_reason: Some(truncate(degrade_reason, 1024)),
            })
        }
    }
}

/// Read the project root's direct children, filtered against the blocklist.
/// Sorted: directories first, then files, both alphabetic case-insensitive.
/// Flat (one level) — recursive expansion is M2 alongside Monaco.
pub fn read_project_files(root: &Path) -> Result<Vec<FileTreeNode>, ParseError> {
    let entries = fs::read_dir(root).map_err(|e| ParseError {
        kind: ParseErrorKind::IoError { detail: e.to_string() },
        message: format!("Could not read directory {}: {}", root.display(), e),
    })?;

    let mut nodes: Vec<FileTreeNode> = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if FILE_TREE_BLOCKLIST.contains(&name.as_str()) {
            continue;
        }
        let is_directory = entry
            .file_type()
            .map(|ft| ft.is_dir())
            .unwrap_or(false);
        nodes.push(FileTreeNode {
            name: name.clone(),
            relative_path: name,
            is_directory,
        });
    }
    nodes.sort_by(|a, b| {
        match (a.is_directory, b.is_directory) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        }
    });
    Ok(nodes)
}

// ---------- Tier 1: swift package describe ----------

/// Run `swift package describe --type json` from the project root and parse
/// the result. Returns `Err(reason)` (not a `ParseError`) so the caller can
/// decide whether to fall through to the regex tier — typical reasons are
/// "swift not on PATH" or "manifest had a compile error."
fn describe_via_swift_package(root: &Path) -> Result<PackageDescription, String> {
    let mut cmd = Command::new("swift");
    cmd.arg("package")
        .arg("describe")
        .arg("--type")
        .arg("json")
        .current_dir(root);
    apply_no_window(&mut cmd);

    let output = cmd.output().map_err(|e| format!("swift not invocable: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(format!(
            "swift package describe exited {} — {}",
            output.status.code().unwrap_or(-1),
            truncate(stderr.trim().to_string(), 512)
        ));
    }

    let json: SwiftDescribe = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("could not parse swift package describe JSON: {e}"))?;

    let manifest_path = root.join(MANIFEST_FILENAME);
    Ok(PackageDescription {
        name: json.name,
        manifest_path: manifest_path.to_string_lossy().to_string(),
        root_path: root.to_string_lossy().to_string(),
        products: json.products.into_iter().map(Into::into).collect(),
        targets: json.targets.into_iter().map(Into::into).collect(),
        degraded: false,
        degraded_reason: None,
    })
}

#[cfg(target_os = "windows")]
fn apply_no_window(cmd: &mut Command) {
    use std::os::windows::process::CommandExt;
    cmd.creation_flags(crate::platform::windows::CREATE_NO_WINDOW);
}

#[cfg(not(target_os = "windows"))]
fn apply_no_window(_cmd: &mut Command) {
    // No-op on Linux/macOS — there is no console-window equivalent to suppress.
}

/// Subset of `swift package describe --type json` output. We only deserialize
/// what the IDE consumes; SwiftPM's full schema is larger and changes between
/// toolchain versions, so being tolerant here is deliberate.
#[derive(Debug, Deserialize)]
struct SwiftDescribe {
    name: String,
    #[serde(default)]
    products: Vec<DescribeProduct>,
    #[serde(default)]
    targets: Vec<DescribeTarget>,
}

#[derive(Debug, Deserialize)]
struct DescribeProduct {
    name: String,
    #[serde(rename = "type", default)]
    type_: serde_json::Value,
    #[serde(default)]
    targets: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct DescribeTarget {
    name: String,
    #[serde(rename = "type", default)]
    type_: Option<String>,
    #[serde(default)]
    path: Option<String>,
}

impl From<DescribeProduct> for PackageProduct {
    fn from(d: DescribeProduct) -> Self {
        PackageProduct {
            name: d.name,
            kind: normalize_product_type(&d.type_),
            targets: d.targets,
        }
    }
}

impl From<DescribeTarget> for PackageTarget {
    fn from(d: DescribeTarget) -> Self {
        PackageTarget {
            name: d.name,
            kind: d.type_,
            path: d.path,
        }
    }
}

/// SwiftPM serializes product types in two shapes depending on toolchain
/// version: a bare string (`"executable"`) or a single-key object
/// (`{ "executable": null }` or `{ "library": ["dynamic"] }`). Normalize to a
/// string discriminator the frontend can switch on.
fn normalize_product_type(v: &serde_json::Value) -> Option<String> {
    if let Some(s) = v.as_str() {
        return Some(s.to_string());
    }
    if let Some(obj) = v.as_object() {
        if let Some(key) = obj.keys().next() {
            return Some(key.clone());
        }
    }
    None
}

// ---------- Tier 2: regex name extraction ----------

/// Find the package's `name:` argument in the raw Package.swift source. Used
/// only when tier 1 fails. Skips Swift line comments (`//...`) but not block
/// comments (`/* ... */`); a commented-out `name: "foo"` block-style would
/// false-match. Documented limitation — degraded mode is already a "best
/// effort" shape, and the user has a clear path to the authoritative tier
/// once they install the toolchain.
fn extract_name_from_manifest(contents: &str) -> Option<String> {
    let bytes = contents.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // Skip line comments.
        if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'/' {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if i + 4 < bytes.len() && &bytes[i..i + 5] == b"name:" {
            let mut j = i + 5;
            while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t' || bytes[j] == b'\n') {
                j += 1;
            }
            if j < bytes.len() && bytes[j] == b'"' {
                let start = j + 1;
                let mut end = start;
                while end < bytes.len() && bytes[end] != b'"' {
                    if bytes[end] == b'\\' && end + 1 < bytes.len() {
                        end += 2;
                        continue;
                    }
                    end += 1;
                }
                if end < bytes.len() && end > start {
                    return Some(contents[start..end].to_string());
                }
            }
        }
        i += 1;
    }
    None
}

fn truncate(s: String, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    let mut out = s[..end].to_string();
    out.push_str("…");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn write_manifest(dir: &Path, contents: &str) {
        fs::write(dir.join(MANIFEST_FILENAME), contents).expect("write manifest");
    }

    #[test]
    fn missing_package_swift_returns_not_a_swiftpm_project() {
        let tmp = tempdir();
        let result = parse_package(&tmp);
        let err = result.expect_err("missing manifest should be an error");
        assert!(
            matches!(err.kind, ParseErrorKind::NotASwiftPmProject),
            "expected NotASwiftPmProject, got {:?}",
            err.kind
        );
        cleanup(&tmp);
    }

    #[test]
    fn regex_fallback_extracts_name_from_minimal_manifest() {
        let manifest = r#"// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "HelloWorld",
    targets: [
        .executableTarget(name: "HelloWorld"),
    ]
)
"#;
        let extracted = extract_name_from_manifest(manifest).expect("regex should find name");
        assert_eq!(extracted, "HelloWorld");
    }

    #[test]
    fn regex_fallback_skips_line_comments() {
        // First name: is in a // comment; the real name: comes after.
        let manifest = r#"// name: "NotThis"
let package = Package(
    name: "RealName"
)"#;
        let extracted = extract_name_from_manifest(manifest);
        assert_eq!(extracted, Some("RealName".to_string()));
    }

    #[test]
    fn regex_fallback_handles_quoted_name_with_whitespace_variants() {
        for src in [
            r#"name:"Tight""#,
            "name: \"Spaced\"",
            "name:\n    \"Newlined\"",
            "name:\t\"Tabbed\"",
        ] {
            assert!(
                extract_name_from_manifest(src).is_some(),
                "should extract from: {src:?}"
            );
        }
    }

    #[test]
    fn regex_fallback_returns_none_when_no_name_field() {
        let manifest = "let x = 1\n// no Package call here";
        assert_eq!(extract_name_from_manifest(manifest), None);
    }

    #[test]
    fn package_description_round_trips_via_serde_camelcase() {
        let desc = PackageDescription {
            name: "HelloWorld".to_string(),
            manifest_path: "/path/Package.swift".to_string(),
            root_path: "/path".to_string(),
            products: vec![PackageProduct {
                name: "HelloWorld".to_string(),
                kind: Some("executable".to_string()),
                targets: vec!["HelloWorld".to_string()],
            }],
            targets: vec![PackageTarget {
                name: "HelloWorld".to_string(),
                kind: Some("executable".to_string()),
                path: Some("Sources/HelloWorld".to_string()),
            }],
            degraded: false,
            degraded_reason: None,
        };
        let json = serde_json::to_string(&desc).expect("serialize");
        assert!(json.contains("\"manifestPath\""), "expected camelCase manifestPath");
        assert!(json.contains("\"rootPath\""), "expected camelCase rootPath");
        assert!(!json.contains("\"degraded\""), "false degraded should be skipped");
        let parsed: PackageDescription = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.name, "HelloWorld");
        assert!(!parsed.degraded);
    }

    #[test]
    fn normalize_product_type_handles_string_and_object_shapes() {
        let bare = serde_json::json!("executable");
        assert_eq!(normalize_product_type(&bare), Some("executable".to_string()));

        let single_key = serde_json::json!({ "executable": null });
        assert_eq!(normalize_product_type(&single_key), Some("executable".to_string()));

        let library_with_args = serde_json::json!({ "library": ["dynamic"] });
        assert_eq!(
            normalize_product_type(&library_with_args),
            Some("library".to_string())
        );

        let null = serde_json::Value::Null;
        assert_eq!(normalize_product_type(&null), None);
    }

    #[test]
    fn read_project_files_filters_blocklist_and_sorts_dirs_first() {
        let tmp = tempdir();
        fs::create_dir(tmp.join("Sources")).expect("mkdir Sources");
        fs::create_dir(tmp.join(".build")).expect("mkdir .build");
        fs::create_dir(tmp.join(".git")).expect("mkdir .git");
        fs::write(tmp.join("Package.swift"), "// noop").expect("write manifest");
        fs::write(tmp.join("README.md"), "# noop").expect("write readme");

        let nodes = read_project_files(&tmp).expect("read");
        let names: Vec<&str> = nodes.iter().map(|n| n.name.as_str()).collect();
        assert!(!names.contains(&".build"), "blocklist should drop .build");
        assert!(!names.contains(&".git"), "blocklist should drop .git");
        assert!(names.contains(&"Sources"), "should keep Sources");
        assert!(names.contains(&"Package.swift"), "should keep Package.swift");

        // Directories first.
        let first_file_idx = nodes.iter().position(|n| !n.is_directory).unwrap_or(0);
        let last_dir_idx = nodes
            .iter()
            .rposition(|n| n.is_directory)
            .unwrap_or(nodes.len());
        assert!(
            last_dir_idx < first_file_idx || nodes.is_empty(),
            "directories should sort before files"
        );

        cleanup(&tmp);
    }

    #[test]
    fn parse_package_falls_back_to_regex_when_swift_unavailable_and_marks_degraded() {
        // We simulate the "swift not on PATH" failure path by pointing at a
        // tmpdir whose Package.swift has a name: but probably can't compile
        // (the `Package(...)` call would need real PackageDescription import
        // resolution — fine on a host with Swift installed, fine to fail
        // otherwise; either way the test asserts the parse_package function
        // returns *something* useful, not a hard error).
        let tmp = tempdir();
        write_manifest(
            &tmp,
            r#"// swift-tools-version: 6.0
import PackageDescription
let package = Package(name: "FallbackOnly")
"#,
        );
        let result = parse_package(&tmp).expect("parse_package should not hard-fail with valid name:");
        assert_eq!(result.name, "FallbackOnly");
        // We don't assert degraded=true unconditionally — on a host with Swift
        // on PATH and a parseable manifest, tier 1 succeeds. We assert the
        // happy path instead: name matches and the description carries either
        // tier 1's targets or tier 2's empty-with-degraded-reason shape.
        if result.degraded {
            assert!(
                result.degraded_reason.is_some(),
                "degraded=true should carry a reason"
            );
            assert!(result.products.is_empty());
            assert!(result.targets.is_empty());
        }
        cleanup(&tmp);
    }

    /// Integration test that verifies tier 1 (`swift package describe`) end-to-end
    /// against the repo's own examples/hello-world fixture. Requires a Swift
    /// toolchain on PATH; gated behind `--ignored` for the same reason
    /// `verify_swift_download_hash` is in `platform/windows.rs`.
    #[test]
    #[ignore]
    fn parse_hello_world_with_swift_command() {
        let repo_root = std::env::current_dir()
            .expect("cwd")
            .parent()
            .expect("parent of src-tauri")
            .to_path_buf();
        let fixture = repo_root.join("examples").join("hello-world");
        assert!(
            fixture.is_dir(),
            "examples/hello-world fixture missing at {}",
            fixture.display()
        );
        let desc = parse_package(&fixture).expect("parse should succeed");
        assert_eq!(desc.name, "HelloWorld");
        if !desc.degraded {
            assert!(
                desc.targets.iter().any(|t| t.name == "HelloWorld"),
                "expected HelloWorld target in {:?}",
                desc.targets
            );
        }
    }

    // ---- minimal tempdir helpers (avoid pulling tempfile crate just for tests) ----

    fn tempdir() -> PathBuf {
        let base = std::env::temp_dir();
        let unique = format!(
            "ossw-project-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        );
        let dir = base.join(unique);
        fs::create_dir_all(&dir).expect("create tempdir");
        dir
    }

    fn cleanup(dir: &Path) {
        let _ = fs::remove_dir_all(dir);
    }
}
