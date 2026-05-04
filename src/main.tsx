/* @refresh reload */
import { render } from "solid-js/web";
import "@vscode/codicons/dist/codicon.css";
import "./styles/vscode-theme.css";
import "./styles/layout.css";
import "./styles/setup-wizard.css";
import App from "./App";

const root = document.getElementById("root");
if (!root) throw new Error("#root not found");
render(() => <App />, root);
