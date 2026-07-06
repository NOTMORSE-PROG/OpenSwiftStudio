// Bridges backend `project-run-progress` events to frontend state (M1-5 /
// M1-6). Install once at app start; it owns the single event subscription and
// the Run/Stop side effects (auto-revealing the Console on run start).

import { setActivePanelTab, setPanelCollapsed } from "../state/appState";
import {
  appendConsoleLines,
  appendConsoleMeta,
  runConfig,
  setRunStatus,
} from "../state/projectState";
import { onRunProgress, startRun, stopRun, type RunEvent } from "./projectApi";

const revealConsole = () => {
  setPanelCollapsed(false);
  setActivePanelTab("console");
};

const applyRunEvent = (event: RunEvent) => {
  switch (event.kind) {
    case "status":
      if (event.status === "building") {
        revealConsole();
        setRunStatus("building");
        appendConsoleMeta(
          `> swift build${runConfig() === "release" ? " -c release" : ""}`,
        );
      } else {
        setRunStatus("running");
        appendConsoleMeta("> Running…");
      }
      break;
    case "output":
      appendConsoleLines(event.stream, event.lines);
      break;
    case "exit":
      setRunStatus("idle");
      appendConsoleMeta(exitMessage(event));
      break;
  }
};

const exitMessage = (event: Extract<RunEvent, { kind: "exit" }>): string => {
  switch (event.outcome) {
    case "exited":
      return `> Program exited with code ${event.code}`;
    case "buildFailed":
      return `> Build failed (exit code ${event.code})`;
    case "stopped":
      return "> Stopped";
    case "spawnError":
      return `> ${event.message ?? "Could not start the process"}`;
  }
};

/** Start listening for run-progress events. Returns an unlisten fn. */
export const installRunListener = () => onRunProgress(applyRunEvent);

/** Kick off a run under the currently-selected configuration. Errors (no
 *  project, no executable, already running) surface as a meta console line. */
export const triggerRun = async () => {
  try {
    await startRun(runConfig());
  } catch (err) {
    revealConsole();
    appendConsoleMeta(`> ${String(err)}`);
  }
};

/** Stop the active run. */
export const triggerStop = async () => {
  try {
    await stopRun();
  } catch (err) {
    console.error("run_stop failed:", err);
  }
};
