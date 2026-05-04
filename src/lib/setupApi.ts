import { invoke } from "@tauri-apps/api/core";
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

export type VsBuildToolsRecord = {
  found: boolean;
  displayName?: string;
  version?: string;
  installPath?: string;
  detectedAt: string;
};

export type SetupState = {
  schemaVersion: number;
  completedAt?: string;
  appVersion: string;
  steps: Record<string, StepRecord>;
  vsBuildToolsDetected?: VsBuildToolsRecord;
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
