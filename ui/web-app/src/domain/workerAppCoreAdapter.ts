// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import type {
  AppCoreAdapter,
  LoadedAdapter,
  UiSessionSnapshot,
  UiInvalidation,
  UiInvalidationListener,
  UiSession,
} from "./appCoreAdapter";
import type { WorkerCreateUiSessionRequest } from "./appCoreWorkerProtocol";
import { debugLog, getBrowserInstanceId, isDebugLogEnabled, type DebugLogRecord } from "./debugLog";
import type { SituationRingCandidate } from "./types";

type WorkerCallTarget =
  | { kind: "adapter" }
  | { kind: "session"; sessionId: number };

type WorkerCallRequest = {
  kind: "call";
  id: number;
  sentAtEpochMs: number;
  browserInstanceId: string;
  debugLogEnabled: boolean;
  debugRunId?: string;
  target: WorkerCallTarget;
  method: string;
  args: unknown[];
};

type WorkerCallResponse =
  | { kind: "response"; id: number; ok: true; result: unknown; workerPostedAtEpochMs?: number; workerPostedAtMs?: number }
  | { kind: "response"; id: number; ok: false; error: WorkerErrorPayload; workerPostedAtEpochMs?: number; workerPostedAtMs?: number };

type WorkerResponseReady = {
  kind: "responseReady";
  id: number;
  target: WorkerCallTarget;
  method: string;
  workerReadyAtMs: number;
};

type WorkerSessionInvalidation = {
  kind: "sessionInvalidation";
  sessionId: number;
  invalidations: UiInvalidation[];
};

type WorkerCoreSettingsChanged = {
  kind: "coreSettingsChanged";
  settingsJson: string | null;
};

type WorkerDebugLog = {
  kind: "workerDebugLog";
  record: DebugLogRecord;
};

type WorkerMessage = WorkerCallResponse | WorkerResponseReady | WorkerSessionInvalidation | WorkerCoreSettingsChanged | WorkerDebugLog;

type WorkerErrorPayload = {
  name?: string;
  message: string;
  stack?: string;
};

type WorkerSessionMarker = {
  __aerobagWorkerSessionId: number;
  initialSnapshot: UiSessionSnapshot;
};

type SituationRingCandidateCache = SituationRingCandidate[];

type PendingWorkerCall = {
  target: WorkerCallTarget;
  method: string;
  startedAt: number;
  readyMarkerReceivedAt?: number;
  readyMarkerDelayMs?: number;
  resolve: (result: unknown) => void;
  reject: (error: Error) => void;
};

class AppCoreWorkerClient {
  private nextId = 1;
  private readonly pending = new Map<number, PendingWorkerCall>();
  private readonly sessionInvalidationListeners = new Map<number, UiInvalidationListener>();

  constructor(private readonly worker: Worker) {
    worker.addEventListener("message", (event: MessageEvent<WorkerMessage>) => {
      this.handleMessage(event.data);
    });
    worker.addEventListener("error", (event) => {
      this.rejectAll(new Error(`app-core worker error: ${event.message}`));
    });
    worker.addEventListener("messageerror", () => {
      this.rejectAll(new Error("app-core worker message clone failed"));
    });
  }

  callAdapter<T>(method: string, args: unknown[] = []): Promise<T> {
    return this.call<T>({ kind: "adapter" }, method, args);
  }

  callSession<T>(sessionId: number, method: string, args: unknown[] = []): Promise<T> {
    return this.call<T>({ kind: "session", sessionId }, method, args);
  }

  setSessionInvalidationListener(sessionId: number, listener: UiInvalidationListener | null): void {
    if (listener) {
      this.sessionInvalidationListeners.set(sessionId, listener);
    } else {
      this.sessionInvalidationListeners.delete(sessionId);
    }
  }

  destroy(): void {
    this.rejectAll(new Error("app-core worker destroyed"));
    this.worker.terminate();
  }

  private call<T>(target: WorkerCallTarget, method: string, args: unknown[]): Promise<T> {
    const id = this.nextId++;
    const message: WorkerCallRequest = {
      kind: "call",
      id,
      sentAtEpochMs: Date.now(),
      browserInstanceId: getBrowserInstanceId(),
      debugLogEnabled: isDebugLogEnabled(),
      ...(currentDebugRunId() ? { debugRunId: currentDebugRunId() ?? undefined } : {}),
      target,
      method,
      args,
    };
    return new Promise((resolve, reject) => {
      this.pending.set(id, {
        target,
        method,
        startedAt: performance.now(),
        resolve: (result) => resolve(result as T),
        reject,
      });
      this.worker.postMessage(message);
    });
  }

