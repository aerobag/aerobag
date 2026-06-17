import { debugLog, debugTiming, installRustDebugLogBridge } from "./debugLog";

type NavKvWasmModule = {
  default?: (moduleOrPath?: string | URL | Request) => Promise<unknown>;
  attach_nav_kv_store_to_session(handle: number, sessionHandle: number): void;
  core_had_operation(handle: number, operationJson: string): string;
  drain_session_resource_effects(sessionHandle: number): Promise<string> | string;
  ingest_resource_in_session(handle: number, resourceId: string, resourceBytes: Uint8Array): Promise<void> | void;
  install_rust_debug_logger(): void;
  nav_db_open_controller_create(candidatesJson: string): number;
  nav_db_open_controller_destroy(handle: number): void;
  nav_db_open_controller_finish(handle: number): Promise<string> | string;
  nav_db_open_controller_ingest_resource(handle: number, resourceId: string, resourceBytes: Uint8Array): Promise<void> | void;
  nav_db_open_controller_step(handle: number): Promise<string> | string;
  nav_kv_destroy(handle: number): void;
  nav_kv_insert_resource(handle: number, resourceId: string, resourceBytes: Uint8Array): Promise<void> | void;
  nav_kv_prefetch_pages(handle: number): Promise<string> | string;
  resolve_metar_manifest_in_session(handle: number): Promise<string> | string;
  resolve_nav_db_artifact_candidates_in_session(handle: number): Promise<string> | string;
  resolve_chart_asset_resource_in_session(handle: number, chartId: string, assetKind: string): Promise<string> | string;
};

export type UiInvalidation =
  | "session_snapshot"
  | "raster_tiles"
  | "map_overlay"
  | "nexrad_overlay"
  | "terrain_overlay"
  | "flight_plan_route"
  | "debug_panel";

export type UiInvalidationListener = (invalidations: UiInvalidation[]) => void;
export type ResourceFailureReporter = (resourceId: string, message: string) => Promise<void> | void;

type PendingNavKvPageInsert = {
  resourceId: string;
  pageIndex: number;
  bytes: Uint8Array;
  priority: number;
  resolve: () => void;
  reject: (error: unknown) => void;
};

let wasmReady: Promise<NavKvWasmModule> | null = null;
let sharedNavKvStorePromise: Promise<NavKvStore | null> | null = null;

async function ensureWasmReady(): Promise<NavKvWasmModule> {
  if (!wasmReady) {
    installRustDebugLogBridge();
    wasmReady = import("@generated/app_wasm.js").then(async (mod) => {
      const wasm = mod as unknown as NavKvWasmModule;
      await wasm.default?.();
      wasm.install_rust_debug_logger();
      return wasm;
    });
  }
  return wasmReady;
}

export class NavKvStore {
  private readonly inFlightPageFetches = new Map<number, Promise<void>>();
  private readonly pendingPageInserts = new Map<number, PendingNavKvPageInsert>();
  private readonly pageRequestPriorities = new Map<number, number>();
  private pageInsertPumpActive = false;
  private pageRequestSequence = 0;

