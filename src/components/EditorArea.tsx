import { Component } from "solid-js";

const EditorArea: Component = () => {
  return (
    <div class="editor-area">
      <div class="editor-tabs">
        <div class="editor-tab active">Welcome</div>
      </div>
      <div class="editor-body">
        <div class="editor-placeholder">
          <h1>OpenSwiftStudio</h1>
          <p>Build iOS apps from Windows. No Mac required.</p>
          <p style="margin-top: 24px;">
            Press <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>P</kbd> to open the command palette.
          </p>
          <p style="margin-top: 16px; opacity: 0.7;">
            Monaco editor wires up in M2. Run + emulator pane wire up in M3.
          </p>
        </div>
      </div>
    </div>
  );
};

export default EditorArea;
