// Session persistence wiring (M1-7). Restores the last-opened project + build
// config on launch, and eagerly saves a snapshot whenever session-relevant
// state changes (crash-safe: every change is already on disk, no save-on-quit
// hook needed). The Package.swift file-watcher half of M1-7 is tracked as M1-15.

import { createEffect, createSignal } from "solid-js";
import { activeView, setActiveView, type ActivityView } from "../state/appState";
import {
  currentProject,
  runConfig,
  setCurrentProject,
  setHasExecutable,
  setProjectFiles,
  setRunConfig,
} from "../state/projectState";
import {
  getProjectFiles,
  hasExecutableProduct,
  loadSession,
  openProject,
  saveSession,
  type SessionState,
} from "./projectApi";

// Must match src-tauri/src/project/session.rs::SESSION_SCHEMA_VERSION.
const SESSION_SCHEMA_VERSION = 1;

/// True once the on-launch restore has completed. The autosave effect stays
/// dormant until then so it can't clobber session.json with empty state before
/// the restore reads it.
export const [sessionRestored, setSessionRestored] = createSignal(false);

/// Non-blocking notice shown when the last-opened project can't be reopened
/// (folder deleted/moved/unparseable). The Sidebar renders it; the IDE falls
/// back to the welcome view.
export const [restoreNotice, setRestoreNotice] = createSignal<string | null>(null);

const isBuildConfig = (v: unknown): v is "debug" | "release" =>
  v === "debug" || v === "release";

const snapshot = (): SessionState => ({
  schemaVersion: SESSION_SCHEMA_VERSION,
  lastProjectPath: currentProject()?.rootPath,
  buildConfig: runConfig(),
  activeView: activeView(),
});

/// Load session.json and re-apply it. Runs once on launch.
export const restoreSession = async () => {
  try {
    const s = await loadSession();
    if (s) {
      if (isBuildConfig(s.buildConfig)) setRunConfig(s.buildConfig);
      if (s.activeView) setActiveView(s.activeView as ActivityView);
      if (s.lastProjectPath) {
        try {
          const meta = await openProject(s.lastProjectPath);
          const files = await getProjectFiles(s.lastProjectPath);
          setCurrentProject(meta);
          setProjectFiles(files);
          setHasExecutable(hasExecutableProduct(meta));
        } catch (err) {
          // Folder deleted/moved/unparseable since last session -> non-blocking
          // notice, stay on the welcome view. (project_open surfaced the error.)
          console.warn("could not reopen last project:", err);
          setRestoreNotice(`Could not reopen the last project at ${s.lastProjectPath}.`);
        }
      }
    }
  } catch (err) {
    console.error("session restore failed:", err);
  } finally {
    setSessionRestored(true);
  }
};

/// Install the reactive autosave. Call once during app setup. Tracks the
/// snapshot signals; saves after each change once restore has completed.
export const installSessionAutosave = () => {
  createEffect(() => {
    const snap = snapshot(); // tracks currentProject / runConfig / activeView
    if (!sessionRestored()) return;
    void saveSession(snap).catch((e) => console.error("session save failed:", e));
  });
};
