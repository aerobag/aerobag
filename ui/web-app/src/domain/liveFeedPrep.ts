import { debugLog } from "./debugLog";

type LiveFeedPrepRequest =
  | {
      kind: "prepare";
      id: number;
      resourceId: string;
      resourceBytes: Uint8Array;
    }
  | {
      kind: "reset";
      id: number;
    };

type LiveFeedPrepResponse =
  | {
      kind: "prepared";
      id: number;
      ok: true;
      preparedBytes?: Uint8Array;
      elapsedMs?: number;
    }
  | {
      kind: "prepared";
      id: number;
      ok: false;
      error: string;
    };

type PendingRequest = {
  resolve: (bytes: Uint8Array | null) => void;
  reject: (error: Error) => void;
  resourceId: string | null;
  startedAt: number;
};

class LiveFeedPrepClient {
  private readonly worker = new Worker(new URL("./liveFeedPrep.worker.ts", import.meta.url), {
    type: "module",
  });
  private readonly pending = new Map<number, PendingRequest>();
  private nextId = 1;

  constructor() {
    this.worker.addEventListener("message", (event: MessageEvent<LiveFeedPrepResponse>) => {
      this.handleMessage(event.data);
    });
    this.worker.addEventListener("error", (event) => {
      this.rejectAll(new Error(`live-feed prep worker error: ${event.message}`));
    });
    this.worker.addEventListener("messageerror", () => {
      this.rejectAll(new Error("live-feed prep worker message clone failed"));
    });
  }

  prepare(resourceId: string, resourceBytes: Uint8Array): Promise<Uint8Array> {
    const id = this.nextId++;
    const request: LiveFeedPrepRequest = {
      kind: "prepare",
      id,
      resourceId,
      resourceBytes,
    };
    const result = new Promise<Uint8Array | null>((resolve, reject) => {
      this.pending.set(id, {
        resolve,
        reject,
        resourceId,
        startedAt: performance.now(),
      });
    });
    this.worker.postMessage(request, [resourceBytes.buffer]);
    return result.then((bytes) => {
      if (!bytes) {
        throw new Error(`live-feed prep ${id} returned no bytes`);
      }
      return bytes;
    });
  }

  reset(): Promise<void> {
    const id = this.nextId++;
    const request: LiveFeedPrepRequest = { kind: "reset", id };
    const result = new Promise<Uint8Array | null>((resolve, reject) => {
      this.pending.set(id, {
        resolve,
        reject,
        resourceId: null,
        startedAt: performance.now(),
      });
    });
    this.worker.postMessage(request);
    return result.then(() => undefined);
  }

  private handleMessage(message: LiveFeedPrepResponse): void {
    const pending = this.pending.get(message.id);
    if (!pending) {
      return;
    }
    this.pending.delete(message.id);
    if (!message.ok) {
      pending.reject(new Error(message.error));
      return;
    }
    if (pending.resourceId) {
      debugLog("live_feed.prep.received", {
        resource_id: pending.resourceId,
        prepared_bytes: message.preparedBytes?.byteLength ?? 0,
        worker_elapsed_ms: message.elapsedMs ?? null,
        round_trip_ms: Math.round(performance.now() - pending.startedAt),
      });
    }
    pending.resolve(message.preparedBytes ?? null);
  }

  private rejectAll(error: Error): void {
    for (const pending of this.pending.values()) {
      pending.reject(error);
    }
    this.pending.clear();
  }
}

let sharedPrepClient: LiveFeedPrepClient | null = null;

export async function prepareLiveFeedResource(
  resourceId: string,
  resourceBytes: Uint8Array,
): Promise<Uint8Array> {
  sharedPrepClient ??= new LiveFeedPrepClient();
  return sharedPrepClient.prepare(resourceId, resourceBytes);
}

export async function ingestPreparedLiveFeedResource(
  sessionHandle: number,
  resourceId: string,
  resourceBytes: Uint8Array,
  ingest: (sessionHandle: number, resourceId: string, preparedBytes: Uint8Array) => Promise<void> | void,
  shouldPrepare: (resourceId: string) => boolean,
  prepare: (resourceId: string, resourceBytes: Uint8Array) => Promise<Uint8Array> = prepareLiveFeedResource,
): Promise<boolean> {
  if (!shouldPrepare(resourceId)) {
    return false;
  }
  const preparedBytes = await prepare(resourceId, resourceBytes);
  await ingest(sessionHandle, resourceId, preparedBytes);
  return true;
}

export async function resetLiveFeedPrep(): Promise<void> {
  if (sharedPrepClient) {
    await sharedPrepClient.reset();
  }
}
