import currentArtifactsJson from "@current-artifacts";
import { debugLog, debugTiming } from "./debugLog";

const currentArtifacts = currentArtifactsJson as {
  bundles?: Array<{ checksum_sha256?: string; filename?: string }>;
};
const latestBundle = currentArtifacts.bundles?.[currentArtifacts.bundles.length - 1];
const navKvCacheKey = encodeURIComponent(latestBundle?.checksum_sha256 ?? latestBundle?.filename ?? "unknown");

type NavKvWasmModule = {
  attach_nav_kv_store_to_session(handle: number, sessionHandle: number): void;
  core_had_operation(handle: number, operationJson: string): string;
  default?: (moduleOrPath?: string | URL | Request) => Promise<unknown>;
  nav_kv_destroy(handle: number): void;
  nav_kv_insert_page(handle: number, pageIndex: number, pageBytes: Uint8Array): void;
  nav_kv_open(rootBytes: Uint8Array): number;
};

let wasmReady: Promise<NavKvWasmModule> | null = null;
let sharedNavKvStorePromise: Promise<NavKvStore | null> | null = null;

async function ensureWasmReady(): Promise<NavKvWasmModule> {
  if (!wasmReady) {
    wasmReady = import("@generated/app_wasm.js").then(async (mod) => {
      const wasm = mod as NavKvWasmModule;
      await wasm.default?.();
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
  ) {}

  static async open(): Promise<NavKvStore | null> {
    const wasm = await debugTiming("nav_kv.wasm.init", () => ensureWasmReady());
    const rootResponse = await debugTiming("nav_kv.root.fetch", () => fetch(`/nav-kv/root?v=${navKvCacheKey}`), {
      cache_key: navKvCacheKey,
    });
    if (!rootResponse.ok) {
      debugLog("nav_kv.root.missing", { status: rootResponse.status });
      return null;
    }
    const rootBytes = new Uint8Array(await debugTiming("nav_kv.root.array_buffer", () => rootResponse.arrayBuffer()));
    const handle = debugTiming("nav_kv.root.parse_core", () => wasm.nav_kv_open(rootBytes), {
      root_bytes: rootBytes.byteLength,
    });
    return new NavKvStore(wasm, handle);
  }

  destroy(): void {
    this.wasm.nav_kv_destroy(this.handle);
  }

  async runCoreOperation<T>(operation: unknown): Promise<T> {
    return this.runPagedOperation<T>(() => this.wasm.core_had_operation(this.handle, JSON.stringify(operation)));
  }

  async runCoreSessionOperation<T>(operation: (navKvHandle: number) => Promise<string> | string): Promise<T> {
    return this.runPagedOperation<T>(() => operation(this.handle));
  }

  attachToSession(sessionHandle: number): void {
    this.wasm.attach_nav_kv_store_to_session(this.handle, sessionHandle);
  }

  private async runPagedOperation<T>(operation: () => Promise<string> | string): Promise<T> {
    for (;;) {
      const response = JSON.parse(await operation()) as
        | { state: "complete"; result: T }
        | { state: "need_pages"; pages: number[] };
      if (response.state === "complete") {
        return response.result;
      }
      await Promise.all(response.pages.map((pageIndex) => this.ensurePage(pageIndex)));
    }
  }

  private ensurePage(pageIndex: number): Promise<void> {
    const cached = this.inFlightPageFetches.get(pageIndex);
    if (cached) {
      return cached;
    }
    const fetched = debugTiming("nav_kv.page.fetch", () => fetch(`/nav-kv/values/${pageIndex.toString().padStart(4, "0")}?v=${navKvCacheKey}`), {
      page: pageIndex,
    }).then(async (response) => {
      if (!response.ok) {
        throw new Error(`failed to fetch nav_kv page ${pageIndex}: ${response.status}`);
      }
      const bytes = new Uint8Array(await debugTiming("nav_kv.page.array_buffer", () => response.arrayBuffer(), { page: pageIndex }));
      this.wasm.nav_kv_insert_page(this.handle, pageIndex, bytes);
    });
    this.inFlightPageFetches.set(pageIndex, fetched);
    return fetched;
  }
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

export async function runCoreHadSessionOperation<T>(operation: (navKvHandle: number) => Promise<string> | string): Promise<T> {
  const store = await getNavKvStore();
  if (!store) {
    throw new Error("nav_kv root is unavailable");
  }
  return store.runCoreSessionOperation<T>(operation);
}

export async function attachNavKvStoreToSession(sessionHandle: number): Promise<void> {
  const store = await getNavKvStore();
  if (!store) {
    throw new Error("nav_kv root is unavailable");
  }
  store.attachToSession(sessionHandle);
}
