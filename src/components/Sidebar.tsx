import { Component, Match, Show, Switch } from "solid-js";
import { activeView, sidebarCollapsed } from "../state/appState";
import { currentProject } from "../state/projectState";
import { restoreNotice, setRestoreNotice } from "../lib/sessionController";
import ProjectTreeView from "./ProjectTreeView";

const Sidebar: Component = () => {
  return (
    <aside class="sidebar" classList={{ collapsed: sidebarCollapsed() }}>
      <div class="sidebar__header">
        <Switch>
          <Match when={activeView() === "files"}>
            <Show when={currentProject()} fallback={<>Explorer</>}>
              {(p) => <span title={p().rootPath}>{p().name.toUpperCase()}</span>}
            </Show>
          </Match>
          <Match when={activeView() === "search"}>Search</Match>
          <Match when={activeView() === "scm"}>Source Control</Match>
          <Match when={activeView() === "debug"}>Run and Debug</Match>
          <Match when={activeView() === "ios-devices"}>iOS Devices</Match>
        </Switch>
      </div>
      <div class="sidebar__content">
        <Switch>
          <Match when={activeView() === "files"}>
            <Show
              when={currentProject()}
              fallback={
                <>
                  <Show when={restoreNotice()}>
                    <div class="sidebar__notice">
                      <span>{restoreNotice()}</span>
                      <button
                        class="sidebar__notice-dismiss"
                        title="Dismiss"
                        onClick={() => setRestoreNotice(null)}
                      >
                        <span class="codicon codicon-close" aria-hidden="true" />
                      </button>
                    </div>
                  </Show>
                  <p class="sidebar__placeholder">
                    No project open. Use Ctrl+Shift+P → "Project: Open" to pick a SwiftPM folder.
                  </p>
                </>
              }
            >
              <ProjectTreeView />
            </Show>
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
