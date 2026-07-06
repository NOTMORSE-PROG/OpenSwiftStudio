import { createSignal } from "solid-js";
import type { FileTreeNode, PackageDescription } from "../lib/projectApi";

export const [currentProject, setCurrentProject] = createSignal<PackageDescription | null>(null);
export const [projectFiles, setProjectFiles] = createSignal<FileTreeNode[]>([]);
export const [projectOpenInProgress, setProjectOpenInProgress] = createSignal(false);
export const [projectOpenError, setProjectOpenError] = createSignal<string | null>(null);

/// Atomic-ish reset for use by the Close path and error rollbacks.
export const clearProject = () => {
  setCurrentProject(null);
  setProjectFiles([]);
  setProjectOpenError(null);
};
