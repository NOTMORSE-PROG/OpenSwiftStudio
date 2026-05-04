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
  SetupStepId,
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
  checkVsBuildTools,
  getSetupState,
  markSetupComplete,
  openExternal,
} from "../lib/setupApi";
import SetupStepper from "./SetupStepper";

const APP_VERSION_FALLBACK = "0.0.1";
const SCHEMA_VERSION = 1;
const VS_BUILD_TOOLS_DOCS_URL =
  "https://visualstudio.microsoft.com/visual-cpp-build-tools/";

const SetupWizard: Component = () => {
  const isFirstStep = () => currentStep() === STEP_ORDER[0];
  const isLastStep = () => currentStep() === STEP_ORDER[STEP_ORDER.length - 1];

  const [hasExistingSetup, setHasExistingSetup] = createSignal(false);
  const [vsBuildTools, setVsBuildTools] = createSignal<SetupCheckResult | null>(null);

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
            ? "stub-in-foundation-chunk"
            : undefined,
      };
    }
    const vs = vsBuildTools();
    const next: SetupState = {
      schemaVersion: SCHEMA_VERSION,
      completedAt: now,
      appVersion: APP_VERSION_FALLBACK,
      steps,
      vsBuildToolsDetected: vs
        ? {
            found: vs.found,
            displayName: vs.displayName,
            version: vs.version,
            installPath: vs.installPath,
            detectedAt: now,
          }
        : undefined,
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
                <PrereqStubStep
                  id="wsl2"
                  title="Windows Subsystem for Linux 2"
                  description="WSL2 hosts the libimobiledevice + xtool bridge that lets the IDE talk to a real iPhone over USB. Detection and install land in the next setup-wizard chunk."
                />
              </Match>
              <Match when={currentStep() === "usbipd"}>
                <PrereqStubStep
                  id="usbipd"
                  title="usbipd-win"
                  description="Bridges your USB-connected iPhone into WSL2 so xtool can sign and deploy. Detection and install land in the next setup-wizard chunk."
                />
              </Match>
              <Match when={currentStep() === "toolchain"}>
                <ToolchainStep
                  vsBuildTools={vsBuildTools()}
                  onVsBuildToolsResult={(r) => setVsBuildTools(r)}
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

type PrereqStubStepProps = {
  id: SetupStepId;
  title: string;
  description: string;
};

const PrereqStubStep: Component<PrereqStubStepProps> = (props) => (
  <section class="setup-step">
    <h2 class="setup-step__heading">{props.title}</h2>
    <p class="setup-step__body">{props.description}</p>
    <div class="setup-step__check-row">
      <span class="setup-step__badge is-skipped">Skipped (stub)</span>
      <span class="setup-step__check-label">
        Real detection lands in the next setup-wizard chunk.
      </span>
    </div>
  </section>
);

type ToolchainStepProps = {
  vsBuildTools: SetupCheckResult | null;
  onVsBuildToolsResult: (r: SetupCheckResult) => void;
};

const ToolchainStep: Component<ToolchainStepProps> = (props) => {
  const [busy, setBusy] = createSignal(false);

  const runDetect = async () => {
    setBusy(true);
    updateStep("toolchain", { status: "checking", message: "" });
    try {
      const result = await checkVsBuildTools();
      props.onVsBuildToolsResult(result);
      if (result.found) {
        const versionLine = result.version ? ` ${result.version}` : "";
        updateStep("toolchain", {
          status: "detected",
          message: `${result.displayName ?? "Visual Studio Build Tools"}${versionLine}`,
        });
      } else {
        updateStep("toolchain", {
          status: "missing",
          message: result.message ?? "Visual Studio Build Tools not detected.",
        });
      }
    } catch (err) {
      updateStep("toolchain", {
        status: "error",
        message: `Detection failed: ${String(err)}`,
      });
    } finally {
      setBusy(false);
    }
  };

  return (
    <section class="setup-step">
      <h2 class="setup-step__heading">Toolchain prerequisites</h2>
      <p class="setup-step__body">
        Swift on Windows compiles against MSVC. We detect Visual Studio Build Tools 2019+
        here. The Swift 6.2.0 toolchain itself is downloaded and verified in the next
        setup-wizard chunk.
      </p>

      <div class="setup-step__check-row">
        <span
          class="setup-step__badge"
          classList={{
            "is-pending": !props.vsBuildTools && !busy(),
            "is-checking": busy(),
            "is-detected": !!props.vsBuildTools?.found,
            "is-missing": props.vsBuildTools !== null && !props.vsBuildTools.found,
          }}
        >
          {busy()
            ? "Checking..."
            : props.vsBuildTools
            ? props.vsBuildTools.found
              ? "Detected"
              : "Not detected"
            : "Not checked"}
        </span>
        <div class="setup-step__check-content">
          <div class="setup-step__check-label">Visual Studio Build Tools 2019+</div>
          <Show when={props.vsBuildTools}>
            <div class="setup-step__check-detail">
              {props.vsBuildTools?.displayName ?? ""}
              {props.vsBuildTools?.version ? ` ${props.vsBuildTools.version}` : ""}
              {props.vsBuildTools?.installPath ? (
                <>
                  <br />
                  <span class="setup-step__check-path">
                    {props.vsBuildTools.installPath}
                  </span>
                </>
              ) : null}
              <Show when={props.vsBuildTools?.message}>
                <br />
                <span class="setup-step__check-detail">
                  {props.vsBuildTools?.message}
                </span>
              </Show>
            </div>
          </Show>
        </div>
        <div class="setup-step__check-actions">
          <button
            class="setup-wizard__btn setup-wizard__btn--secondary"
            onClick={runDetect}
            disabled={busy()}
          >
            Detect
          </button>
          <Show when={props.vsBuildTools !== null && !props.vsBuildTools?.found}>
            <button
              class="setup-wizard__btn"
              onClick={() => void openExternal(VS_BUILD_TOOLS_DOCS_URL)}
            >
              Get Build Tools
            </button>
          </Show>
        </div>
      </div>

      <div class="setup-step__check-row">
        <span class="setup-step__badge is-skipped">Skipped (stub)</span>
        <div class="setup-step__check-content">
          <div class="setup-step__check-label">Swift 6.2.0 toolchain</div>
          <div class="setup-step__check-detail">
            Download + verify lands in the next setup-wizard chunk (M0.5-5).
          </div>
        </div>
      </div>
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
