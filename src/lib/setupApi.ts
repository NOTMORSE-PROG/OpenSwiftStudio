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
export type XtoolRecord = DetectionRecord;

export type SetupState = {
  schemaVersion: number;
  completedAt?: string;
  appVersion: string;
  steps: Record<string, StepRecord>;
  vsBuildToolsDetected?: VsBuildToolsRecord;
  wsl2Detected?: Wsl2Record;
  usbipdDetected?: UsbipdRecord;
  swiftDetected?: SwiftRecord;
  xtoolDetected?: XtoolRecord;
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

export const checkXtool = (): Promise<SetupCheckResult> =>
  invoke<SetupCheckResult>("setup_check_xtool");

export const openExternal = (url: string): Promise<void> => shellOpen(url);

// ---------- Installs ----------

export type InstallOutcome =
  | { kind: "success"; stdout: string }
  | { kind: "rebootRequired"; stdout: string }
  | { kind: "failed"; exitCode: number; stderr: string };

export type InstallId =
  | "wsl2"
  | "usbipd"
  | "toolchain"
  | "xtool"
  | "xtool-setup";

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

export const installXtool = (): Promise<InstallOutcome> =>
  invoke<InstallOutcome>("setup_install_xtool");

// ---------- xtool auth login + sdk install (Apple ID step) ----------

/**
 * Drive `xtool auth login` then `xtool sdk install` against the user's WSL2
 * distro. `xipPath` is a Windows path to a local Xcode .xip; the Rust side
 * maps it to /mnt/<drive>/... before passing to xtool.
 *
 * The password should be an app-specific password generated at
 * appleid.apple.com → Sign-In and Security → App-Specific Passwords. xtool
 * stores its own session token after `auth login` succeeds, so this never
 * needs to be supplied again unless the token expires.
 */
export const runXtool = (
  email: string,
  password: string,
  xipPath: string,
): Promise<InstallOutcome> =>
  invoke<InstallOutcome>("setup_run_xtool", { email, password, xipPath });

/** Persist the Apple ID email in Windows Credential Manager (DPAPI-encrypted)
 *  for next-launch pre-fill. The password is never persisted. */
export const storeAppleId = (email: string): Promise<void> =>
  invoke<void>("setup_store_apple_id", { email });

/** Read the previously-stored Apple ID email, or null if none was stored. */
export const getStoredAppleId = (): Promise<string | null> =>
  invoke<string | null>("setup_get_stored_apple_id");

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
