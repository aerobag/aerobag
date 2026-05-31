import { debugLog, flushDebugLogNow, observeDebugLog, setBrowserInstanceId, type DebugLogRecord } from "./domain/debugLog";

type StartupReloadHarness = {
  active: boolean;
  runId: string;
  sampleIndex: number;
  samples: number;
};

type StartupMilestones = {
  startMs: number | null;
  adapterReadyMs: number | null;
  sessionReadyMs: number | null;
  shellHideMs: number | null;
  firstRasterMs: number | null;
  firstOverlayMs: number | null;
  pointVectorsMs: number | null;
  airspaceVectorsMs: number | null;
  fullVectorsMs: number | null;
  errors: number;
};

const defaultSamples = 6;
const defaultTimeoutMs = 12000;

export function installStartupReloadHarness(): StartupReloadHarness | null {
  if (typeof window === "undefined") {
    return null;
  }
  const params = new URLSearchParams(window.location.search);
  if (!params.has("startupReloadHarness")) {
    return null;
  }

  const runId = params.get("startupRunId") || createRunId();
  const samples = positiveInt(params.get("startupSamples"), defaultSamples);
  const sampleIndex = nonNegativeInt(params.get("startupSampleIndex"), 0);
  const timeoutMs = positiveInt(params.get("startupTimeoutMs"), defaultTimeoutMs);
  const browserInstanceId = `startup-${runId}-${sampleIndex + 1}`;
  setBrowserInstanceId(browserInstanceId);
  (globalThis as unknown as { __aerobagPerfRunId?: string }).__aerobagPerfRunId = runId;

  const harness = { active: true, runId, sampleIndex, samples };
  const milestones: StartupMilestones = {
    startMs: null,
    adapterReadyMs: null,
    sessionReadyMs: null,
    shellHideMs: null,
    firstRasterMs: null,
    firstOverlayMs: null,
    pointVectorsMs: null,
    airspaceVectorsMs: null,
    fullVectorsMs: null,
    errors: 0,
  };

  let finished = false;
  let stopObserving: (() => void) | null = null;

  const finishTimer = window.setTimeout(() => {
    void finishSample("timeout");
  }, timeoutMs);

  stopObserving = observeDebugLog((record) => {
    if (finished) {
      return;
    }
    updateMilestones(milestones, record);
    if (sampleLooksComplete(milestones)) {
      void finishSample("complete");
    }
  });

  debugLog("startup.reload_harness.sample.begin", {
    run_id: runId,
    sample_index: sampleIndex + 1,
    samples,
    timeout_ms: timeoutMs,
    browser_instance_id: browserInstanceId,
    href: window.location.href,
  });

  async function finishSample(reason: string) {
    if (finished) {
      return;
    }
    finished = true;
    window.clearTimeout(finishTimer);
    stopObserving?.();
    debugLog("startup.reload_harness.sample.done", {
      run_id: runId,
      sample_index: sampleIndex + 1,
      samples,
      reason,
      milestones,
    });
    await flushDebugLogNow();
    if (sampleIndex + 1 >= samples) {
      debugLog("startup.reload_harness.done", {
        run_id: runId,
        samples,
      });
      await flushDebugLogNow();
      window.history.replaceState(null, "", withoutHarnessParams(window.location.href));
      return;
    }
    const nextUrl = new URL(window.location.href);
    nextUrl.searchParams.set("startupReloadHarness", "1");
    nextUrl.searchParams.set("startupRunId", runId);
    nextUrl.searchParams.set("startupSamples", String(samples));
    nextUrl.searchParams.set("startupSampleIndex", String(sampleIndex + 1));
    nextUrl.searchParams.set("startupCacheBust", `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`);
    window.history.replaceState(null, "", nextUrl);
    (window.location.reload as unknown as (forceGet?: boolean) => void)(true);
  }

  return harness;
}