  private handleMessage(message: WorkerMessage): void {
    if (message.kind === "sessionInvalidation") {
      this.sessionInvalidationListeners.get(message.sessionId)?.(message.invalidations);
      return;
    }
    if (message.kind === "coreSettingsChanged") {
      writePersistedCoreSettingsJson(message.settingsJson);
      return;
    }
    if (message.kind === "workerDebugLog") {
      debugLog(`worker.${message.record.tag}`, {
        worker_seq: message.record.seq,
        worker_ts_ms: message.record.ts_ms,
        worker_run_id: message.record.run_id ?? null,
        data: message.record.data ?? null,
      });
      return;
    }
    if (message.kind === "responseReady") {
      this.handleResponseReady(message);
      return;
    }
    const pending = this.pending.get(message.id);
    if (!pending) {
      return;
    }
    this.pending.delete(message.id);
    logWorkerResponseReceived(message, pending);
    if (message.ok) {
      pending.resolve(message.result);
    } else {
      pending.reject(workerError(message.error));
    }
  }

  private handleResponseReady(message: WorkerResponseReady): void {
    const pending = this.pending.get(message.id);
    if (!pending) {
      return;
    }
    const receivedAt = performance.now();
    const delayMs = nowEpochishMs() - message.workerReadyAtMs;
    pending.readyMarkerReceivedAt = receivedAt;
    pending.readyMarkerDelayMs = delayMs;
    const important =
      pending.method === "queryMapOverlay"
      || pending.method === "snapshot"
      || delayMs >= 20;
    if (!important) {
      return;
    }
    debugLog("app_core.worker.response.ready_received", {
      id: message.id,
      target: pending.target.kind,
      method: pending.method,
      ready_to_receive_ms: Math.round(delayMs),
      target_matches: pending.target.kind === message.target.kind,
      method_matches: pending.method === message.method,
    });
  }

  private rejectAll(error: Error): void {
    for (const pending of this.pending.values()) {
      pending.reject(error);
    }
    this.pending.clear();
  }
}

function currentDebugRunId(): string | null {
  const candidate = (globalThis as unknown as { __aerobagPerfRunId?: unknown }).__aerobagPerfRunId;
  return typeof candidate === "string" && candidate.length > 0 ? candidate : null;
}

function logWorkerResponseReceived(
  message: WorkerCallResponse,
  pending: PendingWorkerCall,
): void {
  const roundTripMs = performance.now() - pending.startedAt;
  const postToReceiveMs = message.workerPostedAtMs === undefined
    ? message.workerPostedAtEpochMs === undefined
      ? null
      : Date.now() - message.workerPostedAtEpochMs
    : nowEpochishMs() - message.workerPostedAtMs;
  const markerToPayloadMs = pending.readyMarkerReceivedAt === undefined
    ? null
    : performance.now() - pending.readyMarkerReceivedAt;
  const summary = message.ok ? summarizeWorkerResponseResult(message.result) : {};
  const important =
    pending.method === "queryMapOverlay"
    || pending.method === "snapshot"
    || roundTripMs >= 100
    || (postToReceiveMs ?? 0) >= 20
    || !message.ok;
  if (!important) {
    return;
  }
  debugLog("app_core.worker.response.received", {
    id: message.id,
    target: pending.target.kind,
    method: pending.method,
    round_trip_ms: Math.round(roundTripMs),
    post_to_receive_ms: postToReceiveMs === null ? null : Math.round(postToReceiveMs),
    ready_to_receive_ms: pending.readyMarkerDelayMs === undefined ? null : Math.round(pending.readyMarkerDelayMs),
    marker_to_payload_ms: markerToPayloadMs === null ? null : Math.round(markerToPayloadMs),
    ok: message.ok,
    error: message.ok ? null : message.error,
    ...summary,
  });
}

function nowEpochishMs(): number {
  return performance.timeOrigin + performance.now();
}

