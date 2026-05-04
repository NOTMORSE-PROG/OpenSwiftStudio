import {
  Component,
  Match,
  Show,
  Switch,
  createSignal,
  onCleanup,
  onMount,
} from "solid-js";
import {
  STEP_ORDER,
  currentStep,
  goNext,
  goPrev,
  setSetupLoaded,
  setSetupWizardOpen,
  setupLoaded,
  setupWizardOpen,
  stepStates,
  updateStep,
} from "../state/setupState";
import {
  SetupCheckResult,
  SetupState,
  StepRecord,
  checkToolchain,
  checkUsbipd,
  checkVsBuildTools,
  checkWsl2,
  getSetupState,
  markSetupComplete,
  openExternal,
} from "../lib/setupApi";
import SetupStepper from "./SetupStepper";

const APP_VERSION_FALLBACK = "0.0.1";
const SCHEMA_VERSION = 1;
const VS_BUILD_TOOLS_DOCS_URL =
  "https://visualstudio.microsoft.com/visual-cpp-build-tools/";
const WSL_DOCS_URL = "https://learn.microsoft.com/windows/wsl/install";
const USBIPD_DOCS_URL = "https://github.com/dorssel/usbipd-win/releases/latest";
const SWIFT_DOCS_URL = "https://www.swift.org/install/windows/";

const SetupWizard: Component = () => {
  const isFirstStep = () => currentStep() === STEP_ORDER[0];
  const isLastStep = () => currentStep() === STEP_ORDER[STEP_ORDER.length - 1];

  const [hasExistingSetup, setHasExistingSetup] = createSignal(false);
  const [vsBuildTools, setVsBuildTools] = createSignal<SetupCheckResult | null>(null);
  const [wsl2Result, setWsl2Result] = createSignal<SetupCheckResult | null>(null);
  const [usbipdResult, setUsbipdResult] = createSignal<SetupCheckResult | null>(null);
  const [swiftResult, setSwiftResult] = createSignal<SetupCheckResult | null>(null);

  const detectionToRecord = (r: SetupCheckResult | null, now: string) =>
    r
      ? {
          found: r.found,
          displayName: r.displayName,
          version: r.version,
          installPath: r.installPath,
          detectedAt: now,
        }
      : undefined;

  onMount(async () => {
    try {
      const existing = await getSetupState();
      if (existing === null) {
        setHasExistingSetup(false);
        setSetupWizardOpen(true);
      } else {
        setHasExistingSetup(true);
      }
    } catch (err) {
      // If the IPC fails we still let the user into the IDE; they can re-run
      // the wizard from the palette. Logging only — no modal blocker.
      console.error("setup_get_state failed:", err);
    } finally {
      setSetupLoaded(true);
    }

    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      if (!setupWizardOpen()) return;
      if (!hasExistingSetup()) return; // first run: ESC does nothing
      e.preventDefault();
      setSetupWizardOpen(false);
    };
    window.addEventListener("keydown", onKey);
    onCleanup(() => window.removeEventListener("keydown", onKey));
  });

  const finish = async () => {
    const now = new Date().toISOString();
    const steps: Record<string, StepRecord> = {};
    for (const id of STEP_ORDER) {
      const state = stepStates()[id];
      const isWelcomeOrDone = id === "welcome" || id === "done";
      steps[id] = {
        completed: isWelcomeOrDone || state.status === "detected",
        skipped: !isWelcomeOrDone && state.status !== "detected",
        reason:
          !isWelcomeOrDone && state.status !== "detected"
            ? "detect-only"
            : undefined,
      };
    }
    const next: SetupState = {
      schemaVersion: SCHEMA_VERSION,
      completedAt: now,
      appVersion: APP_VERSION_FALLBACK,
      steps,
      vsBuildToolsDetected: detectionToRecord(vsBuildTools(), now),
      wsl2Detected: detectionToRecord(wsl2Result(), now),
      usbipdDetected: detectionToRecord(usbipdResult(), now),
      swiftDetected: detectionToRecord(swiftResult(), now),
    };
    try {
      await markSetupComplete(next);
      setHasExistingSetup(true);
      setSetupWizardOpen(false);
    } catch (err) {
      console.error("setup_mark_complete failed:", err);
      updateStep("done", {
        status: "error",
        message: `Could not save setup state: ${String(err)}`,
      });
    }
  };

  const onPrimary = () => {
    if (isLastStep()) {
      void finish();
    } else {
      goNext();
    }
  };

  return (
    <Show when={setupLoaded() && setupWizardOpen()}>
      <div class="setup-wizard-overlay" role="dialog" aria-modal="true" aria-label="OpenSwiftStudio Setup">
        <div class="setup-wizard">
          <header class="setup-wizard__header">
            <h1 class="setup-wizard__title">OpenSwiftStudio Setup</h1>
            <p class="setup-wizard__subtitle">
              One-time setup. After this, the IDE never asks you to log in again.
            </p>
            <SetupStepper current={currentStep()} states={stepStates()} />
          </header>

          <main class="setup-wizard__body">
            <Switch>
              <Match when={currentStep() === "welcome"}>
                <WelcomeStep />
              </Match>
              <Match when={currentStep() === "wsl2"}>
                <Wsl2Step
                  result={wsl2Result()}
                  onResult={(r) => setWsl2Result(r)}
                />
              </Match>
              <Match when={currentStep() === "usbipd"}>
                <UsbipdStep
                  result={usbipdResult()}
                  onResult={(r) => setUsbipdResult(r)}
                />
              </Match>
              <Match when={currentStep() === "toolchain"}>
                <ToolchainStep
                  vsBuildTools={vsBuildTools()}
                  onVsBuildToolsResult={(r) => setVsBuildTools(r)}
                  swift={swiftResult()}
                  onSwiftResult={(r) => setSwiftResult(r)}
                />
              </Match>
              <Match when={currentStep() === "apple-id"}>
                <AppleIdStubStep />
              </Match>
              <Match when={currentStep() === "done"}>
                <DoneStep />
              </Match>
            </Switch>
          </main>

          <footer class="setup-wizard__footer">
            <button
              class="setup-wizard__btn setup-wizard__btn--secondary"
              onClick={goPrev}
              disabled={isFirstStep()}
            >
              Back
            </button>
            <button class="setup-wizard__btn" onClick={onPrimary}>
              {isLastStep() ? "Finish" : "Continue"}
            </button>
          </footer>
        </div>
      </div>
    </Show>
  );
};

