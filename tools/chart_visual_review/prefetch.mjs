// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

export function sheetsAhead(rows, selected, choice) {
  const index = Math.max(0, rows.findIndex(s => s.id === selected));
  if (choice === "all") return [...rows.slice(index), ...rows.slice(0,index)];
  return choice === "10" && selected ? rows.slice(index,index+11) : [];
}

// Requests are keyed by immutable sheet revision. Background work never fills
// both slots: jumping to a different sheet must not wait behind a long queue.
export class SheetPrefetchQueue {
  constructor({load, changed = () => {}, concurrency = 2}) {
    this.load = load;
    this.changed = changed;
    this.concurrency = concurrency;
    this.entries = new Map();
    this.requests = new Map();
  }

  setRequests(requests) {
    this.requests = new Map();
    for (const request of requests) {
      const previous = this.requests.get(request.key);
      if (!previous || previous.priority > request.priority) this.requests.set(request.key, request);
    }
    this.pump();
    this.changed();
  }

  get(key) { return this.entries.get(key); }

  retryFailed() {
    for (const [key, entry] of this.entries) {
      if (entry.status === "error" && this.requests.has(key)) this.entries.delete(key);
    }
    this.pump();
    this.changed();
  }

  pump() {
    const running = [...this.entries.values()].filter(e => e.status === "loading");
    let count = running.length;
    let background = running.filter(e => (this.requests.get(e.key)?.priority ?? e.priority) >= 2).length;
    for (const request of [...this.requests.values()].sort((a,b) => a.priority-b.priority)) {
      if (count >= this.concurrency) break;
      if (this.entries.has(request.key) || (request.priority >= 2 && background >= 1)) continue;
      const entry = {key:request.key, priority:request.priority, status:"loading"};
      this.entries.set(request.key, entry);
      count++;
      if (request.priority >= 2) background++;
      Promise.resolve().then(() => this.load(request.value)).then(value => {
        Object.assign(entry, {status:"ready", value});
      }, error => {
        Object.assign(entry, {status:"error", error});
      }).finally(() => { this.pump(); this.changed(); });
    }
  }
}

// Warm the ordinary HTTP byte cache, not hundreds of decoded Image objects or
// retained blobs. Later SVG/IMG requests reuse these revision-addressed URLs.
export async function cacheImageBytes(url, fetcher = fetch) {
  const response = await fetcher(url);
  if (!response.ok) throw new Error(`Image fetch failed (${response.status})`);
  const reader = response.body.getReader();
  let bytes = 0;
  try {
    for (;;) {
      const {done, value} = await reader.read();
      if (done) return bytes;
      bytes += value.byteLength;
    }
  } finally { reader.releaseLock(); }
}
