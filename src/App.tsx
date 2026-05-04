import { Component } from "solid-js";
import ActivityBar from "./components/ActivityBar";
import Sidebar from "./components/Sidebar";
import EditorArea from "./components/EditorArea";
import Panel from "./components/Panel";
import StatusBar from "./components/StatusBar";
import CommandPalette, { useCommandPaletteShortcut } from "./components/CommandPalette";
import SetupWizard from "./components/SetupWizard";

const App: Component = () => {
  useCommandPaletteShortcut();

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
