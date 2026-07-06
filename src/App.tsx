import { Component, onCleanup, onMount } from "solid-js";
import type { UnlistenFn } from "@tauri-apps/api/event";
import ActivityBar from "./components/ActivityBar";
import Sidebar from "./components/Sidebar";
import EditorArea from "./components/EditorArea";
import Panel from "./components/Panel";
import StatusBar from "./components/StatusBar";
import CommandPalette, { useCommandPaletteShortcut } from "./components/CommandPalette";
import SetupWizard from "./components/SetupWizard";
import { installRunListener } from "./lib/runController";
import { installSessionAutosave, restoreSession } from "./lib/sessionController";

const App: Component = () => {
  useCommandPaletteShortcut();

  // Autosave must be installed during setup (before restore flips it on).
  installSessionAutosave();

  // Single listener for run-progress events for the app's lifetime.
  let unlistenRun: UnlistenFn | undefined;
  onMount(async () => {
    unlistenRun = await installRunListener();
    await restoreSession();
  });
  onCleanup(() => unlistenRun?.());

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
