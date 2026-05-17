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

type NavKvWasmModule = PublicationResolverWasmModule & {
  attach_nav_kv_store_to_session(handle: number, sessionHandle: number): void;
  core_had_operation(handle: number, operationJson: string): string;
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
    if (!selected?.root_address) {
      debugLog("nav_kv.root.missing", { selected_filename: finish.open_result.selected_filename });
      return null;
    }
    const store = new NavKvStore(wasm, finish.nav_kv_handle, selected.root_address);
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
  ): Promise<T> {
    return this.runPagedOperation<T>(() => operation(this.handle), ingestSessionResource, onInvalidations);
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
      await Promise.all(response.resources.map((resource) => this.ensureResource(resource, ingestSessionResource)));
    }
  }

  private async ensureResource(
    resource: CoreResourceRequest,
    ingestSessionResource?: (resourceId: string, resourceBytes: Uint8Array) => Promise<void> | void,
  ): Promise<void> {
    if (resource.id.startsWith("nav_kv/page/")) {
      return this.ensureNavKvResource(resource);
    }
    if (!ingestSessionResource) {
      throw new Error(`core requested unsupported resource outside a session operation: ${resource.id}`);
    }
    await fetchAndIngestResource(resource, ingestSessionResource, "core.resource.fetch");
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
    const result = await this.resolveResult<{ address: string }>(operation);
    return result.address;
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
    const response = await debugTiming("publication.resource.fetch", () => fetch(resource.address, { cache: "no-cache" }), {
      id: resource.id,
      address: resource.address,
    });
    if (!response.ok) {
      throw new Error(`failed to fetch publication resource ${resource.id} at ${resource.address}: ${response.status}`);
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
  address: string;
  optional?: boolean;
};

type NavDbArtifactCandidate = {
  package_id: string;
  filename: string;
  root_address?: string;
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
): Promise<void> {
  const response = await debugTiming(timingLabel, () => fetch(withNavKvCacheKey(resource.address)), {
    id: resource.id,
    address: resource.address,
  });
  if (!response.ok) {
    if (resource.optional) {
      await ingestResource(resource.id, new Uint8Array());
      return;
    }
    throw new Error(`failed to fetch core resource ${resource.id} at ${resource.address}: ${response.status}`);
  }
  const bytes = new Uint8Array(await response.arrayBuffer());
  await ingestResource(resource.id, bytes);
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
): Promise<T> {
  const store = await getNavKvStore();
  if (!store) {
    throw new Error("nav_kv root is unavailable");
  }
  return store.runCoreSessionOperation<T>(operation, ingestSessionResource, onInvalidations);
}

export async function attachNavKvStoreToSession(sessionHandle: number): Promise<void> {
  const store = await getNavKvStore();
  if (!store) {
    throw new Error("nav_kv root is unavailable");
  }
  store.attachToSession(sessionHandle);
}
