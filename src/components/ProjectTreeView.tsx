import { Component, For, Show } from "solid-js";
import { currentProject, projectFiles } from "../state/projectState";

const ProjectTreeView: Component = () => {
  return (
    <div class="project-tree">
      <Show when={currentProject()?.degraded}>
        <div class="project-tree__degraded" role="alert">
          <strong>Limited project info.</strong>{" "}
          The Swift toolchain isn't on PATH, so we couldn't read the full target list.
          Re-run the setup wizard to install Swift; the file tree below still works.
        </div>
      </Show>
      <ul class="project-tree__list">
        <For each={projectFiles()}>
          {(node) => (
            <li class="project-tree__item" classList={{ "project-tree__item--dir": node.isDirectory }}>
              <span
                class="codicon"
                classList={{
                  "codicon-folder": node.isDirectory,
                  "codicon-file": !node.isDirectory,
                }}
                aria-hidden="true"
              />
              <span class="project-tree__name">{node.name}</span>
            </li>
          )}
        </For>
      </ul>
    </div>
  );
};

export default ProjectTreeView;
