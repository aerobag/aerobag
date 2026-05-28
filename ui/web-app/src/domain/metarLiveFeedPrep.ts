type MetarLiveFeedPrepRequest =
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

type MetarLiveFeedPrepResponse =
  | {
      kind: "prepared";
      id: number;
      ok: true;
      resourceId?: string;
      preparedBytes?: Uint8Array;
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
};

class MetarLiveFeedPrepClient {
  private readonly worker = new Worker(new URL("./metarLiveFeedPrep.worker.ts", import.meta.url), {
    type: "module",
  });
  private readonly pending = new Map<number, PendingRequest>();
  private nextId = 1;

  constructor() {
    this.worker.addEventListener("message", (event: MessageEvent<MetarLiveFeedPrepResponse>) => {
      this.handleMessage(event.data);
    });
    this.worker.addEventListener("error", (event) => {
      this.rejectAll(new Error(`METAR live-feed prep worker error: ${event.message}`));
    });
    this.worker.addEventListener("messageerror", () => {
      this.rejectAll(new Error("METAR live-feed prep worker message clone failed"));
    });
  }

  prepare(resourceId: string, resourceBytes: Uint8Array): Promise<Uint8Array> {
    const id = this.nextId++;
    const request: MetarLiveFeedPrepRequest = {
      kind: "prepare",
      id,
      resourceId,
      resourceBytes,
    };
    const result = new Promise<Uint8Array | null>((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
    });
    this.worker.postMessage(request, [resourceBytes.buffer]);
    return result.then((bytes) => {
      if (!bytes) {
        throw new Error(`METAR live-feed prep ${id} returned no bytes`);
      }
      return bytes;
    });
  }

  reset(): Promise<void> {
    const id = this.nextId++;
    const request: MetarLiveFeedPrepRequest = { kind: "reset", id };
    const result = new Promise<Uint8Array | null>((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
    });
    this.worker.postMessage(request);
    return result.then(() => undefined);
  }

  private handleMessage(message: MetarLiveFeedPrepResponse): void {
    const pending = this.pending.get(message.id);
    if (!pending) {
      return;
    }
    this.pending.delete(message.id);
    if (!message.ok) {
      pending.reject(new Error(message.error));
      return;
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

let sharedPrepClient: MetarLiveFeedPrepClient | null = null;

export function isMetarLiveFeedPayloadResource(resourceId: string): boolean {
  return resourceId.startsWith("live_feeds/state/metars/")
    || resourceId.startsWith("live_feeds/delta/metars/");
}

export async function prepareMetarLiveFeedResource(
  resourceId: string,
  resourceBytes: Uint8Array,
): Promise<Uint8Array> {
  sharedPrepClient ??= new MetarLiveFeedPrepClient();
  return sharedPrepClient.prepare(resourceId, resourceBytes);
}

export async function resetMetarLiveFeedPrep(): Promise<void> {
  if (!sharedPrepClient) {
    return;
  }
  await sharedPrepClient.reset();
}
