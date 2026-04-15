import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { debugLog } from "./domain/debugLog";
import "./styles.css";

declare global {
  interface Window {
    __aerobag_html_start?: number;
    __aerobag_hide_startup_shell?: () => void;
    __aerobag_startup_elapsed_interval?: number;
  }
}

function dismissStartupShell() {
  const shell = document.getElementById("startup-shell");
  if (!shell) {
    return;
  }
  if (typeof window !== "undefined" && window.__aerobag_startup_elapsed_interval != null) {
    window.clearInterval(window.__aerobag_startup_elapsed_interval);
    window.__aerobag_startup_elapsed_interval = undefined;
  }
  shell.remove();
}

if (typeof window !== "undefined") {
  window.__aerobag_hide_startup_shell = dismissStartupShell;
}

function logStartupTiming() {
  const navigationEntry = performance.getEntriesByType("navigation")[0];
  debugLog("START", {
    href: typeof window !== "undefined" ? window.location.href : "",
    userAgent: typeof navigator !== "undefined" ? navigator.userAgent : "",
    html_start_ms: typeof window !== "undefined" ? window.__aerobag_html_start ?? null : null,
    main_start_ms: Math.round(performance.now()),
    navigation: navigationEntry && "toJSON" in navigationEntry
      ? (navigationEntry as PerformanceNavigationTiming).toJSON()
      : null,
  });
}

function installPaintObservers() {
  if (typeof PerformanceObserver === "undefined") {
    return;
  }
  try {
    const observer = new PerformanceObserver((list) => {
      for (const entry of list.getEntries()) {
        debugLog("PERF_PAINT", {
          name: entry.name,
          start_time_ms: Math.round(entry.startTime),
          duration_ms: Math.round(entry.duration),
        });
      }
    });
    observer.observe({ type: "paint", buffered: true });
  } catch {
    // Paint entries may be unavailable in some browser modes.
  }
}

function logStartupResources() {
  window.setTimeout(() => {
    const resources = performance
      .getEntriesByType("resource")
      .map((entry) => entry as PerformanceResourceTiming)
      .filter((entry) => {
        const url = entry.name;
        return (
          url.includes("/src/") ||
          url.includes("/node_modules/.vite/") ||
          url.includes("/node_modules/") ||
          url.includes(".wasm") ||
          url.includes("/nav-db/") ||
          url.includes("/vectors/") ||
          url.includes("/plates/") ||
          url.includes("/afd/") ||
          url.includes("/thumbnails/")
        );
      })
      .map((entry) => ({
        name: entry.name,
        initiator_type: entry.initiatorType,
        start_time_ms: Math.round(entry.startTime),
        response_end_ms: Math.round(entry.responseEnd),
        duration_ms: Math.round(entry.duration),
        transfer_size: entry.transferSize,
        encoded_body_size: entry.encodedBodySize,
        decoded_body_size: entry.decodedBodySize,
      }))
      .sort((left, right) => left.start_time_ms - right.start_time_ms);
    debugLog("PERF_RESOURCES", { resources });
  }, 0);
}

logStartupTiming();
installPaintObservers();
logStartupResources();

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
