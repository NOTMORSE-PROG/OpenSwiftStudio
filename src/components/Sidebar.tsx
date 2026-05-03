import { Component, Match, Switch } from "solid-js";
import { activeView, sidebarCollapsed } from "../state/appState";

const Sidebar: Component = () => {
  return (
    <aside class="sidebar" classList={{ collapsed: sidebarCollapsed() }}>
      <div class="sidebar__header">
        <Switch>
          <Match when={activeView() === "files"}>Explorer</Match>
          <Match when={activeView() === "search"}>Search</Match>
          <Match when={activeView() === "scm"}>Source Control</Match>
          <Match when={activeView() === "debug"}>Run and Debug</Match>
          <Match when={activeView() === "ios-devices"}>iOS Devices</Match>
        </Switch>
      </div>
      <div class="sidebar__content">
        <Switch>
          <Match when={activeView() === "files"}>
            <p class="sidebar__placeholder">
              No project open yet. New Project / Open Folder will appear here in M1.
            </p>
          </Match>
          <Match when={activeView() === "search"}>
            <p class="sidebar__placeholder">Search wires up in M2 (after Monaco + LSP).</p>
          </Match>
          <Match when={activeView() === "scm"}>
            <p class="sidebar__placeholder">Source control wires up in M10 polish.</p>
          </Match>
          <Match when={activeView() === "debug"}>
            <p class="sidebar__placeholder">
              Debug pane wires up in M5 (LLDB DAP integration).
            </p>
          </Match>
          <Match when={activeView() === "ios-devices"}>
            <p class="sidebar__placeholder">
              Device Manager wires up in M4. Will list iPhone SE / 15 / 16 / 16 Pro Max / iPad / iPad Pro and let you switch which one the emulator pane uses.
            </p>
          </Match>
        </Switch>
      </div>
    </aside>
  );
};

export default Sidebar;
