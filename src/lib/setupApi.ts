import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import { open as shellOpen } from "@tauri-apps/plugin-shell";

export type SetupCheckResult = {
  found: boolean;
  displayName?: string;
  version?: string;
  installPath?: string;
  message?: string;
};

export type StepRecord = {
  completed: boolean;
  skipped: boolean;
  reason?: string;
};

export type DetectionRecord = {
  found: boolean;
  displayName?: string;
  version?: string;
  installPath?: string;
  detectedAt: string;
};

// Aliases keep the original VsBuildToolsRecord name while letting all
// detection records share one shape on the wire (see src-tauri/src/setup/state.rs).
export type VsBuildToolsRecord = DetectionRecord;
export type Wsl2Record = DetectionRecord;
export type UsbipdRecord = DetectionRecord;
export type SwiftRecord = DetectionRecord;

export type SetupState = {
  schemaVersion: number;
  completedAt?: string;
  appVersion: string;
  steps: Record<string, StepRecord>;
  vsBuildToolsDetected?: VsBuildToolsRecord;
  wsl2Detected?: Wsl2Record;
  usbipdDetected?: UsbipdRecord;
  swiftDetected?: SwiftRecord;
};

export const getSetupState = (): Promise<SetupState | null> =>
  invoke<SetupState | null>("setup_get_state");

export const markSetupComplete = (state: SetupState): Promise<void> =>
  invoke<void>("setup_mark_complete", { state });

export const resetSetup = (): Promise<void> => invoke<void>("setup_reset");

export const checkVsBuildTools = (): Promise<SetupCheckResult> =>
  invoke<SetupCheckResult>("setup_check_vs_build_tools");

export const checkWsl2 = (): Promise<SetupCheckResult> =>
  invoke<SetupCheckResult>("setup_check_wsl2");

export const checkUsbipd = (): Promise<SetupCheckResult> =>
  invoke<SetupCheckResult>("setup_check_usbipd");

export const checkToolchain = (): Promise<SetupCheckResult> =>
  invoke<SetupCheckResult>("setup_check_toolchain");

export const openExternal = (url: string): Promise<void> => shellOpen(url);

// ---------- Installs ----------

export type InstallOutcome =
  | { kind: "success"; stdout: string }
  | { kind: "rebootRequired"; stdout: string }
  | { kind: "failed"; exitCode: number; stderr: string };

export type InstallId = "wsl2" | "usbipd" | "toolchain";

export type ProgressPhase = "download" | "verify" | "install";

export type InstallProgressPayload =
  | { id: InstallId; kind: "line"; line: string }
  | {
      id: InstallId;
      kind: "progress";
      phase: ProgressPhase;
      received: number;
      total: number;
    };

const INSTALL_PROGRESS_EVENT = "setup-install-progress";

export const installWsl2 = (): Promise<InstallOutcome> =>
  invoke<InstallOutcome>("setup_install_wsl2");

export const installUsbipd = (): Promise<InstallOutcome> =>
  invoke<InstallOutcome>("setup_install_usbipd");

export const installToolchain = (): Promise<InstallOutcome> =>
  invoke<InstallOutcome>("setup_install_toolchain");

/**
 * Subscribe to streaming install-progress events. The handler fires for every
 * subprocess line and every progress chunk; consumers narrow on `payload.kind`.
 * Returns an unlisten fn; call it from `onCleanup` to detach.
 */
export const onInstallProgress = async (
  handler: (payload: InstallProgressPayload) => void,
): Promise<UnlistenFn> =>
  listen<InstallProgressPayload>(INSTALL_PROGRESS_EVENT, (event) => {
    handler(event.payload);
  });
