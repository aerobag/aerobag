import init, {
  install_rust_debug_logger,
  render_terrain_warning_raw_rgba,
  render_terrain_warning_raw_rgba_from_packed_tiles,
} from "@generated/app_wasm.js";
import type { TerrainOverlaySourceTile } from "./appCoreAdapter";
import { debugLog, installRustDebugLogBridge } from "./debugLog";

type TerrainWorkerRenderRequest = {
  kind: "render";
  id: number;
  generation: number;
  cacheKey: string;
  tileKey: string;
  altitudeBucketFt: number;
  sourceTiles: TerrainOverlaySourceTile[];
};

type TerrainWorkerRenderResponse =
  | {
      kind: "rendered";
      id: number;
      ok: true;
      generation: number;
      cacheKey: string;
      tileKey: string;
      rawBytes: Uint8Array;
    }
  | {
      kind: "rendered";
      id: number;
      ok: false;
      generation: number;
      cacheKey: string;
      tileKey: string;
      error: string;
    };

type TerrainWorkerRuntime = {
  addEventListener(type: "message", listener: (event: MessageEvent<TerrainWorkerRenderRequest>) => void): void;
  postMessage(message: TerrainWorkerRenderResponse, transfer?: Transferable[]): void;
};

const ctx = self as unknown as TerrainWorkerRuntime;
const sourceTileCache = new Map<string, Promise<Uint8Array>>();
let wasmReady: Promise<void> | null = null;

ctx.addEventListener("message", (event) => {
  const message = event.data;
  if (message.kind !== "render") {
    return;
  }
  void renderTerrainTile(message);
});

async function renderTerrainTile(message: TerrainWorkerRenderRequest): Promise<void> {
  const startedAt = performance.now();
  try {
    await ensureWasmReady();
    if (message.sourceTiles.length === 0) {
      throw new Error(`terrain tile ${message.tileKey} has no source tiles`);
    }
    const fetchStartedAt = performance.now();
    const sourceBytes = await Promise.all(message.sourceTiles.map(fetchSourceTile));
    const fetchMs = performance.now() - fetchStartedAt;
    const renderStartedAt = performance.now();
    const rawBytes = sourceBytes.length === 1
      ? render_terrain_warning_raw_rgba(sourceBytes[0], message.altitudeBucketFt)
      : render_terrain_warning_raw_rgba_from_packed_tiles(packTerrainTileBytes(sourceBytes), message.altitudeBucketFt);
    const renderMs = performance.now() - renderStartedAt;
    const transferableBytes = new Uint8Array(rawBytes);
    debugLog("terrain.worker.tile.done", {
      tile_key: message.tileKey,
      source_count: sourceBytes.length,
      source_bytes: sourceBytes.reduce((sum, bytes) => sum + bytes.byteLength, 0),
      raw_bytes: transferableBytes.byteLength,
      fetch_ms: Math.round(fetchMs),
      render_ms: Math.round(renderMs),
      elapsed_ms: Math.round(performance.now() - startedAt),
    });
    ctx.postMessage({
      kind: "rendered",
      id: message.id,
      ok: true,
      generation: message.generation,
      cacheKey: message.cacheKey,
      tileKey: message.tileKey,
      rawBytes: transferableBytes,
    }, [transferableBytes.buffer]);
  } catch (error) {
    const messageText = error instanceof Error ? error.message : String(error);
    debugLog("terrain.worker.tile.error", {
      tile_key: message.tileKey,
      elapsed_ms: Math.round(performance.now() - startedAt),
      error: messageText,
    });
    ctx.postMessage({
      kind: "rendered",
      id: message.id,
      ok: false,
      generation: message.generation,
      cacheKey: message.cacheKey,
      tileKey: message.tileKey,
      error: messageText,
    });
  }
}

async function ensureWasmReady(): Promise<void> {
  if (!wasmReady) {
    installRustDebugLogBridge();
    wasmReady = init().then(() => {
      install_rust_debug_logger();
    });
  }
  return wasmReady;
}

function fetchSourceTile(sourceTile: TerrainOverlaySourceTile): Promise<Uint8Array> {
  const key = `${sourceTile.product_id}/${sourceTile.path}`;
  let cached = sourceTileCache.get(key);
  if (!cached) {
    cached = fetchSourceTileBytes(sourceTile).catch((error) => {
      if (sourceTileCache.get(key) === cached) {
        sourceTileCache.delete(key);
      }
      throw error;
    });
    sourceTileCache.set(key, cached);
  }
  return cached;
}

async function fetchSourceTileBytes(sourceTile: TerrainOverlaySourceTile): Promise<Uint8Array> {
  const resource = sourceTile.resource;
  if (!resource) {
    throw new Error(`terrain source ${sourceTile.product_id}/${sourceTile.path} has no resolved resource`);
  }
  if (resource.source.kind === "unavailable") {
    throw new Error(`terrain source ${resource.id} unavailable: ${resource.source.message}`);
  }
  if (resource.source.kind !== "public_url") {
    throw new Error(`terrain worker cannot fetch ${resource.source.kind} resource ${resource.id}`);
  }
  const response = await fetch(resource.source.url, { cache: "force-cache" });
  if (!response.ok) {
    throw new Error(`failed to fetch terrain source ${resource.id} at ${resource.source.url}: ${response.status}`);
  }
  return new Uint8Array(await response.arrayBuffer());
}

function packTerrainTileBytes(tiles: Uint8Array[]): Uint8Array {
  const byteLength = 4 + tiles.reduce((sum, tile) => sum + 4 + tile.byteLength, 0);
  const packed = new Uint8Array(byteLength);
  const view = new DataView(packed.buffer);
  let cursor = 0;
  view.setUint32(cursor, tiles.length, true);
  cursor += 4;
  for (const tile of tiles) {
    view.setUint32(cursor, tile.byteLength, true);
    cursor += 4;
    packed.set(tile, cursor);
    cursor += tile.byteLength;
  }
  return packed;
}
