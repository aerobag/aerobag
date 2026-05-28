import type { TerrainOverlaySourceTile } from "./appCoreAdapter";
import { getBrowserInstanceId } from "./debugLog";

type TerrainRenderRequest = {
  generation: number;
  cacheKey: string;
  tileKey: string;
  altitudeBucketFt: number;
  sourceTiles: TerrainOverlaySourceTile[];
};

export type TerrainRenderResult = {
  generation: number;
  cacheKey: string;
  tileKey: string;
  rawBytes: Uint8Array;
};

type TerrainWorkerRenderRequest = TerrainRenderRequest & {
  kind: "render";
  id: number;
  browserInstanceId: string;
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

export class TerrainOverlayRenderer {
  private nextId = 1;
  private readonly worker = new Worker(new URL("./terrainOverlay.worker.ts", import.meta.url), {
    name: "aerobag-terrain-overlay",
    type: "module",
  });
  private readonly pending = new Map<number, {
    resolve: (result: TerrainRenderResult) => void;
    reject: (error: Error) => void;
  }>();

  constructor() {
    this.worker.addEventListener("message", (event: MessageEvent<TerrainWorkerRenderResponse>) => {
      this.handleMessage(event.data);
    });
    this.worker.addEventListener("error", (event) => {
      this.rejectAll(new Error(`terrain worker error: ${event.message}`));
    });
    this.worker.addEventListener("messageerror", () => {
      this.rejectAll(new Error("terrain worker message clone failed"));
    });
  }

  renderTile(request: TerrainRenderRequest): Promise<TerrainRenderResult> {
    const id = this.nextId++;
    const message: TerrainWorkerRenderRequest = {
      kind: "render",
      id,
      browserInstanceId: getBrowserInstanceId(),
      ...request,
    };
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      this.worker.postMessage(message);
    });
  }

  destroy(): void {
    this.rejectAll(new Error("terrain worker destroyed"));
    this.worker.terminate();
  }

  private handleMessage(message: TerrainWorkerRenderResponse): void {
    const pending = this.pending.get(message.id);
    if (!pending) {
      return;
    }
    this.pending.delete(message.id);
    if (message.ok) {
      pending.resolve({
        generation: message.generation,
        cacheKey: message.cacheKey,
        tileKey: message.tileKey,
        rawBytes: message.rawBytes,
      });
    } else {
      pending.reject(new Error(message.error));
    }
  }

  private rejectAll(error: Error): void {
    for (const pending of this.pending.values()) {
      pending.reject(error);
    }
    this.pending.clear();
  }
}
