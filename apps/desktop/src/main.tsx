import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { App } from "./App";
import { TrayQuotaApp } from "./tray-quota-app";
import "./styles.css";

function isTrayPopoverWindow(): boolean {
  try {
    return getCurrentWebviewWindow().label === "quota-popover";
  } catch {
    return false;
  }
}

const isTrayPopover = isTrayPopoverWindow();

if (isTrayPopover) {
  document.documentElement.dataset.trayPopover = "1";
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    {isTrayPopover ? <TrayQuotaApp /> : <App />}
  </React.StrictMode>,
);
