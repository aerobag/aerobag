import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { loadBestAvailableAdapter } from "./domain/appCoreAdapter";
import { debugLog, isDebugLogEnabled, perfDebugLog, VERBOSE_PERF_DEBUG_LOGS } from "./domain/debugLog";
import { installStartupReloadHarness } from "./startupReloadHarness";
import "./styles.css";

declare global {
  interface AerobagStartupCacheWarmFetch {
    url: string;
    ok: boolean;
    status: number;
    elapsed_ms: number | null;
    error?: string;
  }

  interface AerobagStartupCacheWarmState {
    started_at_ms: number;
    done_at_ms?: number;
    elapsed_ms?: number;
    status: string;
    status_code?: number;
    resource_count: number;
    resource_urls: string[];
    fetches: AerobagStartupCacheWarmFetch[];
    failed_count?: number;
    error?: string;
    promise?: Promise<AerobagStartupCacheWarmState>;
  }

  interface Window {
    __aerobag_html_start?: number;
    __aerobag_startup_cache_warm?: AerobagStartupCacheWarmState;
    __aerobag_hide_startup_shell?: (reason?: string) => void;
    __aerobag_startup_elapsed_interval?: number;
    __aerobag_startup_watchdog?: number;
    __aerobag_mark_startup_shell_managed?: () => void;
    __aerobag_show_startup_shell_error?: (message: string, detail?: string) => void;
  }
}

let startupShellHideLogged = false;

function dismissStartupShell(reason = "unspecified") {
  const shell = document.getElementById("startup-shell");
  if (!startupShellHideLogged) {
    startupShellHideLogged = true;
    debugLog("startup.shell.hide", {
      reason,
      had_shell: shell !== null,
      html_start_ms: typeof window !== "undefined" ? window.__aerobag_html_start ?? null : null,
      elapsed_ms: Math.round(performance.now()),
    });
  }
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

function startupCacheWarmSummary(state: AerobagStartupCacheWarmState) {
  const successfulFetches = state.fetches.filter((fetchResult) => fetchResult.ok);
  const elapsedValues = successfulFetches
    .map((fetchResult) => fetchResult.elapsed_ms)
    .filter((elapsedMs): elapsedMs is number => elapsedMs !== null);
  return {
    started_at_ms: state.started_at_ms,
    done_at_ms: state.done_at_ms ?? null,
    elapsed_ms: state.elapsed_ms ?? null,
    status: state.status,
    resource_count: state.resource_count,
    fetched_count: successfulFetches.length,
    failed_count: state.failed_count ?? state.fetches.length - successfulFetches.length,
    max_fetch_elapsed_ms: elapsedValues.length > 0 ? Math.max(...elapsedValues) : null,
    resource_urls: state.resource_urls,
    fetches: state.fetches,
    error: state.error ?? null,
    status_code: state.status_code ?? null,
  };
}

function logStartupCacheWarm() {
  const cacheWarm = window.__aerobag_startup_cache_warm;
  if (!cacheWarm) {
    debugLog("startup.cache_warm.missing");
    return;
  }
  debugLog("startup.cache_warm.started", startupCacheWarmSummary(cacheWarm));
  void cacheWarm.promise?.then((state) => {
    debugLog("startup.cache_warm.done", startupCacheWarmSummary(state));
  });
}

function installPaintObservers() {
  if (!VERBOSE_PERF_DEBUG_LOGS || typeof PerformanceObserver === "undefined") {
    return;
  }
  try {
    const observer = new PerformanceObserver((list) => {
      for (const entry of list.getEntries()) {
        perfDebugLog("PERF_PAINT", () => ({
          name: entry.name,
          start_time_ms: Math.round(entry.startTime),
          duration_ms: Math.round(entry.duration),
        }));
      }
    });
    observer.observe({ type: "paint", buffered: true });
  } catch {
    // Paint entries may be unavailable in some browser modes.
  }
}

function installLongTaskObserver() {
  if (!VERBOSE_PERF_DEBUG_LOGS || typeof PerformanceObserver === "undefined") {
    return;
  }
  if (!PerformanceObserver.supportedEntryTypes?.includes("longtask")) {
    return;
  }
  try {
    const observer = new PerformanceObserver((list) => {
      for (const entry of list.getEntries()) {
        perfDebugLog("PERF_LONG_TASK", () => ({
          name: entry.name,
          start_time_ms: Math.round(entry.startTime),
          duration_ms: Math.round(entry.duration),
        }));
      }
    });
    observer.observe({ type: "longtask", buffered: true });
  } catch {
    // Long task entries are not available in all browsers.
  }
}

function installEventLoopLagMonitor() {
  if (!VERBOSE_PERF_DEBUG_LOGS || typeof window === "undefined") {
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
    perfDebugLog("PERF_EVENT_LOOP_LAG", () => ({
      lag_ms: Math.round(lagMs),
      now_ms: Math.round(now),
    }));
  }, 500);
}

function logStartupResources() {
  if (!isDebugLogEnabled()) {
    return;
  }
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

const startupReloadHarness = installStartupReloadHarness();
logStartupTiming();
logStartupCacheWarm();
if (startupReloadHarness?.active) {
  debugLog("startup.reload_harness.installed", {
    run_id: startupReloadHarness.runId,
    sample_index: startupReloadHarness.sampleIndex + 1,
    samples: startupReloadHarness.samples,
  });
}
installPaintObservers();
installLongTaskObserver();
installEventLoopLagMonitor();
logStartupResources();

function preloadAppCoreAdapter() {
  const startedAt = performance.now();
  debugLog("app_core.adapter.preload.start");
  void loadBestAvailableAdapter().then((loaded) => {
    debugLog("app_core.adapter.preload.done", {
      backend: loaded.backend,
      elapsed_ms: Math.round(performance.now() - startedAt),
    });
  }).catch((error) => {
    debugLog("app_core.adapter.preload.failed", {
      elapsed_ms: Math.round(performance.now() - startedAt),
      message: error instanceof Error ? error.message : String(error),
    });
  });
}

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
  if (window.location.pathname === "/metar-bakeoff") {
    void import("./metarBakeoff").then(({ runMetarBakeoff }) => runMetarBakeoff(rootNode));
  } else {
    preloadAppCoreAdapter();
    ReactDOM.createRoot(rootNode).render(
      <React.StrictMode>
        <App />
      </React.StrictMode>,
    );
  }
  window.__aerobag_mark_startup_shell_managed?.();
} catch (error) {
  const detail = error instanceof Error ? error.message : String(error);
  window.__aerobag_show_startup_shell_error?.("Startup failed", detail);
  throw error;
}
