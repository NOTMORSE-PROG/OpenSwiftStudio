import { createSignal } from "solid-js";
import type { BuildConfig, FileTreeNode, PackageDescription } from "../lib/projectApi";

export const [currentProject, setCurrentProject] = createSignal<PackageDescription | null>(null);
export const [projectFiles, setProjectFiles] = createSignal<FileTreeNode[]>([]);
export const [projectOpenInProgress, setProjectOpenInProgress] = createSignal(false);
export const [projectOpenError, setProjectOpenError] = createSignal<string | null>(null);

/// Atomic-ish reset for use by the Close path and error rollbacks.
export const clearProject = () => {
  setCurrentProject(null);
  setProjectFiles([]);
  setProjectOpenError(null);
  setHasExecutable(false);
};

// ---------- Run state (M1-5 / M1-13) ----------

/// Active build configuration for the open project. In-memory for now;
/// cross-restart persistence rides M1-7's session.json in chunk 3.
export const [runConfig, setRunConfig] = createSignal<BuildConfig>("debug");

/// idle → building → running → idle. Anything non-idle means a run is in
/// flight and the Run control shows Stop; the config toggle is disabled.
export type RunStatus = "idle" | "building" | "running";
export const [runStatus, setRunStatus] = createSignal<RunStatus>("idle");
export const isRunActive = (): boolean => runStatus() !== "idle";

/// Whether the open project exposes an executable product/target. Gates the Run
/// button (mirrors the backend's `executable_name` check). Set on project open.
export const [hasExecutable, setHasExecutable] = createSignal(false);

// ---------- Console ring buffer (M1-6) ----------

export type ConsoleStream = "stdout" | "stderr" | "meta";
export type ConsoleLine = { seq: number; stream: ConsoleStream; text: string };

/// Cap the buffer so a runaway process can't grow the DOM unbounded; oldest
/// lines drop first. 2000 keeps the newest output while staying light for a
/// plain <For> (virtualization deferred to M5's console rework).
export const CONSOLE_LINE_CAP = 2000;

export const [consoleLines, setConsoleLines] = createSignal<ConsoleLine[]>([]);

let seqCounter = 0;

export const appendConsoleLines = (stream: ConsoleStream, texts: string[]) => {
  if (texts.length === 0) return;
  const additions = texts.map((text) => ({ seq: seqCounter++, stream, text }));
  setConsoleLines((prev) => {
    const next = prev.concat(additions);
    return next.length > CONSOLE_LINE_CAP ? next.slice(next.length - CONSOLE_LINE_CAP) : next;
  });
};

export const appendConsoleMeta = (text: string) => appendConsoleLines("meta", [text]);

export const clearConsole = () => setConsoleLines([]);
