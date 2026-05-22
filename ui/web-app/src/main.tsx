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
    __aerobag_startup_watchdog?: number;
    __aerobag_mark_startup_shell_managed?: () => void;
    __aerobag_show_startup_shell_error?: (message: string, detail?: string) => void;
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
  if (typeof window !== "undefined" && window.__aerobag_startup_watchdog != null) {
    window.clearTimeout(window.__aerobag_startup_watchdog);
    window.__aerobag_startup_watchdog = undefined;
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

function installLongTaskObserver() {
  if (typeof PerformanceObserver === "undefined") {
    return;
  }
  if (!PerformanceObserver.supportedEntryTypes?.includes("longtask")) {
    return;
  }
  try {
    const observer = new PerformanceObserver((list) => {
      for (const entry of list.getEntries()) {
        debugLog("PERF_LONG_TASK", {
          name: entry.name,
          start_time_ms: Math.round(entry.startTime),
          duration_ms: Math.round(entry.duration),
        });
      }
    });
    observer.observe({ type: "longtask", buffered: true });
  } catch {
    // Long task entries are not available in all browsers.
  }
}

function installEventLoopLagMonitor() {
  if (typeof window === "undefined") {
    return;
  }
  let expectedAt = performance.now() + 500;
  window.setInterval(() => {
    const now = performance.now();
    const lagMs = now - expectedAt;
    expectedAt = now + 500;
    if (lagMs < 250) {
      return;
    }
    debugLog("PERF_EVENT_LOOP_LAG", {
      lag_ms: Math.round(lagMs),
      now_ms: Math.round(now),
    });
  }, 500);
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
          url.includes("/packages/")
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
installLongTaskObserver();
installEventLoopLagMonitor();
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

try {
  const rootNode = document.getElementById("root");
  if (!rootNode) {
    throw new Error("Missing #root element");
  }
  ReactDOM.createRoot(rootNode).render(
    <React.StrictMode>
      <App />
    </React.StrictMode>,
  );
  window.__aerobag_mark_startup_shell_managed?.();
} catch (error) {
  const detail = error instanceof Error ? error.message : String(error);
  window.__aerobag_show_startup_shell_error?.("Startup failed", detail);
  throw error;
}
