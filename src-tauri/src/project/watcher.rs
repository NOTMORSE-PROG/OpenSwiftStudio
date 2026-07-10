// Watches the open project's Package.swift for external edits (M1-15).
//
// Watches the project ROOT directory non-recursively rather than the manifest
// file itself: editors that save via temp-file + rename replace the inode,
// which a file-level watch can miss. A directory watch filtered on the file
// name catches modify, rename-replace, delete, and recreate alike. Debounced
// so a save burst (or a `git checkout` storming sibling paths) triggers at
// most one re-parse per window.

use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use notify_debouncer_mini::notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_mini::{new_debouncer, DebounceEventResult, DebouncedEventKind, Debouncer};

/// At most one re-parse per this window, however many raw filesystem events an
/// external save produces (Windows ReadDirectoryChangesW fires several per
/// save; the debouncer coalesces per path).
pub const MANIFEST_DEBOUNCE: Duration = Duration::from_millis(500);

/// Owns the watcher's background thread; dropping it stops the watch. One
/// lives at a time in the Tauri-managed [`ManifestWatch`] slot — `project_open`
/// replaces it, `project_close` clears it.
pub struct ManifestWatcher {
    _debouncer: Debouncer<RecommendedWatcher>,
}

/// Managed slot for the active project's manifest watcher.
pub type ManifestWatch = Mutex<Option<ManifestWatcher>>;

/// Start watching `root` (non-recursive) and invoke `on_change` after each
/// debounced batch that touches `Package.swift`. The callback runs on the
/// watcher's thread — keep it re-entrant-safe.
pub fn watch_manifest<F>(root: &Path, mut on_change: F) -> Result<ManifestWatcher, String>
where
    F: FnMut() + Send + 'static,
{
    let mut debouncer = new_debouncer(MANIFEST_DEBOUNCE, move |result: DebounceEventResult| {
        if let Ok(events) = result {
            // Only the final quiet-period event (`Any`) triggers a re-parse.
            // The debouncer also emits `AnyContinuous` progress markers while
            // activity is ongoing; reacting to those would double-fire on a
            // single save burst (one `Any` always follows once writes settle).
            if events
                .iter()
                .any(|e| e.kind == DebouncedEventKind::Any && is_manifest(&e.path))
            {
                on_change();
            }
        }
    })
    .map_err(|e| format!("could not create manifest watcher: {e}"))?;

    debouncer
        .watcher()
        .watch(root, RecursiveMode::NonRecursive)
        .map_err(|e| format!("could not watch {}: {e}", root.display()))?;

    Ok(ManifestWatcher {
        _debouncer: debouncer,
    })
}

/// Case-insensitive: Windows filesystems are, and SwiftPM accepts the manifest
/// however the filesystem reports its casing.
fn is_manifest(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.eq_ignore_ascii_case("package.swift"))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Instant;

    fn tempdir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ossw-watcher-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&dir).expect("mkdir tempdir");
        dir
    }

    fn counting_watch(root: &Path) -> (ManifestWatcher, Arc<AtomicUsize>) {
        let count = Arc::new(AtomicUsize::new(0));
        let c = count.clone();
        let watcher = watch_manifest(root, move || {
            c.fetch_add(1, Ordering::SeqCst);
        })
        .expect("watch_manifest");
        (watcher, count)
    }

    /// Poll until `count` reaches `expected` or `timeout` elapses.
    fn wait_for(count: &AtomicUsize, expected: usize, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if count.load(Ordering::SeqCst) >= expected {
                return true;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        false
    }

    #[test]
    fn is_manifest_matches_case_insensitively_and_rejects_others() {
        assert!(is_manifest(Path::new("C:\\proj\\Package.swift")));
        assert!(is_manifest(Path::new("C:\\proj\\package.SWIFT")));
        assert!(!is_manifest(Path::new("C:\\proj\\Package.resolved")));
        assert!(!is_manifest(Path::new("C:\\proj\\main.swift")));
        assert!(!is_manifest(Path::new("C:\\proj")));
    }

    #[test]
    fn rapid_manifest_edits_coalesce_to_exactly_one_reparse() {
        let dir = tempdir();
        let manifest = dir.join("Package.swift");
        fs::write(&manifest, "// v0").expect("seed manifest");
        let (_watcher, count) = counting_watch(&dir);

        // A burst of writes inside one debounce window: one callback expected.
        for i in 0..3 {
            fs::write(&manifest, format!("// edit {i}")).expect("edit manifest");
            std::thread::sleep(Duration::from_millis(30));
        }

        assert!(
            wait_for(&count, 1, Duration::from_secs(3)),
            "debounced callback never fired"
        );
        // Well past another full window: still exactly one.
        std::thread::sleep(MANIFEST_DEBOUNCE + Duration::from_millis(300));
        assert_eq!(count.load(Ordering::SeqCst), 1, "burst must coalesce to one");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn sibling_file_storm_fires_nothing() {
        let dir = tempdir();
        fs::write(dir.join("Package.swift"), "// stable").expect("seed manifest");
        let (_watcher, count) = counting_watch(&dir);

        // Simulate a checkout-style storm on non-manifest siblings.
        for i in 0..20 {
            fs::write(dir.join(format!("file-{i}.swift")), "x").expect("storm file");
        }

        std::thread::sleep(MANIFEST_DEBOUNCE + Duration::from_millis(700));
        assert_eq!(
            count.load(Ordering::SeqCst),
            0,
            "non-manifest events must not trigger a re-parse"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn manifest_delete_fires_a_reparse() {
        let dir = tempdir();
        let manifest = dir.join("Package.swift");
        fs::write(&manifest, "// doomed").expect("seed manifest");
        let (_watcher, count) = counting_watch(&dir);

        fs::remove_file(&manifest).expect("delete manifest");

        assert!(
            wait_for(&count, 1, Duration::from_secs(3)),
            "delete must surface as a manifest change"
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
