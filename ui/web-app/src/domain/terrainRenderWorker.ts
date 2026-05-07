import initWasm, {
  render_terrain_warning_raw_rgba_from_packed_tiles,
} from "@generated/app_wasm.js";

type TerrainRenderRequest = {
  id: number;
  altitudeFt: number;
  packedTileBytes: Uint8Array;
};

type TerrainRenderResponse =
  | { id: number; ok: true; rawBytes: Uint8Array }
  | { id: number; ok: false; error: string };

type TerrainWorkerScope = {
  onmessage: ((event: MessageEvent<TerrainRenderRequest>) => void) | null;
  postMessage(message: TerrainRenderResponse, transfer?: Transferable[]): void;
};

let wasmReady: Promise<unknown> | null = null;
const workerSelf = self as unknown as TerrainWorkerScope;

function ensureWasmReady() {
  if (!wasmReady) {
    wasmReady = initWasm();
  }
  return wasmReady;
}

workerSelf.onmessage = (event: MessageEvent<TerrainRenderRequest>) => {
  const request = event.data;
  void (async () => {
    try {
      await ensureWasmReady();
      const rawBytes = render_terrain_warning_raw_rgba_from_packed_tiles(request.packedTileBytes, request.altitudeFt);
      const response: TerrainRenderResponse = {
        id: request.id,
        ok: true,
        rawBytes,
      };
      workerSelf.postMessage(response, [rawBytes.buffer as ArrayBuffer]);
    } catch (error: unknown) {
      const response: TerrainRenderResponse = {
        id: request.id,
        ok: false,
        error: error instanceof Error ? error.message : String(error),
      };
      workerSelf.postMessage(response);
    }
  })();
};
