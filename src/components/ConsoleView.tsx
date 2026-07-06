import { Component, For, Show, createEffect, createSignal, on } from "solid-js";
import { clearConsole, consoleLines } from "../state/projectState";

/// The Console tab body (M1-6). Renders the run's stdout/stderr/meta lines,
/// keeps the view pinned to the newest output unless the user scrolls up, and
/// exposes Clear + a jump-to-bottom affordance.
const ConsoleView: Component = () => {
  let bodyRef: HTMLDivElement | undefined;
  const [pinned, setPinned] = createSignal(true);

  const scrollToBottom = () => {
    if (bodyRef) bodyRef.scrollTop = bodyRef.scrollHeight;
  };

  // Follow new output while pinned. `on(consoleLines, ...)` tracks appends.
  createEffect(
    on(consoleLines, () => {
      if (pinned()) requestAnimationFrame(scrollToBottom);
    }),
  );

  const onScroll = () => {
    if (!bodyRef) return;
    const distanceFromBottom =
      bodyRef.scrollHeight - bodyRef.scrollTop - bodyRef.clientHeight;
    setPinned(distanceFromBottom < 8);
  };

  const jump = () => {
    setPinned(true);
    scrollToBottom();
  };

  return (
    <div class="console">
      <div class="console__toolbar">
        <button
          class="console__btn"
          title="Clear console"
          onClick={() => clearConsole()}
        >
          <span class="codicon codicon-clear-all" aria-hidden="true" />
          <span>Clear</span>
        </button>
      </div>
      <div class="console__body" ref={bodyRef} onScroll={onScroll}>
        <Show
          when={consoleLines().length > 0}
          fallback={<div class="console__empty">Run a project to see build and program output here.</div>}
        >
          <For each={consoleLines()}>
            {(line) => (
              <div class={`console__line console__line--${line.stream}`}>{line.text}</div>
            )}
          </For>
        </Show>
      </div>
      <Show when={!pinned()}>
        <button class="console__jump" title="Scroll to latest output" onClick={jump}>
          <span class="codicon codicon-arrow-down" aria-hidden="true" />
          <span>Jump to bottom</span>
        </button>
      </Show>
    </div>
  );
};

export default ConsoleView;
