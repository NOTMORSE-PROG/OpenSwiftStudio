import { createSignal } from "solid-js";

export type SetupStepId =
  | "welcome"
  | "wsl2"
  | "usbipd"
  | "toolchain"
  | "apple-id"
  | "done";

export const STEP_ORDER: SetupStepId[] = [
  "welcome",
  "wsl2",
  "usbipd",
  "toolchain",
  "apple-id",
  "done",
];

export type StepStatus =
  | "pending"
  | "checking"
  | "detected"
  | "missing"
  | "error"
  | "skipped";

export type SetupStepState = {
  status: StepStatus;
  message: string;
};

export const initialStepStates = (): Record<SetupStepId, SetupStepState> => ({
  welcome:    { status: "pending", message: "" },
  wsl2:       { status: "pending", message: "" },
  usbipd:     { status: "pending", message: "" },
  toolchain:  { status: "pending", message: "" },
  "apple-id": { status: "pending", message: "" },
  done:       { status: "pending", message: "" },
});

export const [setupWizardOpen, setSetupWizardOpen] = createSignal(false);
export const [currentStep, setCurrentStep] = createSignal<SetupStepId>("welcome");
export const [stepStates, setStepStates] = createSignal<Record<SetupStepId, SetupStepState>>(initialStepStates());
export const [setupLoaded, setSetupLoaded] = createSignal(false);

export const updateStep = (id: SetupStepId, patch: Partial<SetupStepState>) => {
  const current = stepStates();
  setStepStates({ ...current, [id]: { ...current[id], ...patch } });
};

export const goNext = () => {
  const idx = STEP_ORDER.indexOf(currentStep());
  if (idx >= 0 && idx < STEP_ORDER.length - 1) {
    setCurrentStep(STEP_ORDER[idx + 1]);
  }
};

export const goPrev = () => {
  const idx = STEP_ORDER.indexOf(currentStep());
  if (idx > 0) {
    setCurrentStep(STEP_ORDER[idx - 1]);
  }
};