function updateMilestones(milestones: StartupMilestones, record: DebugLogRecord) {
  if (record.tag === "START") {
    milestones.startMs ??= record.ts_ms;
    return;
  }
  if (record.tag === "app_core.adapter.preload.done") {
    milestones.adapterReadyMs ??= record.ts_ms;
    return;
  }
  if (record.tag === "startup.session.create.done") {
    milestones.sessionReadyMs ??= record.ts_ms;
    return;
  }
  if (record.tag === "startup.shell.hide") {
    milestones.shellHideMs ??= record.ts_ms;
    return;
  }
  if (
    record.tag === "WINDOW_ERROR"
    || record.tag === "UNHANDLED_REJECTION"
    || record.tag === "window.error"
    || record.tag === "window.unhandledrejection"
    || record.tag === "startup.fatal"
  ) {
    milestones.errors += 1;
    return;
  }
  if (record.tag === "map.raster.plan.after_paint") {
    const data = asRecord(record.data);
    if (numberField(data, "tiles") > 0) {
      milestones.firstRasterMs ??= record.ts_ms;
    }
    return;
  }
  if (record.tag !== "map.overlay.query.after_paint") {
    return;
  }
  const data = asRecord(record.data);
  const visibleFeatures = numberField(data, "visible_features");
  const visibleMetars = numberField(data, "visible_metars");
  const visiblePireps = numberField(data, "visible_pireps");
  const tfrPaths = numberField(data, "tfr_paths");
  const airspacePaths = numberField(data, "airspace_paths");
  const airspaceLabels = numberField(data, "airspace_labels");
  const anyOverlay = visibleFeatures + visibleMetars + visiblePireps + tfrPaths + airspacePaths + airspaceLabels > 0;
  if (anyOverlay) {
    milestones.firstOverlayMs ??= record.ts_ms;
  }
  if (visibleFeatures + visibleMetars + visiblePireps + tfrPaths > 0) {
    milestones.pointVectorsMs ??= record.ts_ms;
  }
  if (airspacePaths + airspaceLabels > 0) {
    milestones.airspaceVectorsMs ??= record.ts_ms;
  }
  if (visibleMetars > 0 && airspacePaths + airspaceLabels > 0) {
    milestones.fullVectorsMs ??= record.ts_ms;
  }
}

function sampleLooksComplete(milestones: StartupMilestones): boolean {
  return (
    milestones.firstRasterMs !== null
    && milestones.pointVectorsMs !== null
    && milestones.airspaceVectorsMs !== null
    && milestones.fullVectorsMs !== null
  );
}

function asRecord(value: unknown): Record<string, unknown> {
  return value && typeof value === "object" ? value as Record<string, unknown> : {};
}

function numberField(record: Record<string, unknown>, field: string): number {
  const value = record[field];
  return typeof value === "number" && Number.isFinite(value) ? value : 0;
}

function positiveInt(value: string | null, fallback: number): number {
  if (value === null) {
    return fallback;
  }
  const parsed = Number.parseInt(value, 10);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : fallback;
}

function nonNegativeInt(value: string | null, fallback: number): number {
  if (value === null) {
    return fallback;
  }
  const parsed = Number.parseInt(value, 10);
  return Number.isFinite(parsed) && parsed >= 0 ? parsed : fallback;
}

function createRunId(): string {
  const cryptoApi = (globalThis as unknown as { crypto?: { randomUUID?: () => string } }).crypto;
  if (cryptoApi?.randomUUID) {
    return cryptoApi.randomUUID().slice(0, 8);
  }
  return `run-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;
}

function withoutHarnessParams(href: string): string {
  const url = new URL(href);
  for (const key of [
    "startupReloadHarness",
    "startupRunId",
    "startupSamples",
    "startupSampleIndex",
    "startupTimeoutMs",
    "startupCacheBust",
  ]) {
    url.searchParams.delete(key);
  }
  return `${url.pathname}${url.search}${url.hash}`;
}