function summarizeWorkerResponseResult(result: unknown): Record<string, unknown> {
  if (result instanceof Uint8Array) {
    return { result_kind: "bytes", byte_length: result.byteLength };
  }
  if (!result || typeof result !== "object") {
    return { result_kind: typeof result };
  }
  const record = result as Record<string, unknown>;
  if (Array.isArray(record.visible_features) || Array.isArray(record.visible_metars)) {
    return {
      result_kind: "map_overlay",
      visible_features: Array.isArray(record.visible_features) ? record.visible_features.length : null,
      visible_metars: Array.isArray(record.visible_metars) ? record.visible_metars.length : null,
      visible_pireps: Array.isArray(record.visible_pireps) ? record.visible_pireps.length : null,
      airspace_paths: Array.isArray(record.airspace_paths) ? record.airspace_paths.length : null,
      airspace_labels: Array.isArray(record.airspace_labels) ? record.airspace_labels.length : null,
    };
  }
  if ("app_ui_state" in record && "debug_state" in record) {
    return { result_kind: "session_snapshot" };
  }
  return { result_kind: "object" };
}

export async function loadWorkerBackedAdapter(): Promise<LoadedAdapter> {
  const worker = new Worker(new URL("./appCore.worker.ts", import.meta.url), {
    name: "aerobag-app-core",
    type: "module",
  });
  const client = new AppCoreWorkerClient(worker);
  await client.callAdapter("prewarm");
  return {
    adapter: workerBackedAdapter(client),
    backend: "wasm-worker",
    detail: "Using generated Rust WASM bindings in an app-core worker.",
  };
}

function workerBackedAdapter(client: AppCoreWorkerClient): AppCoreAdapter {
  let cachedSituationRingCandidates: SituationRingCandidateCache = [];
  let situationRingCandidatesPromise: Promise<SituationRingCandidateCache> | null = null;
  const loadSituationRingCandidates = () => {
    situationRingCandidatesPromise ??= client
      .callAdapter<SituationRingCandidateCache>("situationRingCandidates")
      .then((candidates) => {
        cachedSituationRingCandidates = candidates;
        return candidates;
      });
    return situationRingCandidatesPromise;
  };
  return {
    prewarm: () => client.callAdapter("prewarm"),
    situationRingCandidates: () => cachedSituationRingCandidates,
    loadSituationRingCandidates,
    createUiSession: async (recentAirportIds, selectedAirportId, selectedChartId) => {
      const request: WorkerCreateUiSessionRequest = {
        recentAirportIds,
        selectedAirportId,
        selectedChartId,
        settingsJson: readPersistedCoreSettingsJson(),
      };
      const marker = await client.callAdapter<WorkerSessionMarker>("createUiSession", [request]);
      return workerBackedSession(client, marker.__aerobagWorkerSessionId, marker.initialSnapshot);
    },
    resolveWaypointIdentifier: (...args) => client.callAdapter("resolveWaypointIdentifier", args),
    resolveNavRefPosition: (...args) => client.callAdapter("resolveNavRefPosition", args),
    suggestWaypointIdentifiersNear: (...args) => client.callAdapter("suggestWaypointIdentifiersNear", args),
    suggestAirwaysNearAnchor: (...args) => client.callAdapter("suggestAirwaysNearAnchor", args),
    listProcedures: (...args) => client.callAdapter("listProcedures", args),
    describeProcedureOptions: (...args) => client.callAdapter("describeProcedureOptions", args),
    findProcedurePlateMatch: (...args) => client.callAdapter("findProcedurePlateMatch", args),
  };
}

