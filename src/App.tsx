import { Component, onCleanup, onMount } from "solid-js";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import ActivityBar from "./components/ActivityBar";
import Sidebar from "./components/Sidebar";
import EditorArea from "./components/EditorArea";
import Panel from "./components/Panel";
import StatusBar from "./components/StatusBar";
import CommandPalette, { useCommandPaletteShortcut } from "./components/CommandPalette";
import SetupWizard from "./components/SetupWizard";
import { installManifestListener } from "./lib/projectActions";
import { installRunListener } from "./lib/runController";
import { installSessionAutosave, restoreSession } from "./lib/sessionController";

const App: Component = () => {
  useCommandPaletteShortcut();

  // Autosave must be installed during setup (before restore flips it on).
  installSessionAutosave();

  // Single listener for run-progress events for the app's lifetime.
  let unlistenRun: UnlistenFn | undefined;
  let unlistenManifest: UnlistenFn | undefined;
  onMount(async () => {
    // The window starts hidden (tauri.conf.json `visible: false`) so saved
    // geometry is applied before anything is on screen; reveal after the
    // first render. Frontend-owned so a first launch with no saved state
    // can never stay hidden.
    const window = getCurrentWindow();
    await window.show();
    await window.setFocus();

    unlistenRun = await installRunListener();
    unlistenManifest = await installManifestListener();
    await restoreSession();
  });
  onCleanup(() => {
    unlistenRun?.();
    unlistenManifest?.();
  });

  return (
    <div class="app">
      <div class="app-body">
        <ActivityBar />
        <Sidebar />
        <div class="main">
          <EditorArea />
          <Panel />
        </div>
      </div>
      <StatusBar />
      <CommandPalette />
      <SetupWizard />
    </div>
  );
};

export default App;
