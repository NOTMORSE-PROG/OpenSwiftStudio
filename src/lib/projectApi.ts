import { invoke } from "@tauri-apps/api/core";
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

export const openProject = (path: string): Promise<PackageDescription> =>
  invoke<PackageDescription>("project_open", { path });

export const closeProject = (): Promise<void> => invoke<void>("project_close");

export const getProjectMeta = (): Promise<PackageDescription | null> =>
  invoke<PackageDescription | null>("project_get_meta");

export const getProjectFiles = (path: string): Promise<FileTreeNode[]> =>
  invoke<FileTreeNode[]>("project_get_files", { path });

export const getToolchain = (): Promise<SetupCheckResult> =>
  invoke<SetupCheckResult>("app_get_toolchain");