  private constructor(
    private readonly wasm: NavKvWasmModule,
    private readonly handle: number,
    navKvRootUrl: string,
  ) {
    this.navKvPackageRoot = navKvRootUrl.replace(/\/root(?:[?#].*)?$/, "");
  }

  static async open(sessionHandle: number): Promise<NavKvStore | null> {
    const wasm = await debugTiming("nav_kv.wasm.init", () => ensureWasmReady());
    const candidates = await debugTiming(
      "nav_kv.open.resolve_candidates",
      () => resolveSessionPublicationResult<NavDbArtifactCandidate[]>(
        wasm,
        sessionHandle,
        () => wasm.resolve_nav_db_artifact_candidates_in_session(sessionHandle),
      ),
    );
    if (candidates.length === 0) {
      debugLog("nav_kv.root.missing", { reason: "publication has no nav-db candidates" });
      return null;
    }
    const byFilename = new Map(candidates.map((candidate) => [candidate.filename, candidate]));
    const controllerHandle = debugTiming("nav_kv.open.controller_create", () =>
      wasm.nav_db_open_controller_create(JSON.stringify(candidates)), {
        candidate_count: candidates.length,
      });
    let finished = false;
    let finish: NavDbOpenFinish | null = null;
    let iteration = 0;
    try {
      for (;;) {
        iteration += 1;
        const stepStartedAt = performance.now();
        const responseJson = await wasm.nav_db_open_controller_step(controllerHandle);
        const operationMs = performance.now() - stepStartedAt;
        const parseStartedAt = performance.now();
        const response = JSON.parse(responseJson) as
          | { state: "complete"; result: NavDbOpenResult }
          | { state: "need_resources"; resources: CoreResourceRequest[] };
        const parseMs = performance.now() - parseStartedAt;
        debugLog("nav_kv.open.controller_step", {
          iteration,
          state: response.state,
          operation_ms: Math.round(operationMs),
          parse_ms: Math.round(parseMs),
          resource_count: response.state === "need_resources" ? response.resources.length : 0,
          resources: response.state === "need_resources" ? response.resources.map((resource) => resource.id) : undefined,
        });
        if (response.state === "complete") {
          finish = JSON.parse(await debugTiming("nav_kv.open.controller_finish", () =>
            wasm.nav_db_open_controller_finish(controllerHandle), {
              iteration,
            })) as NavDbOpenFinish;
          finished = true;
          break;
        }
        await debugTiming(
          "nav_kv.open.resource_batch",
          () => Promise.all(
            response.resources.map((resource) => fetchAndIngestResource(
              resource,
              (resourceId, bytes) => wasm.nav_db_open_controller_ingest_resource(controllerHandle, resourceId, bytes),
              "nav_kv.open_resource.fetch",
            )),
          ),
          {
            iteration,
            resource_count: response.resources.length,
            resources: response.resources.map((resource) => resource.id),
          },
        );
      }
    } catch (error) {
      if (isCoreSourceAssertion(error)) {
        throw error;
      }
      debugLog("nav_kv.root.missing", { reason: error instanceof Error ? error.message : String(error) });
      return null;
    } finally {
      if (!finished) {
        wasm.nav_db_open_controller_destroy(controllerHandle);
      }
    }
    if (!finish) {
      debugLog("nav_kv.root.missing", { reason: "nav-db open controller did not finish" });
      return null;
    }
    const selected = byFilename.get(finish.open_result.selected_filename);
    const rootUrl = selected?.root_source ? publicResourceUrl({
      id: `nav_db/artifact/${selected.filename}/root`,
      source: selected.root_source,
    }) : null;
    if (!rootUrl) {
      debugLog("nav_kv.root.missing", { selected_filename: finish.open_result.selected_filename });
      return null;
    }
    const store = new NavKvStore(wasm, finish.nav_kv_handle, rootUrl);
    await debugTiming("nav_kv.open.prefetch_root_pages", () => store.prefetchRootPages());
    return store;
  }

  private readonly navKvPackageRoot: string;

  destroy(): void {
    this.wasm.nav_kv_destroy(this.handle);
  }

  async runCoreOperation<T>(operation: unknown): Promise<T> {
    return this.runPagedOperation<T>(() => this.wasm.core_had_operation(this.handle, JSON.stringify(operation)));
  }

  async runCoreSessionOperation<T>(
    operation: (navKvHandle: number) => Promise<string> | string,
    ingestSessionResource?: (resourceId: string, resourceBytes: Uint8Array) => Promise<void> | void,
    onInvalidations?: UiInvalidationListener,
    reportResourceFailure?: ResourceFailureReporter,
    drainSessionEffects?: () => Promise<string> | string,
    operationLabel?: string,
  ): Promise<T> {
    const result = await this.runPagedOperation<T>(
      () => operation(this.handle),
      ingestSessionResource,
      onInvalidations,
      reportResourceFailure,
      operationLabel,
    );
    if (drainSessionEffects) {
      this.launchSessionEffectPump(
        drainSessionEffects,
        ingestSessionResource,
        onInvalidations,
        reportResourceFailure,
      );
    }
    return result;
  }

  attachToSession(sessionHandle: number): void {
    debugTiming("nav_kv.attach_to_session", () =>
      this.wasm.attach_nav_kv_store_to_session(this.handle, sessionHandle));
  }

  private async prefetchRootPages(): Promise<void> {
    const pagesJson = await debugTiming("nav_kv.prefetch_pages.list", () =>
      this.wasm.nav_kv_prefetch_pages(this.handle));
    const pages = JSON.parse(pagesJson) as number[];
    debugLog("nav_kv.prefetch_pages.list_result", {
      page_count: pages.length,
      pages,
      json_bytes: pagesJson.length,
    });
    if (pages.length === 0) {
      return;
    }
    await debugTiming("nav_kv.prefetch_pages", () => Promise.all(pages.map((page) => this.ensureNavKvPage(page))), {
      pages,
      page_count: pages.length,
    });
  }

  private async runPagedOperation<T>(
    operation: () => Promise<string> | string,
    ingestSessionResource?: (resourceId: string, resourceBytes: Uint8Array) => Promise<void> | void,
    onInvalidations?: UiInvalidationListener,
    reportResourceFailure?: ResourceFailureReporter,
    operationLabel?: string,
  ): Promise<T> {
    let iteration = 0;
    for (;;) {
      iteration += 1;
      const operationStartedAt = performance.now();
      const responseJson = await operation();
      const operationMs = performance.now() - operationStartedAt;
      const parseStartedAt = performance.now();
      const response = JSON.parse(responseJson) as
        | { state: "complete"; result: T; invalidations?: UiInvalidation[] }
        | { state: "need_resources"; resources: CoreResourceRequest[] };
      const parseMs = performance.now() - parseStartedAt;
      if (operationLabel) {
        debugLog(`${operationLabel}.core_had.step`, {
          iteration,
          state: response.state,
          operation_ms: Math.round(operationMs),
          parse_ms: Math.round(parseMs),
          json_bytes: responseJson.length,
          resource_count: response.state === "need_resources" ? response.resources.length : 0,
          resources: response.state === "need_resources"
            ? response.resources.map((resource) => resource.id)
            : undefined,
        });
      }
      if (response.state === "complete") {
        if (response.invalidations && response.invalidations.length > 0) {
          debugLog("core.ui.invalidations.source", {
            source: "paged_operation_complete",
            operation_label: operationLabel ?? null,
            invalidations: response.invalidations,
          });
          onInvalidations?.(response.invalidations);
        }
        return response.result;
      }
      await Promise.all(
        response.resources.map((resource) =>
          this.ensureResource(resource, ingestSessionResource, reportResourceFailure),
        ),
      );
    }
  }

  private async ensureResource(
    resource: CoreResourceRequest,
    ingestSessionResource?: (resourceId: string, resourceBytes: Uint8Array) => Promise<void> | void,
    reportResourceFailure?: ResourceFailureReporter,
  ): Promise<void> {
    if (resource.id.startsWith("nav_kv/page/")) {
      return this.ensureNavKvResource(resource);
    }
    if (!ingestSessionResource) {
      throw new Error(`core requested unsupported resource outside a session operation: ${resource.id}`);
    }
    await fetchAndIngestResource(resource, ingestSessionResource, "core.resource.fetch", reportResourceFailure);
  }

  private launchSessionEffectPump(
    drainSessionEffects: () => Promise<string> | string,
    ingestSessionResource?: (resourceId: string, resourceBytes: Uint8Array) => Promise<void> | void,
    onInvalidations?: UiInvalidationListener,
    reportResourceFailure?: ResourceFailureReporter,
  ): void {
    void this.pumpSessionEffects(
      drainSessionEffects,
      ingestSessionResource,
      onInvalidations,
      reportResourceFailure,
    ).catch((error: unknown) => {
      debugLog("core.session_effect.pump.error", {
        error: error instanceof Error ? error.message : String(error),
      });
    });
  }

  private async pumpSessionEffects(
    drainSessionEffects: () => Promise<string> | string,
    ingestSessionResource?: (resourceId: string, resourceBytes: Uint8Array) => Promise<void> | void,
    onInvalidations?: UiInvalidationListener,
    reportResourceFailure?: ResourceFailureReporter,
  ): Promise<void> {
    for (;;) {
      const effects = JSON.parse(await drainSessionEffects()) as CoreSessionResourceEffect[];
      if (effects.length === 0) {
        return;
      }
      const invalidations = new Set<UiInvalidation>();
      const settled = await Promise.allSettled(
        effects.map(async (effect) => {
          await this.ensureResource(effect.resource, ingestSessionResource, reportResourceFailure);
          for (const invalidation of effect.after_success_invalidations ?? []) {
            invalidations.add(invalidation);
          }
        }),
      );
      const failures = settled
        .map((result, index) => ({ result, effect: effects[index] }))
        .filter((entry): entry is { result: PromiseRejectedResult; effect: CoreSessionResourceEffect } =>
          entry.result.status === "rejected",
        );
      if (failures.length > 0) {
        debugLog("core.session_effect.resource_failures", {
          failures: failures.map(({ result, effect }) => ({
            resource: effect.resource.id,
            error: result.reason instanceof Error ? result.reason.message : String(result.reason),
          })),
        });
      }
      if (invalidations.size > 0) {
        debugLog("core.ui.invalidations.source", {
          source: "session_effect",
          invalidations: [...invalidations],
          resources: effects.map((effect) => effect.resource.id),
          effect_invalidations: effects.map((effect) => ({
            resource: effect.resource.id,
            invalidations: effect.after_success_invalidations ?? [],
          })),
        });
        onInvalidations?.([...invalidations]);
      }
    }
  }

  private ensureNavKvResource(resource: CoreResourceRequest): Promise<void> {
    const pageIndex = Number(resource.id.slice("nav_kv/page/".length));
    if (!Number.isInteger(pageIndex)) {
      throw new Error(`invalid nav kv page resource id: ${resource.id}`);
    }
    return this.ensureNavKvPage(pageIndex);
  }

  private ensureNavKvPage(pageIndex: number): Promise<void> {
    const priority = ++this.pageRequestSequence;
    this.pageRequestPriorities.set(pageIndex, priority);
    const pendingInsert = this.pendingPageInserts.get(pageIndex);
    if (pendingInsert) {
      pendingInsert.priority = priority;
    }
    const cached = this.inFlightPageFetches.get(pageIndex);
    if (cached) {
      return cached;
    }
    const resourceId = `nav_kv/page/${pageIndex.toString().padStart(4, "0")}`;
    const address = `${this.navKvPackageRoot}/page_${pageIndex.toString().padStart(4, "0")}`;
    const requestUrl = withNavKvCacheKey(address);
    const startedAt = performance.now();
    const fetched = debugTiming("nav_kv.page.fetch", () => fetch(requestUrl), {
      page: pageIndex,
    })
      .then(async (response) => {
        const headersAt = performance.now();
        if (!response.ok) {
          throw new Error(`failed to fetch nav_kv page ${pageIndex}: ${response.status}`);
        }
        const buffer = await debugTiming("nav_kv.page.array_buffer", () => response.arrayBuffer(), { page: pageIndex });
        const bytes = debugTiming("nav_kv.page.uint8_array", () => new Uint8Array(buffer), {
          page: pageIndex,
          byte_length: buffer.byteLength,
        });
        logNavKvPageFetchDetail(pageIndex, requestUrl, startedAt, headersAt, performance.now(), bytes.byteLength);
        await this.insertNavKvPage(resourceId, pageIndex, bytes, priority);
      });
    this.inFlightPageFetches.set(pageIndex, fetched);
    return fetched;
  }

  private insertNavKvPage(resourceId: string, pageIndex: number, bytes: Uint8Array, priority: number): Promise<void> {
    const updatedPriority = Math.max(priority, this.pageRequestPriorities.get(pageIndex) ?? priority);
    return new Promise<void>((resolve, reject) => {
      this.pendingPageInserts.set(pageIndex, {
        resourceId,
        pageIndex,
        bytes,
        priority: updatedPriority,
        resolve,
        reject,
      });
      this.pumpPageInsertQueue();
    });
  }

  private pumpPageInsertQueue(): void {
    if (this.pageInsertPumpActive) {
      return;
    }
    this.pageInsertPumpActive = true;
    void this.runPageInsertQueue();
  }

  private async runPageInsertQueue(): Promise<void> {
    try {
      for (;;) {
        const entry = this.takeHighestPriorityPageInsert();
        if (!entry) {
          return;
        }
        await yieldToWorkerEventLoop();
        try {
          await debugTiming("nav_kv.page.wasm_insert", () =>
            this.wasm.nav_kv_insert_resource(this.handle, entry.resourceId, entry.bytes), {
              page: entry.pageIndex,
              byte_length: entry.bytes.byteLength,
            });
          this.pageRequestPriorities.delete(entry.pageIndex);
          entry.resolve();
        } catch (error) {
          entry.reject(error);
        }
      }
    } finally {
      this.pageInsertPumpActive = false;
      if (this.pendingPageInserts.size > 0) {
        this.pumpPageInsertQueue();
      }
    }
  }

  private takeHighestPriorityPageInsert(): PendingNavKvPageInsert | null {
    let selected: PendingNavKvPageInsert | null = null;
    for (const entry of this.pendingPageInserts.values()) {
      if (!selected || entry.priority > selected.priority) {
        selected = entry;
      }
    }
    if (selected) {
      this.pendingPageInserts.delete(selected.pageIndex);
    }
    return selected;
  }

}

function yieldToWorkerEventLoop(): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, 0));
}