function workerBackedSession(client: AppCoreWorkerClient, sessionId: number, initialSnapshot: UiSessionSnapshot): UiSession {
  const call = <T>(method: string, args: unknown[] = []) => client.callSession<T>(sessionId, method, args);
  let latestSnapshot = initialSnapshot;
  const updateSnapshot = async (promise: Promise<UiSessionSnapshot>) => {
    latestSnapshot = await promise;
    return latestSnapshot;
  };
  return {
    setInvalidationListener: (listener) => client.setSessionInvalidationListener(sessionId, listener),
    initialSnapshot: () => latestSnapshot,
    snapshot: () => updateSnapshot(call("snapshot")),
    maintainNavDb: (...args) => updateSnapshot(call("maintainNavDb", args)),
    requestSessionSnapshotRefresh: (...args) => call("requestSessionSnapshotRefresh", args),
    sessionSnapshotViewportGestureActiveChanged: (...args) => call("sessionSnapshotViewportGestureActiveChanged", args),
    sessionSnapshotViewportActivity: () => call("sessionSnapshotViewportActivity"),
    sessionSnapshotRefreshCompleted: () => call("sessionSnapshotRefreshCompleted"),
    pollSessionSnapshotRefresh: () => call("pollSessionSnapshotRefresh"),
    deriveChartPageState: () => call("deriveChartPageState"),
    airportInfo: (...args) => call("airportInfo", args),
    insertWaypointAtFlightPlanRow: (...args) => updateSnapshot(call("insertWaypointAtFlightPlanRow", args)),
    suggestWaypointIdentifiersAtFlightPlanRow: (...args) => call("suggestWaypointIdentifiersAtFlightPlanRow", args),
    previewFlightPlanEntry: (...args) => call("previewFlightPlanEntry", args),
    appendFlightPlanEntry: (...args) => updateSnapshot(call("appendFlightPlanEntry", args)),
    prepareAirwayPresentationAtFlightPlanRow: (...args) => call("prepareAirwayPresentationAtFlightPlanRow", args),
    insertAirwayAtFlightPlanRow: (...args) => updateSnapshot(call("insertAirwayAtFlightPlanRow", args)),
    selectProcedureAtFlightPlanRow: (...args) => updateSnapshot(call("selectProcedureAtFlightPlanRow", args)),
    describePlateProcedureLoads: (...args) => call("describePlateProcedureLoads", args),
    loadPlateProcedure: (...args) => updateSnapshot(call("loadPlateProcedure", args)),
    restoreDirectTo: () => updateSnapshot(call("restoreDirectTo")),
    performFlightPlanRowAction: (...args) => updateSnapshot(call("performFlightPlanRowAction", args)),
    altitudeComparisons: () => call("altitudeComparisons"),
    performAltitudePlannerAction: (...args) => updateSnapshot(call("performAltitudePlannerAction", args)),
    performStatusAction: (...args) => updateSnapshot(call("performStatusAction", args)),
    performMapSelectionAction: (...args) => updateSnapshot(call("performMapSelectionAction", args)),
    activateNextLeg: () => updateSnapshot(call("activateNextLeg")),
    stopNavigation: () => updateSnapshot(call("stopNavigation")),
    suspendSequencing: () => updateSnapshot(call("suspendSequencing")),
    unsuspendSequencing: () => updateSnapshot(call("unsuspendSequencing")),
    sequenceActiveLeg: () => updateSnapshot(call("sequenceActiveLeg")),
    setSituation: (...args) => updateSnapshot(call("setSituation", args)),
    tickBadAutopilot: (...args) => updateSnapshot(call("tickBadAutopilot", args)),
    loadPlaybackTrace: (...args) => updateSnapshot(call("loadPlaybackTrace", args)),
    playPlayback: (...args) => updateSnapshot(call("playPlayback", args)),
    pausePlayback: (...args) => updateSnapshot(call("pausePlayback", args)),
    seekPlayback: (...args) => updateSnapshot(call("seekPlayback", args)),
    setPlaybackRate: (...args) => updateSnapshot(call("setPlaybackRate", args)),
    tickPlayback: (...args) => updateSnapshot(call("tickPlayback", args)),
    engageMapFollow: (...args) => updateSnapshot(call("engageMapFollow", args)),
    disengageMapFollow: (...args) => updateSnapshot(call("disengageMapFollow", args)),
    setMapFollowOffset: (...args) => updateSnapshot(call("setMapFollowOffset", args)),
    syncMapFollow: (...args) => updateSnapshot(call("syncMapFollow", args)),
    registerOwnshipSource: (...args) => updateSnapshot(call("registerOwnshipSource", args)),
    updateOwnshipSourceStatus: (...args) => updateSnapshot(call("updateOwnshipSourceStatus", args)),
    pushSituationSample: (...args) => updateSnapshot(call("pushSituationSample", args)),
    selectOwnshipSource: (...args) => updateSnapshot(call("selectOwnshipSource", args)),
    performOwnshipTextAction: (...args) => updateSnapshot(call("performOwnshipTextAction", args)),
    applySituationControlInput: (...args) => updateSnapshot(call("applySituationControlInput", args)),
    setMapLayerVisibility: (...args) => updateSnapshot(call("setMapLayerVisibility", args)),
    setMapLayerEnabled: (...args) => updateSnapshot(call("setMapLayerEnabled", args)),
    setDebugFlag: (...args) => updateSnapshot(call("setDebugFlag", args)),
    performSettingsAction: (...args) => updateSnapshot(call("performSettingsAction", args)),
    takeCloudAuthorizationRequest: (...args) => call("takeCloudAuthorizationRequest", args),
    completeCloudAuthorization: (...args) => updateSnapshot(call("completeCloudAuthorization", args)),
    performCloudUiAction: (...args) => updateSnapshot(call("performCloudUiAction", args)),
    recordOfflinePackagePreferences: (...args) => updateSnapshot(call("recordOfflinePackagePreferences", args)),
    takeCloudProviderRequest: (...args) => call("takeCloudProviderRequest", args),
    completeCloudProviderRequest: (...args) => updateSnapshot(call("completeCloudProviderRequest", args)),
    cloudEventStreamPlan: (...args) => call("cloudEventStreamPlan", args),
    reportCloudEventStreamEvent: (...args) => updateSnapshot(call("reportCloudEventStreamEvent", args)),
    acceptDisclaimer: (...args) => updateSnapshot(call("acceptDisclaimer", args)),
    loadRasterMapCatalog: () => updateSnapshot(call("loadRasterMapCatalog")),
    resolveChartAssetUrl: (...args) => call("resolveChartAssetUrl", args),
    selectMapFamily: (...args) => updateSnapshot(call("selectMapFamily", args)),
    selectRasterMap: (...args) => updateSnapshot(call("selectRasterMap", args)),
    selectAirport: (...args) => updateSnapshot(call("selectAirport", args)),
    selectChart: (...args) => updateSnapshot(call("selectChart", args)),
    selectChartReference: (...args) => updateSnapshot(call("selectChartReference", args)),
    ingestPointTiles: (...args) => call("ingestPointTiles", args),
    ingestAirspaceRefTiles: (...args) => call("ingestAirspaceRefTiles", args),
    ingestAirspaceFeatures: (...args) => call("ingestAirspaceFeatures", args),
    ingestAirspaceLabelTiles: (...args) => call("ingestAirspaceLabelTiles", args),
    queryMapOverlay: (...args) => call("queryMapOverlay", args),
    queryMapSelection: (...args) => call("queryMapSelection", args),
    queryMapSelectionForNavRef: (...args) => call("queryMapSelectionForNavRef", args),
    queryTerrainOverlay: (...args) => call("queryTerrainOverlay", args),
    queryNexradOverlay: (...args) => call("queryNexradOverlay", args),
    queryRasterTilePlan: (...args) => call("queryRasterTilePlan", args),
    renderTerrainOverlayTileByKey: (...args) => call("renderTerrainOverlayTileByKey", args),
    projectFlightPlanRoute: () => call("projectFlightPlanRoute"),
    syncLiveFeeds: () => call("syncLiveFeeds"),
    startLiveFeedSubscription: () => call("startLiveFeedSubscription"),
    notifyLiveFeedOnline: () => {
      void call("notifyLiveFeedOnline");
    },
    stopLiveFeedSubscription: () => call("stopLiveFeedSubscription"),
    ingestLiveFeedSseEvent: (...args) => call("ingestLiveFeedSseEvent", args),
    ingestLiveFeedSseEvents: (...args) => call("ingestLiveFeedSseEvents", args),
    restoreChartPageState: (...args) => updateSnapshot(call("restoreChartPageState", args)),
    destroy: async () => {
      client.setSessionInvalidationListener(sessionId, null);
      await call("destroy");
    },
  };
}

function workerError(payload: WorkerErrorPayload): Error {
  const error = new Error(payload.message);
  error.name = payload.name ?? "Error";
  if (payload.stack) {
    error.stack = payload.stack;
  }
  return error;
}

const webCoreSettingsStorageKey = "aerobag.core.settings.v1";

function readPersistedCoreSettingsJson(): string | null {
  try {
    return window.localStorage.getItem(webCoreSettingsStorageKey);
  } catch {
    return null;
  }
}

function writePersistedCoreSettingsJson(settingsJson: string | null): void {
  try {
    if (settingsJson === null || settingsJson.length === 0) {
      window.localStorage.removeItem(webCoreSettingsStorageKey);
      return;
    }
    window.localStorage.setItem(webCoreSettingsStorageKey, settingsJson);
  } catch {
    // Losing a persistence write should not make the live session unusable.
  }
}
