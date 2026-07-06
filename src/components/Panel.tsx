import { Component, For, Match, Switch } from "solid-js";
import { activePanelTab, setActivePanelTab, panelCollapsed, PanelTab } from "../state/appState";
import ConsoleView from "./ConsoleView";

type TabDef = { id: PanelTab; label: string };

const tabs: TabDef[] = [
  { id: "console",  label: "Console" },
  { id: "debug",    label: "Debug" },
  { id: "problems", label: "Problems" },
  { id: "terminal", label: "Terminal" },
];

const Panel: Component = () => {
  return (
    <div class="panel" classList={{ collapsed: panelCollapsed() }}>
      <div class="panel__tabs" role="tablist">
        <For each={tabs}>
          {(t) => (
            <div
              class="panel__tab"
              classList={{ active: activePanelTab() === t.id }}
              role="tab"
              aria-selected={activePanelTab() === t.id}
              onClick={() => setActivePanelTab(t.id)}
            >
              {t.label}
            </div>
          )}
        </For>
      </div>
      <div class="panel__body" classList={{ "panel__body--console": activePanelTab() === "console" }}>
        <Switch>
          <Match when={activePanelTab() === "console"}>
            <ConsoleView />
          </Match>
          <Match when={activePanelTab() === "debug"}>
            Debug output will appear here once M5 wires LLDB DAP.
          </Match>
          <Match when={activePanelTab() === "problems"}>
            No problems detected. Errors and warnings from sourcekit-lsp will appear here in M2.
          </Match>
          <Match when={activePanelTab() === "terminal"}>
            Integrated terminal will be wired in M1.
          </Match>
        </Switch>
      </div>
    </div>
  );
};

export default Panel;
