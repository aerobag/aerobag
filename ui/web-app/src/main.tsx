import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { debugLog } from "./domain/debugLog";
import "./styles.css";

debugLog("START", {
  href: typeof window !== "undefined" ? window.location.href : "",
  userAgent: typeof navigator !== "undefined" ? navigator.userAgent : "",
});

if (typeof window !== "undefined") {
  window.addEventListener("error", (event) => {
    debugLog("WINDOW_ERROR", {
      message: event.message,
      filename: event.filename,
      lineno: event.lineno,
      colno: event.colno,
      error: event.error instanceof Error
        ? {
            name: event.error.name,
            message: event.error.message,
            stack: event.error.stack ?? null,
          }
        : String(event.error),
    });
  });

  window.addEventListener("unhandledrejection", (event) => {
    const reason = event.reason;
    debugLog("UNHANDLED_REJECTION", reason instanceof Error
      ? {
          name: reason.name,
          message: reason.message,
          stack: reason.stack ?? null,
        }
      : { reason: String(reason) });
  });
}

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
