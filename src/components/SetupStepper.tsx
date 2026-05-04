import { Component, For } from "solid-js";
import {
  SetupStepId,
  SetupStepState,
  STEP_ORDER,
} from "../state/setupState";

const LABELS: Record<SetupStepId, string> = {
  welcome:    "Welcome",
  wsl2:       "WSL2",
  usbipd:     "usbipd-win",
  toolchain:  "Toolchain",
  "apple-id": "Apple ID",
  done:       "Done",
};

type Props = {
  current: SetupStepId;
  states: Record<SetupStepId, SetupStepState>;
};

const SetupStepper: Component<Props> = (props) => {
  return (
    <ol class="setup-stepper">
      <For each={STEP_ORDER}>
        {(id) => {
          const isCurrent = () => id === props.current;
          const status = () => props.states[id].status;
          return (
            <li
              class="setup-stepper__item"
              classList={{
                "is-current": isCurrent(),
                [`is-${status()}`]: true,
              }}
            >
              <span class="setup-stepper__dot" aria-hidden="true" />
              <span class="setup-stepper__label">{LABELS[id]}</span>
            </li>
          );
        }}
      </For>
    </ol>
  );
};

export default SetupStepper;
