import { Component, For, Show, createResource } from "solid-js";
import { recentProjects } from "../state/projectState";
import { pathsExist } from "../lib/projectApi";
import { openProjectByPath, removeRecentProject } from "../lib/projectActions";

/// Last path segment for a project display label.
const basename = (p: string): string =>
  p.replace(/[\\/]+$/, "").split(/[\\/]/).pop() || p;

const EditorArea: Component = () => {
  // Existence flags for the recent list, refetched whenever it changes, so we
  // can mark entries whose folder was deleted (M1-8).
  const [exists] = createResource(recentProjects, pathsExist);
  const isStale = (i: number): boolean => exists()?.[i] === false;

  return (
    <div class="editor-area">
      <div class="editor-tabs">
        <div class="editor-tab active">Welcome</div>
      </div>
      <div class="editor-body">
        <div class="editor-placeholder">
          <h1>OpenSwiftStudio</h1>
          <p>Build iOS apps from Windows. No Mac required.</p>

          <Show when={recentProjects().length > 0}>
            <div class="welcome-recent">
              <h2 class="welcome-recent__title">Recent</h2>
              <For each={recentProjects()}>
                {(p, i) => (
                  <div
                    class="welcome-recent__row"
                    classList={{ "welcome-recent__row--stale": isStale(i()) }}
                  >
                    <button
                      class="welcome-recent__open"
                      disabled={isStale(i())}
                      title={p}
                      onClick={() => { if (!isStale(i())) void openProjectByPath(p); }}
                    >
                      <span class="welcome-recent__name">{basename(p)}</span>
                      <span class="welcome-recent__path">{p}</span>
                    </button>
                    <Show when={isStale(i())}>
                      <span class="welcome-recent__badge" title="Folder no longer exists">missing</span>
                      <button
                        class="welcome-recent__remove"
                        title="Remove from Recent"
                        onClick={() => removeRecentProject(p)}
                      >
                        <span class="codicon codicon-close" aria-hidden="true" />
                      </button>
                    </Show>
                  </div>
                )}
              </For>
            </div>
          </Show>

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