const WelcomeStep: Component = () => (
  <section class="setup-step">
    <h2 class="setup-step__heading">Welcome.</h2>
    <p class="setup-step__body">
      OpenSwiftStudio is a free, open-source IDE that lets you build, run, debug, and ship
      iOS apps from Windows. No Mac required. The next few steps make sure your machine
      has the prerequisites to run the toolchain.
    </p>
    <p class="setup-step__body">
      You can re-run this wizard any time via <code>Ctrl+Shift+P</code> &rarr;{" "}
      <em>Setup: Re-run Setup Wizard&hellip;</em>
    </p>
  </section>
);

// Single check-row block used by Wsl2Step / UsbipdStep / ToolchainStep.
// Inlined here rather than a generic component because each call site needs
// slightly different copy + URL handling, and the row is small.
type CheckRowProps = {
  result: SetupCheckResult | null;
  busy: boolean;
  label: string;
  onDetect: () => void;
  installUrl: string;
  installButtonLabel: string;
};

const CheckRow: Component<CheckRowProps> = (props) => (
  <div class="setup-step__check-row">
    <span
      class="setup-step__badge"
      classList={{
        "is-pending": !props.result && !props.busy,
        "is-checking": props.busy,
        "is-detected": !!props.result?.found,
        "is-missing": props.result !== null && !props.result.found,
      }}
    >
      {props.busy
        ? "Checking..."
        : props.result
        ? props.result.found
          ? "Detected"
          : "Not detected"
        : "Not checked"}
    </span>
    <div class="setup-step__check-content">
      <div class="setup-step__check-label">{props.label}</div>
      <Show when={props.result}>
        <div class="setup-step__check-detail">
          {props.result?.displayName ?? ""}
          {props.result?.version ? ` ${props.result.version}` : ""}
          {props.result?.installPath ? (
            <>
              <br />
              <span class="setup-step__check-path">
                {props.result.installPath}
              </span>
            </>
          ) : null}
          <Show when={props.result?.message}>
            <br />
            <span class="setup-step__check-detail">{props.result?.message}</span>
          </Show>
        </div>
      </Show>
    </div>
    <div class="setup-step__check-actions">
      <button
        class="setup-wizard__btn setup-wizard__btn--secondary"
        onClick={props.onDetect}
        disabled={props.busy}
      >
        Detect
      </button>
      <Show when={props.result !== null && !props.result?.found}>
        <button
          class="setup-wizard__btn"
          onClick={() => void openExternal(props.installUrl)}
        >
          {props.installButtonLabel}
        </button>
      </Show>
    </div>
  </div>
);