async function resolveSessionPublicationResult<T>(
  wasm: NavKvWasmModule,
  sessionHandle: number,
  operation: () => Promise<string> | string,
): Promise<T> {
  for (;;) {
    const response = JSON.parse(await operation()) as
      | { state: "complete"; result: T }
      | { state: "need_resources"; resources: CoreResourceRequest[] };
    if (response.state === "complete") {
      return response.result;
    }
    await Promise.all(response.resources.map((resource) =>
      fetchAndIngestResource(
        resource,
        (resourceId, bytes) => wasm.ingest_resource_in_session(sessionHandle, resourceId, bytes),
        "publication.resource.fetch",
      ),
    ));
  }
}

export async function resolveChartAssetUrl(
  sessionHandle: number,
  chartId: string,
  assetKind: "asset" | "thumbnail",
): Promise<string> {
  const wasm = await ensureWasmReady();
  const result = await resolveSessionPublicationResult<{ source: CoreResourceSource }>(
    wasm,
    sessionHandle,
    () => wasm.resolve_chart_asset_resource_in_session(sessionHandle, chartId, assetKind),
  );
  return publicResourceUrl({ id: `chart_asset/${assetKind}/${chartId}`, source: result.source });
}

type CoreResourceRequest = {
  id: string;
  source: CoreResourceSource;
  optional?: boolean;
};

