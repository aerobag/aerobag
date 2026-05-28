import type {
  AppCoreAdapter,
  LoadedAdapter,
  UiInvalidation,
  UiInvalidationListener,
  UiSession,
} from "./appCoreAdapter";

type WorkerCallTarget =
  | { kind: "adapter" }
  | { kind: "session"; sessionId: number };

type WorkerCallRequest = {
  kind: "call";
  id: number;
  sentAtEpochMs: number;
  target: WorkerCallTarget;
  method: string;
  args: unknown[];
};

type WorkerCallResponse =
  | { kind: "response"; id: number; ok: true; result: unknown }
  | { kind: "response"; id: number; ok: false; error: WorkerErrorPayload };

type WorkerSessionInvalidation = {
  kind: "sessionInvalidation";
  sessionId: number;
  invalidations: UiInvalidation[];
};

type WorkerMessage = WorkerCallResponse | WorkerSessionInvalidation;

type WorkerErrorPayload = {
  name?: string;
  message: string;
  stack?: string;
};

type WorkerSessionMarker = {
  __aerobagWorkerSessionId: number;
};

class AppCoreWorkerClient {
  private nextId = 1;
  private readonly pending = new Map<number, {
    resolve: (result: unknown) => void;
    reject: (error: Error) => void;
  }>();
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
      target,
      method,
      args,
    };
    return new Promise((resolve, reject) => {
      this.pending.set(id, {
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
    const pending = this.pending.get(message.id);
    if (!pending) {
      return;
    }
    this.pending.delete(message.id);
    if (message.ok) {
      pending.resolve(message.result);
    } else {
      pending.reject(workerError(message.error));
    }
  }

  private rejectAll(error: Error): void {
    for (const pending of this.pending.values()) {
      pending.reject(error);
    }
    this.pending.clear();
  }
}

export async function loadWorkerBackedAdapter(): Promise<LoadedAdapter> {
  const worker = new Worker(new URL("./appCore.worker.ts", import.meta.url), {
    name: "aerobag-app-core",
    type: "module",
  });
  const client = new AppCoreWorkerClient(worker);
  const situationRingCandidates = await client.callAdapter<ReturnType<AppCoreAdapter["situationRingCandidates"]>>(
    "situationRingCandidates",
  );
  return {
    adapter: workerBackedAdapter(client, situationRingCandidates),
    backend: "wasm-worker",
    detail: "Using generated Rust WASM bindings in an app-core worker.",
  };
}

function workerBackedAdapter(
  client: AppCoreWorkerClient,
  situationRingCandidates: ReturnType<AppCoreAdapter["situationRingCandidates"]>,
): AppCoreAdapter {
  return {
    prewarm: () => client.callAdapter("prewarm"),
    situationRingCandidates: () => situationRingCandidates,
    emptyFlightPlan: () => client.callAdapter("emptyFlightPlan"),
    createUiSession: async (...args) => {
      const marker = await client.callAdapter<WorkerSessionMarker>("createUiSession", args);
      return workerBackedSession(client, marker.__aerobagWorkerSessionId);
    },
    deriveChartPageState: (...args) => client.callAdapter("deriveChartPageState", args),
    resolveWaypointIdentifier: (...args) => client.callAdapter("resolveWaypointIdentifier", args),
    resolveNavRefPosition: (...args) => client.callAdapter("resolveNavRefPosition", args),
    suggestWaypointIdentifiersNear: (...args) => client.callAdapter("suggestWaypointIdentifiersNear", args),
    suggestAirwaysNearAnchor: (...args) => client.callAdapter("suggestAirwaysNearAnchor", args),
    prepareAirwayPresentationForAnchors: (...args) => client.callAdapter("prepareAirwayPresentationForAnchors", args),
    listProcedures: (...args) => client.callAdapter("listProcedures", args),
    describeProcedureOptions: (...args) => client.callAdapter("describeProcedureOptions", args),
    findProcedurePlateMatch: (...args) => client.callAdapter("findProcedurePlateMatch", args),
    describePlateProcedureLoads: (...args) => client.callAdapter("describePlateProcedureLoads", args),
  };
}

function workerBackedSession(client: AppCoreWorkerClient, sessionId: number): UiSession {
  const call = <T>(method: string, args: unknown[] = []) => client.callSession<T>(sessionId, method, args);
  return {
    setInvalidationListener: (listener) => client.setSessionInvalidationListener(sessionId, listener),
    snapshot: () => call("snapshot"),
    insertWaypointAtFlightPlanRow: (...args) => call("insertWaypointAtFlightPlanRow", args),
    suggestWaypointIdentifiersAtFlightPlanRow: (...args) => call("suggestWaypointIdentifiersAtFlightPlanRow", args),
    previewFlightPlanEntry: (...args) => call("previewFlightPlanEntry", args),
    appendFlightPlanEntry: (...args) => call("appendFlightPlanEntry", args),
    insertAirwayAtFlightPlanRow: (...args) => call("insertAirwayAtFlightPlanRow", args),
    selectProcedureAtFlightPlanRow: (...args) => call("selectProcedureAtFlightPlanRow", args),
    loadPlateProcedure: (...args) => call("loadPlateProcedure", args),
    restoreDirectTo: () => call("restoreDirectTo"),
    performFlightPlanRowAction: (...args) => call("performFlightPlanRowAction", args),
    performStatusAction: (...args) => call("performStatusAction", args),
    performMapSelectionAction: (...args) => call("performMapSelectionAction", args),
    activateNextLeg: () => call("activateNextLeg"),
    suspendSequencing: () => call("suspendSequencing"),
    unsuspendSequencing: () => call("unsuspendSequencing"),
    sequenceActiveLeg: () => call("sequenceActiveLeg"),
    setSituation: (...args) => call("setSituation", args),
    tickDebugOwnshipDriver: (...args) => call("tickDebugOwnshipDriver", args),
    loadPlaybackTrace: (...args) => call("loadPlaybackTrace", args),
    playPlayback: (...args) => call("playPlayback", args),
    pausePlayback: (...args) => call("pausePlayback", args),
    seekPlayback: (...args) => call("seekPlayback", args),
    setPlaybackRate: (...args) => call("setPlaybackRate", args),
    tickPlayback: (...args) => call("tickPlayback", args),
    engageMapFollow: (...args) => call("engageMapFollow", args),
    disengageMapFollow: (...args) => call("disengageMapFollow", args),
    setMapFollowOffset: (...args) => call("setMapFollowOffset", args),
    syncMapFollow: (...args) => call("syncMapFollow", args),
    registerOwnshipSource: (...args) => call("registerOwnshipSource", args),
    updateOwnshipSourceStatus: (...args) => call("updateOwnshipSourceStatus", args),
    pushSituationSample: (...args) => call("pushSituationSample", args),
    selectOwnshipSource: (...args) => call("selectOwnshipSource", args),
    applySituationControlInput: (...args) => call("applySituationControlInput", args),
    setMapLayerVisibility: (...args) => call("setMapLayerVisibility", args),
    setMapLayerEnabled: (...args) => call("setMapLayerEnabled", args),
    setDebugFlag: (...args) => call("setDebugFlag", args),
    loadRasterMapCatalog: () => call("loadRasterMapCatalog"),
    selectMapFamily: (...args) => call("selectMapFamily", args),
    selectRasterMap: (...args) => call("selectRasterMap", args),
    selectAirport: (...args) => call("selectAirport", args),
    selectChart: (...args) => call("selectChart", args),
    ingestPointTiles: (...args) => call("ingestPointTiles", args),
    ingestAirspaceRefTiles: (...args) => call("ingestAirspaceRefTiles", args),
    ingestAirspaceFeatures: (...args) => call("ingestAirspaceFeatures", args),
    ingestAirspaceLabelTiles: (...args) => call("ingestAirspaceLabelTiles", args),
    queryMapOverlay: (...args) => call("queryMapOverlay", args),
    queryMapSelection: (...args) => call("queryMapSelection", args),
    queryTerrainOverlay: (...args) => call("queryTerrainOverlay", args),
    queryNexradOverlay: (...args) => call("queryNexradOverlay", args),
    queryRasterTilePlan: (...args) => call("queryRasterTilePlan", args),
    renderTerrainOverlayTileByKey: (...args) => call("renderTerrainOverlayTileByKey", args),
    projectFlightPlanRoute: () => call("projectFlightPlanRoute"),
    syncLiveFeeds: () => call("syncLiveFeeds"),
    startLiveFeedSubscription: () => call("startLiveFeedSubscription"),
    stopLiveFeedSubscription: () => call("stopLiveFeedSubscription"),
    ingestLiveFeedSseEvent: (...args) => call("ingestLiveFeedSseEvent", args),
    ingestLiveFeedSseEvents: (...args) => call("ingestLiveFeedSseEvents", args),
    restoreChartPageState: (...args) => call("restoreChartPageState", args),
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
