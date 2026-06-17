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

type TerrainWorkerRenderBatchRequest = {
  kind: "render_batch";
  id: number;
  browserInstanceId: string;
  requests: TerrainRenderRequest[];
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
    }
  | {
      kind: "rendered_batch_tile";
      id: number;
      ok: true;
      generation: number;
      cacheKey: string;
      tileKey: string;
      rawBytes: Uint8Array;
    }
  | {
      kind: "rendered_batch";
      id: number;
      ok: true;
      tileCount: number;
    }
  | {
      kind: "rendered_batch";
      id: number;
      ok: false;
      error: string;
    };

type TerrainWorkerRequest = TerrainWorkerRenderRequest | TerrainWorkerRenderBatchRequest;

export class TerrainOverlayRenderer {
  private nextId = 1;
  private readonly worker = new Worker(new URL("./terrainOverlay.worker.ts", import.meta.url), {
    name: "aerobag-terrain-overlay",
    type: "module",
  });
  private readonly pending = new Map<number, {
    resolve: (result: unknown) => void;
    reject: (error: Error) => void;
    onTile?: (result: TerrainRenderResult) => void;
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
      this.pending.set(id, { resolve: (result) => resolve(result as TerrainRenderResult), reject });
      this.worker.postMessage(message);
    });
  }

  renderTiles(requests: TerrainRenderRequest[], onTile: (result: TerrainRenderResult) => void): Promise<void> {
    const id = this.nextId++;
    const message: TerrainWorkerRequest = {
      kind: "render_batch",
      id,
      browserInstanceId: getBrowserInstanceId(),
      requests,
    };
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve: () => resolve(), reject, onTile });
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
    if (message.ok && message.kind === "rendered") {
      this.pending.delete(message.id);
      pending.resolve({
        generation: message.generation,
        cacheKey: message.cacheKey,
        tileKey: message.tileKey,
        rawBytes: message.rawBytes,
      });
    } else if (message.ok && message.kind === "rendered_batch_tile") {
      pending.onTile?.({
        generation: message.generation,
        cacheKey: message.cacheKey,
        tileKey: message.tileKey,
        rawBytes: message.rawBytes,
      });
    } else if (message.ok && message.kind === "rendered_batch") {
      this.pending.delete(message.id);
      pending.resolve(undefined);
    } else {
      this.pending.delete(message.id);
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