type CoreSessionResourceEffect = {
  resource: CoreResourceRequest;
  after_success_invalidations?: UiInvalidation[];
};

type CoreResourceSource =
  | { kind: "public_url"; url: string }
  | { kind: "package_member"; package_id: string; filename: string; member_path: string }
  | { kind: "installed_artifact_member"; filename: string; member_path: string }
  | { kind: "nav_kv_member"; member_path: string }
  | { kind: "unavailable"; message: string };

type NavDbArtifactCandidate = {
  package_id: string;
  filename: string;
  warning_text?: string;
  root_source?: CoreResourceSource;
};

type NavDbArtifactOpenStatus = {
  package_id: string;
  filename: string;
  readable: boolean;
  message?: string;
};

type NavDbOpenResult = {
  selected_package_id: string;
  selected_filename: string;
  selected_contract_id?: string;
  selected_cycle?: string;
  selected_cycle_version?: string;
  selected_effective_date?: string;
  selected_expiration_date?: string;
  selected_warning_text?: string;
  statuses: NavDbArtifactOpenStatus[];
};

type NavDbOpenFinish = {
  nav_kv_handle: number;
  open_result: NavDbOpenResult;
};

async function fetchAndIngestResource(
  resource: CoreResourceRequest,
  ingestResource: (resourceId: string, resourceBytes: Uint8Array) => Promise<void> | void,
  timingLabel: string,
  reportResourceFailure?: ResourceFailureReporter,
): Promise<void> {
  const url = publicResourceUrl(resource);
  const response = await debugTiming(timingLabel, () => fetch(withNavKvCacheKey(url)), {
    id: resource.id,
    url,
  });
  if (!response.ok) {
    const message = `failed to fetch core resource ${resource.id} at ${url}: ${response.status}`;
    if (resource.optional) {
      await ingestResource(resource.id, new Uint8Array());
      return;
    }
    await reportResourceFailure?.(resource.id, message);
    throw new Error(message);
  }
  const buffer = await debugTiming(`${timingLabel}.array_buffer`, () => response.arrayBuffer(), {
    id: resource.id,
    url,
  });
  const bytes = debugTiming(`${timingLabel}.uint8_array`, () => new Uint8Array(buffer), {
    id: resource.id,
    byte_length: buffer.byteLength,
  });
  await debugTiming(`${timingLabel}.ingest`, () => ingestResource(resource.id, bytes), {
    id: resource.id,
    byte_length: bytes.byteLength,
    encoded_byte_length: bytes.byteLength,
  });
}

