import { Component, Show, createSignal, onMount } from "solid-js";
import { setCommandPaletteOpen } from "../state/appState";
import { currentProject } from "../state/projectState";
import { getToolchain } from "../lib/projectApi";

const StatusBar: Component = () => {
  const [toolchainLabel, setToolchainLabel] = createSignal<string>("Swift");

  onMount(async () => {
    try {
      const r = await getToolchain();
      if (r.found) {
        const v = r.version?.trim();
        setToolchainLabel(v ? `Swift ${v}` : "Swift");
      } else {
        setToolchainLabel("Swift (not detected)");
      }
    } catch (err) {
      console.error("app_get_toolchain failed:", err);
    }
  });

  return (
    <div class="status-bar">
      <div
        class="status-bar__item"
        title="Open command palette"
        onClick={() => setCommandPaletteOpen(true)}
      >
        <span class="codicon codicon-terminal" aria-hidden="true" />
        <span>OpenSwiftStudio</span>
      </div>
      <Show when={currentProject()}>
        {(p) => (
          <div class="status-bar__item status-bar__item--project" title={p().rootPath}>
            <span class="codicon codicon-folder-opened" aria-hidden="true" />
            <span>{p().name}</span>
          </div>
        )}
      </Show>
      <div class="status-bar__item">M1 — pre-release</div>
      <div class="status-bar__spacer" />
      <div class="status-bar__item">UTF-8</div>
      <div class="status-bar__item" title="Active Swift toolchain">
        {toolchainLabel()}
      </div>
      <div class="status-bar__item">Ln 1, Col 1</div>
    </div>
  );
};

export default StatusBar;
