import { Component, For, createMemo, createSignal, Show, onMount, onCleanup } from "solid-js";
import {
  commandPaletteOpen, setCommandPaletteOpen,
  setSidebarCollapsed, sidebarCollapsed,
  setPanelCollapsed, panelCollapsed,
  setActiveView,
} from "../state/appState";
import {
  initialStepStates,
  setCurrentStep,
  setSetupWizardOpen,
  setStepStates,
} from "../state/setupState";
import { resetSetup } from "../lib/setupApi";

type Command = {
  id: string;
  label: string;
  run: () => void;
};

const commands = (): Command[] => [
  {
    id: "view.toggleSidebar",
    label: "View: Toggle Sidebar",
    run: () => setSidebarCollapsed(!sidebarCollapsed()),
  },
  {
    id: "view.togglePanel",
    label: "View: Toggle Panel",
    run: () => setPanelCollapsed(!panelCollapsed()),
  },
  { id: "view.openExplorer",   label: "View: Open Explorer",      run: () => setActiveView("files") },
  { id: "view.openSearch",     label: "View: Open Search",        run: () => setActiveView("search") },
  { id: "view.openDebug",      label: "View: Open Run and Debug", run: () => setActiveView("debug") },
  { id: "view.openIosDevices", label: "View: Open iOS Devices",   run: () => setActiveView("ios-devices") },
  { id: "view.openSCM",        label: "View: Open Source Control", run: () => setActiveView("scm") },
  { id: "project.new",  label: "Project: New (wires up in M1)",  run: () => console.log("M1") },
  { id: "run.start",    label: "Run: Start (wires up in M3)",    run: () => console.log("M3") },
  { id: "debug.start",  label: "Debug: Start (wires up in M5)",  run: () => console.log("M5") },
  {
    id: "setup.rerun",
    label: "Setup: Re-run Setup Wizard...",
    run: () => {
      void resetSetup()
        .catch((err) => console.error("setup_reset failed:", err))
        .finally(() => {
          setCurrentStep("welcome");
          setStepStates(initialStepStates());
          setSetupWizardOpen(true);
        });
    },
  },
];

const fuzzyScore = (query: string, text: string): number => {
  if (!query) return 1;
  const q = query.toLowerCase();
  const t = text.toLowerCase();
  if (t.includes(q)) return 1;
  const stripSeparators = (s: string) => s.replace(/[-_]/g, "");
  const qStripped = stripSeparators(q);
  const tStripped = stripSeparators(t);
  if (qStripped && tStripped.includes(qStripped)) return 1;
  let qi = 0;
  for (let i = 0; i < t.length && qi < q.length; i++) {
    if (t[i] === q[qi]) qi++;
  }
  return qi === q.length ? 0.5 : 0;
};

const CommandPalette: Component = () => {
  const [query, setQuery] = createSignal("");
  const [selected, setSelected] = createSignal(0);
  let inputRef: HTMLInputElement | undefined;

  const filtered = createMemo(() => {
    const q = query().trim();
    return commands()
      .map((c) => ({ cmd: c, score: fuzzyScore(q, c.label) }))
      .filter((x) => x.score > 0)
      .sort((a, b) => b.score - a.score)
      .map((x) => x.cmd);
  });

  const close = () => {
    setCommandPaletteOpen(false);
    setQuery("");
    setSelected(0);
  };

  const runSelected = () => {
    const items = filtered();
    if (items[selected()]) {
      items[selected()].run();
      close();
    }
  };

  const onKey = (e: KeyboardEvent) => {
    if (e.key === "Escape") {
      e.preventDefault();
      close();
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      setSelected((s) => Math.min(s + 1, filtered().length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setSelected((s) => Math.max(s - 1, 0));
    } else if (e.key === "Enter") {
      e.preventDefault();
      runSelected();
    }
  };

  onMount(() => {
    queueMicrotask(() => inputRef?.focus());
  });

  return (
    <Show when={commandPaletteOpen()}>
      <div class="cmd-palette-overlay" onClick={close}>
        <div class="cmd-palette" onClick={(e) => e.stopPropagation()}>
          <input
            ref={inputRef}
            class="cmd-palette__input"
            type="text"
            placeholder="Type a command..."
            value={query()}
            onInput={(e) => { setQuery(e.currentTarget.value); setSelected(0); }}
            onKeyDown={onKey}
          />
          <div class="cmd-palette__list">
            <For each={filtered()}>
              {(cmd, idx) => (
                <div
                  class="cmd-palette__item"
                  classList={{ selected: idx() === selected() }}
                  onMouseEnter={() => setSelected(idx())}
                  onClick={() => { cmd.run(); close(); }}
                >
                  {cmd.label}
                </div>
              )}
            </For>
          </div>
        </div>
      </div>
    </Show>
  );
};

export const useCommandPaletteShortcut = () => {
  const handler = (e: KeyboardEvent) => {
    if ((e.ctrlKey || e.metaKey) && e.shiftKey && (e.key === "P" || e.key === "p")) {
      e.preventDefault();
      setCommandPaletteOpen(true);
    }
  };
  onMount(() => window.addEventListener("keydown", handler));
  onCleanup(() => window.removeEventListener("keydown", handler));
};

export default CommandPalette;