function publicResourceUrl(resource: CoreResourceRequest): string {
  const source = resource.source;
  if (!source) {
    throw new Error(`core resource ${resource.id} is missing typed source`);
  }
  if (source.kind === "public_url") {
    return source.url;
  }
  if (source.kind === "unavailable") {
    throw new Error(`core resource ${resource.id} is unavailable: ${source.message}`);
  }
  throw new Error(`web cannot fetch ${source.kind} resource ${resource.id}; expected public_url`);
}

function isCoreSourceAssertion(error: unknown): boolean {
  return error instanceof Error
    && (error.message.startsWith("web cannot fetch ")
      || error.message.includes(" is missing typed source"));
}

function withNavKvCacheKey(address: string): string {
  if (!address.includes("/nav_db_")) {
    return address;
  }
  const separator = address.includes("?") ? "&" : "?";
  return `${address}${separator}v=${encodeURIComponent(address)}`;
}

function logNavKvPageFetchDetail(
  pageIndex: number,
  requestUrl: string,
  startedAt: number,
  headersAt: number,
  completedAt: number,
  byteLength: number,
) {
  const timing = lastResourceTimingForUrl(requestUrl);
  const responseEnd = timing?.responseEnd;
  debugLog("nav_kv.page.fetch_detail", {
    page: pageIndex,
    byte_length: byteLength,
    fetch_header_ms: Math.round(headersAt - startedAt),
    array_buffer_done_ms: Math.round(completedAt - headersAt),
    total_ms: Math.round(completedAt - startedAt),
    resource_duration_ms: timing ? Math.round(timing.duration) : null,
    resource_response_start_ms: timing ? Math.round(timing.responseStart - timing.startTime) : null,
    resource_response_end_ms: timing ? Math.round(timing.responseEnd - timing.startTime) : null,
    response_end_to_array_done_ms: responseEnd !== undefined ? Math.round(completedAt - responseEnd) : null,
    transfer_size: timing?.transferSize ?? null,
    encoded_body_size: timing?.encodedBodySize ?? null,
    decoded_body_size: timing?.decodedBodySize ?? null,
  });
}

