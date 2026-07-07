// Shared project-open actions (M1-3 / M1-8). One code path opens a project by
// absolute path — used by the Open-Folder dialog, the Recent Projects list, and
// the on-launch session restore — so degraded-parse and error handling behave
// identically everywhere.

import { setActiveView } from "../state/appState";
import {
  clearProject,
  recentProjects,
  setCurrentProject,
  setHasExecutable,
  setProjectFiles,
  setProjectOpenError,
  setProjectOpenInProgress,
  setRecentProjects,
} from "../state/projectState";
import {
  getProjectFiles,
  hasExecutableProduct,
  mruPush,
  openProject,
} from "./projectApi";

/// Push `path` to the front of the recent list (backend dedupe/cap). The
/// updated list is persisted by the session autosave (single writer).
export const recordRecentProject = async (path: string) => {
  try {
    setRecentProjects(await mruPush(recentProjects(), path));
  } catch (err) {
    console.error("mru_push failed:", err);
  }
};

/// Drop a recent entry (e.g. its folder was deleted). Persisted via autosave.
export const removeRecentProject = (path: string) => {
  const key = path.toLowerCase();
  setRecentProjects(recentProjects().filter((p) => p.toLowerCase() !== key));
};

/// Open a SwiftPM project by absolute path. Returns true on success. On failure
/// sets `projectOpenError` and leaves no project open (welcome view). Records
/// the path into Recent Projects unless `recordRecent` is false (the restore
/// path skips it — the last project is already at the front).
export const openProjectByPath = async (
  path: string,
  opts?: { recordRecent?: boolean },
): Promise<boolean> => {
  setProjectOpenError(null);
  setProjectOpenInProgress(true);
  try {
    const meta = await openProject(path);
    const files = await getProjectFiles(path);
    setCurrentProject(meta);
    setProjectFiles(files);
    setHasExecutable(hasExecutableProduct(meta));
    setActiveView("files");
    if (opts?.recordRecent !== false) await recordRecentProject(path);
    return true;
  } catch (err) {
    console.error("project open failed:", err);
    setProjectOpenError(String(err));
    clearProject();
    return false;
  } finally {
    setProjectOpenInProgress(false);
  }
};
