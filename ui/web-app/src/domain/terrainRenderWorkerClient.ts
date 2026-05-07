type TerrainRenderRequest = {
  id: number;
  altitudeFt: number;
  packedTileBytes: Uint8Array;
};

type TerrainRenderResponse =
  | { id: number; ok: true; rawBytes: Uint8Array }
  | { id: number; ok: false; error: string };

type PendingTerrainRender = {
  resolve: (bytes: Uint8Array) => void;
  reject: (error: Error) => void;
};

export class TerrainRenderWorkerClient {
  private readonly worker: Worker;
  private readonly pending = new Map<number, PendingTerrainRender>();
  private nextRequestId = 1;

  constructor() {
    this.worker = new Worker(new URL("./terrainRenderWorker.ts", import.meta.url), {
      type: "module",
    });
    this.worker.onmessage = (event: MessageEvent<TerrainRenderResponse>) => {
      const response = event.data;
      const pending = this.pending.get(response.id);
      if (!pending) {
        return;
      }
      this.pending.delete(response.id);
      if (response.ok) {
        pending.resolve(response.rawBytes);
      } else {
        pending.reject(new Error(response.error));
      }
    };
    this.worker.onerror = (event) => {
      const error = new Error(event.message || "terrain render worker failed");
      for (const pending of this.pending.values()) {
        pending.reject(error);
      }
      this.pending.clear();
    };
  }

  renderPackedTiles(packedTileBytes: Uint8Array, altitudeFt: number): Promise<Uint8Array> {
    return this.post({ altitudeFt, packedTileBytes }, [packedTileBytes.buffer]);
  }

  destroy() {
    this.worker.terminate();
    for (const pending of this.pending.values()) {
      pending.reject(new Error("terrain render worker destroyed"));
    }
    this.pending.clear();
  }

  private post(
    request: Omit<TerrainRenderRequest, "id">,
    transfer: Transferable[],
  ): Promise<Uint8Array> {
    const id = this.nextRequestId++;
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      this.worker.postMessage({ ...request, id }, transfer);
    });
  }
}