function lastResourceTimingForUrl(requestUrl: string): PerformanceResourceTiming | null {
  if (typeof performance === "undefined" || typeof window === "undefined") {
    return null;
  }
  const absoluteUrl = new URL(requestUrl, window.location.href).href;
  const entries = performance.getEntriesByName(absoluteUrl, "resource") as PerformanceResourceTiming[];
  return entries.at(-1) ?? null;
}

export async function getNavKvStore(sessionHandle?: number): Promise<NavKvStore | null> {
  if (!sharedNavKvStorePromise) {
    if (sessionHandle === undefined) {
      throw new Error("nav_kv root must be opened through a ui session publication resolver");
    }
    sharedNavKvStorePromise = NavKvStore.open(sessionHandle);
  }
  return sharedNavKvStorePromise;
}

export async function runCoreHadOperation<T>(operation: unknown): Promise<T> {
  const trace = coreHadOperationTrace(operation);
  return debugTiming("core.had_operation.total", async () => {
    const store = await debugTiming("core.had_operation.store", () => getNavKvStore(), trace);
    if (!store) {
      throw new Error("nav_kv root is unavailable");
    }
    return store.runCoreOperation<T>(operation);
  }, trace);
}

export async function runCoreHadSessionOperation<T>(
  sessionHandle: number,
  operation: (navKvHandle: number) => Promise<string> | string,
  ingestSessionResource?: (resourceId: string, resourceBytes: Uint8Array) => Promise<void> | void,
  onInvalidations?: UiInvalidationListener,
  reportResourceFailure?: ResourceFailureReporter,
  drainSessionEffects?: () => Promise<string> | string,
  operationLabel?: string,
): Promise<T> {
  const store = await getNavKvStore(sessionHandle);
  if (!store) {
    throw new Error("nav_kv root is unavailable");
  }
  return store.runCoreSessionOperation<T>(
    operation,
    ingestSessionResource,
    onInvalidations,
    reportResourceFailure,
    drainSessionEffects,
    operationLabel,
  );
}

export async function attachNavKvStoreToSession(sessionHandle: number): Promise<void> {
  const store = await getNavKvStore(sessionHandle);
  if (!store) {
    throw new Error("nav_kv root is unavailable");
  }
  store.attachToSession(sessionHandle);
}

function coreHadOperationTrace(operation: unknown): Record<string, unknown> {
  if (!operation || typeof operation !== "object" || !("kind" in operation)) {
    return { kind: "unknown" };
  }
  const record = operation as Record<string, unknown>;
  const trace: Record<string, unknown> = { kind: record.kind };
  for (const key of ["airport_id", "procedure_id", "procedure_kind", "airway_name", "prefix"]) {
    if (record[key] !== undefined) {
      trace[key] = record[key];
    }
  }
  return trace;
}