type DetectStepProps = {
  result: SetupCheckResult | null;
  onResult: (r: SetupCheckResult) => void;
};

const Wsl2Step: Component<DetectStepProps> = (props) => {
  const [busy, setBusy] = createSignal(false);
  const runDetect = async () => {
    setBusy(true);
    updateStep("wsl2", { status: "checking", message: "" });
    try {
      const result = await checkWsl2();
      props.onResult(result);
      updateStep("wsl2", {
        status: result.found ? "detected" : "missing",
        message: result.found
          ? `${result.displayName ?? "WSL"}${result.version ? ` ${result.version}` : ""}`
          : result.message ?? "WSL2 not detected.",
      });
    } catch (err) {
      updateStep("wsl2", { status: "error", message: `Detection failed: ${String(err)}` });
    } finally {
      setBusy(false);
    }
  };
  return (
    <section class="setup-step">
      <h2 class="setup-step__heading">Windows Subsystem for Linux 2</h2>
      <p class="setup-step__body">
        WSL2 hosts the libimobiledevice + xtool bridge that lets the IDE talk to a real
        iPhone over USB. Detection here only — the assisted install (UAC + reboot
        guidance) lands in the next setup-wizard chunk.
      </p>
      <CheckRow
        result={props.result}
        busy={busy()}
        label="Windows Subsystem for Linux 2"
        onDetect={runDetect}
        installUrl={WSL_DOCS_URL}
        installButtonLabel="Get WSL2"
      />
    </section>
  );
};

const UsbipdStep: Component<DetectStepProps> = (props) => {
  const [busy, setBusy] = createSignal(false);
  const runDetect = async () => {
    setBusy(true);
    updateStep("usbipd", { status: "checking", message: "" });
    try {
      const result = await checkUsbipd();
      props.onResult(result);
      updateStep("usbipd", {
        status: result.found ? "detected" : "missing",
        message: result.found
          ? `${result.displayName ?? "usbipd-win"}${result.version ? ` ${result.version}` : ""}`
          : result.message ?? "usbipd-win not detected.",
      });
    } catch (err) {
      updateStep("usbipd", { status: "error", message: `Detection failed: ${String(err)}` });
    } finally {
      setBusy(false);
    }
  };
  return (
    <section class="setup-step">
      <h2 class="setup-step__heading">usbipd-win</h2>
      <p class="setup-step__body">
        Bridges your USB-connected iPhone into WSL2 so xtool can sign and deploy. Detection
        here only — the assisted install (winget primary, MSI fallback) lands in the next
        setup-wizard chunk.
      </p>
      <CheckRow
        result={props.result}
        busy={busy()}
        label="usbipd-win"
        onDetect={runDetect}
        installUrl={USBIPD_DOCS_URL}
        installButtonLabel="Get usbipd-win"
      />
    </section>
  );
};

type ToolchainStepProps = {
  vsBuildTools: SetupCheckResult | null;
  onVsBuildToolsResult: (r: SetupCheckResult) => void;
  swift: SetupCheckResult | null;
  onSwiftResult: (r: SetupCheckResult) => void;
};

