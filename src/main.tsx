import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import Home from "./windows/Home";
import Overlay from "./windows/Overlay";
import "./styles.css";

// One bundle serves both windows; the window's label (set in tauri.conf.json) picks the screen.
function windowLabel(): string {
  try {
    return getCurrentWindow().label;
  } catch {
    return "main"; // opened in a plain browser during development
  }
}

const label = windowLabel();
document.documentElement.dataset.window = label;
const Root = label === "overlay" ? Overlay : Home;

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <Root />
  </React.StrictMode>,
);
