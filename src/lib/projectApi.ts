import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { SetupCheckResult } from "./setupApi";

/// Mirrors src-tauri/src/project/parser.rs::PackageProduct.
export type PackageProduct = {
  name: string;
  /// "executable" / "library" / other. Frontend gates the Run button on
  /// kind === "executable" (lands in M1 chunk 2).
  kind?: string;
  targets?: string[];
};

/// Mirrors src-tauri/src/project/parser.rs::PackageTarget.
export type PackageTarget = {
  name: string;
  kind?: string;
  path?: string;
};

/// Mirrors src-tauri/src/project/parser.rs::PackageDescription. `degraded`
/// signals that the authoritative `swift package describe` parse failed and
/// the regex name extraction fallback was used — products/targets will be
/// empty and the UI shows a "limited info" warning.
export type PackageDescription = {
  name: string;
  manifestPath: string;
  rootPath: string;
  products?: PackageProduct[];
  targets?: PackageTarget[];
  degraded?: boolean;
  degradedReason?: string;
};

export type FileTreeNode = {
  name: string;
  relativePath: string;
  isDirectory: boolean;
};

/// True when the package exposes something runnable (an executable product or
/// target). Mirrors the backend's `run::executable_name` gate so the frontend
/// can enable/disable the Run button consistently.
export const hasExecutableProduct = (pkg: PackageDescription): boolean =>
  (pkg.products ?? []).some((p) => p.kind === "executable") ||
  (pkg.targets ?? []).some((t) => t.kind === "executable");

export const openProject = (path: string): Promise<PackageDescription> =>
  invoke<PackageDescription>("project_open", { path });

export const closeProject = (): Promise<void> => invoke<void>("project_close");

export const getProjectMeta = (): Promise<PackageDescription | null> =>
  invoke<PackageDescription | null>("project_get_meta");

export const getProjectFiles = (path: string): Promise<FileTreeNode[]> =>
  invoke<FileTreeNode[]>("project_get_files", { path });

export const getToolchain = (): Promise<SetupCheckResult> =>
  invoke<SetupCheckResult>("app_get_toolchain");

// ---------- Run (M1-5 / M1-6 / M1-13) ----------

/// Mirrors src-tauri/src/project/run.rs::BuildConfig (serde camelCase).
export type BuildConfig = "debug" | "release";

/// Mirrors src-tauri/src/project/run.rs::RunEvent. `output` carries a batch of
/// lines for one stream of one phase (per-stream ordering is guaranteed,
/// global interleaving is not); `exit` is terminal (exactly one per run).
export type RunEvent =
  | { kind: "status"; status: "building" | "running" }
  | {
      kind: "output";
      phase: "build" | "run";
      stream: "stdout" | "stderr";
      lines: string[];
    }
  | {
      kind: "exit";
      phase: "build" | "run";
      code: number;
      outcome: "exited" | "buildFailed" | "stopped" | "spawnError";
      message?: string;
    };

/** Build the open project under `config` and run its binary. Resolves as soon
 *  as the pipeline is spawned; progress arrives via `onRunProgress`. Rejects
 *  synchronously when no project is open, there is no executable product, or a
 *  run is already active. */
export const startRun = (config: BuildConfig): Promise<void> =>
  invoke<void>("run_start", { config });

/** Kill the active run's process tree. No-op when nothing is running. */
export const stopRun = (): Promise<void> => invoke<void>("run_stop");

// ---------- Session persistence (M1-7) ----------

/// Mirrors src-tauri/src/project/session.rs::SessionState.
export type SessionState = {
  schemaVersion: number;
  lastProjectPath?: string;
  buildConfig?: BuildConfig;
  activeView?: string;
  openFiles?: string[];
};

export const loadSession = (): Promise<SessionState | null> =>
  invoke<SessionState | null>("session_load");

export const saveSession = (state: SessionState): Promise<void> =>
  invoke<void>("session_save", { state });

export const clearSession = (): Promise<void> => invoke<void>("session_clear");

const PROJECT_RUN_EVENT = "project-run-progress";

/** Subscribe to run-progress events. Returns an unlisten fn; call it from
 *  onCleanup to detach. */
export const onRunProgress = async (
  handler: (event: RunEvent) => void,
): Promise<UnlistenFn> =>
  listen<RunEvent>(PROJECT_RUN_EVENT, (e) => handler(e.payload));
