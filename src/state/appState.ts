import { createSignal } from "solid-js";

export type ActivityView = "files" | "search" | "debug" | "ios-devices" | "scm";

export const [activeView, setActiveView] = createSignal<ActivityView>("files");
export const [sidebarCollapsed, setSidebarCollapsed] = createSignal(false);
export const [panelCollapsed, setPanelCollapsed] = createSignal(false);
export const [commandPaletteOpen, setCommandPaletteOpen] = createSignal(false);

export type PanelTab = "console" | "debug" | "problems" | "terminal";
export const [activePanelTab, setActivePanelTab] = createSignal<PanelTab>("console");