const ToolchainStep: Component<ToolchainStepProps> = (props) => {
  const [vsBusy, setVsBusy] = createSignal(false);
  const [swiftBusy, setSwiftBusy] = createSignal(false);

  // Toolchain step status reflects the *combined* prereqs: only "detected"
  // when both VS Build Tools and Swift are found, otherwise the worst-case
  // status (missing/error) wins so the stepper dot reads accurately.
  const updateCombinedStatus = () => {
    const vs = props.vsBuildTools;
    const sw = props.swift;
    if (!vs || !sw) {
      // Some checks haven't run yet — leave whatever was set by the most
      // recent runDetect call alone.
      return;
    }
    if (vs.found && sw.found) {
      updateStep("toolchain", {
        status: "detected",
        message: `Both VS Build Tools (${vs.version ?? "?"}) and Swift (${sw.version ?? "?"}) detected.`,
      });
    } else {
      const missing = [
        !vs.found ? "VS Build Tools" : null,
        !sw.found ? "Swift" : null,
      ]
        .filter(Boolean)
        .join(", ");
      updateStep("toolchain", {
        status: "missing",
        message: `Missing: ${missing}.`,
      });
    }
  };

  const runVsDetect = async () => {
    setVsBusy(true);
    updateStep("toolchain", { status: "checking", message: "" });
    try {
      const result = await checkVsBuildTools();
      props.onVsBuildToolsResult(result);
      updateCombinedStatus();
    } catch (err) {
      updateStep("toolchain", {
        status: "error",
        message: `VS Build Tools detection failed: ${String(err)}`,
      });
    } finally {
      setVsBusy(false);
    }
  };

  const runSwiftDetect = async () => {
    setSwiftBusy(true);
    updateStep("toolchain", { status: "checking", message: "" });
    try {
      const result = await checkToolchain();
      props.onSwiftResult(result);
      updateCombinedStatus();
    } catch (err) {
      updateStep("toolchain", {
        status: "error",
        message: `Swift detection failed: ${String(err)}`,
      });
    } finally {
      setSwiftBusy(false);
    }
  };

  return (
    <section class="setup-step">
      <h2 class="setup-step__heading">Toolchain prerequisites</h2>
      <p class="setup-step__body">
        Swift on Windows compiles against MSVC, so the wizard checks both. The Swift
        toolchain download + verify itself lands in the next setup-wizard chunk; here we
        just confirm whether you already have a version installed.
      </p>

      <CheckRow
        result={props.vsBuildTools}
        busy={vsBusy()}
        label="Visual Studio Build Tools 2019+"
        onDetect={runVsDetect}
        installUrl={VS_BUILD_TOOLS_DOCS_URL}
        installButtonLabel="Get Build Tools"
      />

      <CheckRow
        result={props.swift}
        busy={swiftBusy()}
        label="Swift toolchain"
        onDetect={runSwiftDetect}
        installUrl={SWIFT_DOCS_URL}
        installButtonLabel="Get Swift"
      />
    </section>
  );
};

const AppleIdStubStep: Component = () => (
  <section class="setup-step">
    <h2 class="setup-step__heading">Apple ID</h2>
    <p class="setup-step__body">
      The next setup-wizard chunk will collect your free Apple ID and let you drop in the
      Xcode .xip you download from <em>developer.apple.com</em>. We never re-host the .xip
      (clean-room rules); you fetch it under your own account.
    </p>
    <div class="setup-step__check-row">
      <span class="setup-step__badge is-skipped">Skipped (stub)</span>
      <span class="setup-step__check-label">
        Apple ID + Xcode .xip flow lands in M0.5-6.
      </span>
    </div>
  </section>
);

const DoneStep: Component = () => (
  <section class="setup-step">
    <h2 class="setup-step__heading">All set.</h2>
    <p class="setup-step__body">
      Click Finish to save your setup state. The IDE will not show this wizard again. If
      you ever need to re-run it (new machine, fresh install, retry a check), open the
      command palette with <code>Ctrl+Shift+P</code> and pick{" "}
      <em>Setup: Re-run Setup Wizard&hellip;</em>
    </p>
  </section>
);

export default SetupWizard;
