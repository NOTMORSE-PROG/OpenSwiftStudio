import { Component } from "solid-js";
import { setCommandPaletteOpen } from "../state/appState";

const StatusBar: Component = () => {
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
      <div class="status-bar__item">M0 — pre-release</div>
      <div class="status-bar__spacer" />
      <div class="status-bar__item">UTF-8</div>
      <div class="status-bar__item">Swift</div>
      <div class="status-bar__item">Ln 1, Col 1</div>
    </div>
  );
};

export default StatusBar;
