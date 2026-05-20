import { debugLog, debugTiming, installRustDebugLogBridge } from "./debugLog";

export type PublicationResolverWasmModule = {
  default?: (moduleOrPath?: string | URL | Request) => Promise<unknown>;
  publication_resolver_destroy(handle: number): void;
  publication_resolver_ingest_resource(handle: number, resourceId: string, resourceBytes: Uint8Array): Promise<void> | void;
  publication_resolver_open(publicBaseUrl: string): number;
  publication_resolver_resolve_metar_manifest(handle: number): Promise<string> | string;
  publication_resolver_resolve_nav_db_artifact_candidates(handle: number): Promise<string> | string;
  publication_resolver_resolve_obstacle_manifest(handle: number): Promise<string> | string;
  publication_resolver_resolve_package_member(handle: number, packageId: string, memberPath: string): Promise<string> | string;
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

type NavKvWasmModule = PublicationResolverWasmModule & {
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

  private constructor(
    private readonly wasm: NavKvWasmModule,
    private readonly handle: number,
    navKvRootUrl: string,
  ) {
    this.navKvPackageRoot = navKvRootUrl.replace(/\/root(?:[?#].*)?$/, "");
  }

  static async open(): Promise<NavKvStore | null> {
    const wasm = await debugTiming("nav_kv.wasm.init", () => ensureWasmReady());
    const publicationResolver = new PublicationResolver(wasm, wasm.publication_resolver_open("/packages"));
    let candidates: NavDbArtifactCandidate[];
    try {
      candidates = await publicationResolver.resolveNavDbArtifactCandidates();
    } finally {
      publicationResolver.destroy();
    }
    if (candidates.length === 0) {
      debugLog("nav_kv.root.missing", { reason: "publication has no nav-db candidates" });
      return null;
    }
    const byFilename = new Map(candidates.map((candidate) => [candidate.filename, candidate]));
    const controllerHandle = wasm.nav_db_open_controller_create(JSON.stringify(candidates));
    let finished = false;
    let finish: NavDbOpenFinish | null = null;
    try {
      for (;;) {
        const response = JSON.parse(await wasm.nav_db_open_controller_step(controllerHandle)) as
          | { state: "complete"; result: NavDbOpenResult }
          | { state: "need_resources"; resources: CoreResourceRequest[] };
        if (response.state === "complete") {
          finish = JSON.parse(await wasm.nav_db_open_controller_finish(controllerHandle)) as NavDbOpenFinish;
          finished = true;
          break;
        }
        await Promise.all(
          response.resources.map((resource) => fetchAndIngestResource(
            resource,
            (resourceId, bytes) => wasm.nav_db_open_controller_ingest_resource(controllerHandle, resourceId, bytes),
            "nav_kv.open_resource.fetch",
          )),
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
    await store.prefetchRootPages();
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
  ): Promise<T> {
    const result = await this.runPagedOperation<T>(
      () => operation(this.handle),
      ingestSessionResource,
      onInvalidations,
      reportResourceFailure,
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
    this.wasm.attach_nav_kv_store_to_session(this.handle, sessionHandle);
  }

  private async prefetchRootPages(): Promise<void> {
    const pages = JSON.parse(await this.wasm.nav_kv_prefetch_pages(this.handle)) as number[];
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
  ): Promise<T> {
    for (;;) {
      const response = JSON.parse(await operation()) as
        | { state: "complete"; result: T; invalidations?: UiInvalidation[] }
        | { state: "need_resources"; resources: CoreResourceRequest[] };
      if (response.state === "complete") {
        if (response.invalidations && response.invalidations.length > 0) {
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
    const cached = this.inFlightPageFetches.get(pageIndex);
    if (cached) {
      return cached;
    }
    const resourceId = `nav_kv/page/${pageIndex.toString().padStart(4, "0")}`;
    const address = `${this.navKvPackageRoot}/page_${pageIndex.toString().padStart(4, "0")}`;
    const fetched = debugTiming("nav_kv.page.fetch", () => fetch(withNavKvCacheKey(address)), {
      page: pageIndex,
    })
      .then(async (response) => {
        if (!response.ok) {
          throw new Error(`failed to fetch nav_kv page ${pageIndex}: ${response.status}`);
        }
        const bytes = new Uint8Array(
          await debugTiming("nav_kv.page.array_buffer", () => response.arrayBuffer(), { page: pageIndex }),
        );
        await this.wasm.nav_kv_insert_resource(this.handle, resourceId, bytes);
      });
    this.inFlightPageFetches.set(pageIndex, fetched);
    return fetched;
  }

}

export class PublicationResolver {
  constructor(
    private readonly wasm: PublicationResolverWasmModule,
    private readonly handle: number,
  ) {}

  async resolveNavDbArtifactCandidates(): Promise<NavDbArtifactCandidate[]> {
    return this.resolveResult(() => this.wasm.publication_resolver_resolve_nav_db_artifact_candidates(this.handle));
  }

  async resolveObstacleManifest(): Promise<string> {
    return this.resolve(() => this.wasm.publication_resolver_resolve_obstacle_manifest(this.handle));
  }

  async resolveMetarManifest(): Promise<string> {
    return this.resolve(() => this.wasm.publication_resolver_resolve_metar_manifest(this.handle));
  }

  async resolvePackageMember(packageId: string, memberPath: string): Promise<string> {
    return this.resolve(() => this.wasm.publication_resolver_resolve_package_member(this.handle, packageId, memberPath));
  }

  private async resolve(operation: () => Promise<string> | string): Promise<string> {
    const result = await this.resolveResult<{ source: CoreResourceSource }>(operation);
    return publicResourceUrl({ id: "publication/resolved_package_member", source: result.source });
  }

  private async resolveResult<T>(operation: () => Promise<string> | string): Promise<T> {
    for (;;) {
      const response = JSON.parse(await operation()) as
        | { state: "complete"; result: T }
        | { state: "need_resources"; resources: CoreResourceRequest[] };
      if (response.state === "complete") {
        return response.result;
      }
      await Promise.all(response.resources.map((resource) => this.fetchAndIngest(resource)));
    }
  }

  destroy(): void {
    this.wasm.publication_resolver_destroy(this.handle);
  }

  private async fetchAndIngest(resource: CoreResourceRequest): Promise<void> {
    const url = publicResourceUrl(resource);
    const response = await debugTiming("publication.resource.fetch", () => fetch(url, { cache: "no-cache" }), {
      id: resource.id,
      url,
    });
    if (!response.ok) {
      throw new Error(`failed to fetch publication resource ${resource.id} at ${url}: ${response.status}`);
    }
    const bytes = new Uint8Array(await response.arrayBuffer());
    await this.wasm.publication_resolver_ingest_resource(this.handle, resource.id, bytes);
  }
}

let sharedPublicationResolverPromise: Promise<PublicationResolver> | null = null;

export async function getPublicationResolver(): Promise<PublicationResolver> {
  if (!sharedPublicationResolverPromise) {
    sharedPublicationResolverPromise = ensureWasmReady().then((wasm) =>
      new PublicationResolver(wasm, wasm.publication_resolver_open("/packages")),
    );
  }
  return sharedPublicationResolverPromise;
}

export async function resolvePackageMemberUrl(packageId: string, memberPath: string): Promise<string> {
  return (await getPublicationResolver()).resolvePackageMember(packageId, memberPath);
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
  const bytes = new Uint8Array(await response.arrayBuffer());
  await ingestResource(resource.id, bytes);
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

export async function getNavKvStore(): Promise<NavKvStore | null> {
  if (!sharedNavKvStorePromise) {
    sharedNavKvStorePromise = NavKvStore.open();
  }
  return sharedNavKvStorePromise;
}

export async function runCoreHadOperation<T>(operation: unknown): Promise<T> {
  const store = await getNavKvStore();
  if (!store) {
    throw new Error("nav_kv root is unavailable");
  }
  return store.runCoreOperation<T>(operation);
}

export async function runCoreHadSessionOperation<T>(
  operation: (navKvHandle: number) => Promise<string> | string,
  ingestSessionResource?: (resourceId: string, resourceBytes: Uint8Array) => Promise<void> | void,
  onInvalidations?: UiInvalidationListener,
  reportResourceFailure?: ResourceFailureReporter,
  drainSessionEffects?: () => Promise<string> | string,
): Promise<T> {
  const store = await getNavKvStore();
  if (!store) {
    throw new Error("nav_kv root is unavailable");
  }
  return store.runCoreSessionOperation<T>(
    operation,
    ingestSessionResource,
    onInvalidations,
    reportResourceFailure,
    drainSessionEffects,
  );
}

export async function attachNavKvStoreToSession(sessionHandle: number): Promise<void> {
  const store = await getNavKvStore();
  if (!store) {
    throw new Error("nav_kv root is unavailable");
  }
  store.attachToSession(sessionHandle);
}
