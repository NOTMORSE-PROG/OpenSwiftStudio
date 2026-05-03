import { Component, For } from "solid-js";
import { activeView, setActiveView, ActivityView } from "../state/appState";

type Item = { id: ActivityView; label: string; codicon: string };

const items: Item[] = [
  { id: "files",        label: "Explorer",      codicon: "files" },
  { id: "search",       label: "Search",        codicon: "search" },
  { id: "scm",          label: "Source Control", codicon: "source-control" },
  { id: "debug",        label: "Run and Debug",  codicon: "debug-alt" },
  { id: "ios-devices",  label: "iOS Devices",    codicon: "device-mobile" },
];

const ActivityBar: Component = () => {
  return (
    <div class="activity-bar" role="tablist" aria-label="Activity Bar">
      <For each={items}>
        {(item) => (
          <div
            class="activity-bar__item"
            classList={{ active: activeView() === item.id }}
            title={item.label}
            role="tab"
            aria-selected={activeView() === item.id}
            onClick={() => setActiveView(item.id)}
          >
            <span class={`codicon codicon-${item.codicon} activity-bar__icon`} aria-hidden="true" />
          </div>
        )}
      </For>
      <div class="activity-bar__spacer" />
    </div>
  );
};

export default ActivityBar;
