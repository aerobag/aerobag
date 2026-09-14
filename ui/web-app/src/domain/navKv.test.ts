// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { describe, expect, expectTypeOf, it, vi } from "vitest";
import { observeDebugLog, type DebugLogRecord } from "./debugLog";
import {
  completeResourceFreeSessionMutation,
  NavKvStore,
  ResourceIngestCoordinator,
  resolvePublicResourceUrl,
  type SessionMutationOperationJson,
  type SessionResultOperationJson,
  type SessionSnapshotOperationJson,
} from "./navKv";

type TestableNavKvStore = {
  ensureNavKvPage(pageIndex: number): Promise<void>;
};

describe("session operation wire types", () => {
  it("keeps results, mutations, and snapshots nominally distinct", () => {
    expectTypeOf<SessionResultOperationJson>().not.toMatchTypeOf<SessionMutationOperationJson>();
    expectTypeOf<SessionResultOperationJson>().not.toMatchTypeOf<SessionSnapshotOperationJson>();
    expectTypeOf<SessionMutationOperationJson>().not.toMatchTypeOf<SessionSnapshotOperationJson>();
  });

  it("completes pre-NAVKV mutations without opening the resource pager", async () => {
    const completion = JSON.stringify({
      state: "complete",
      result: { ui_contract_version: 2, session_revision: 1 },
    }) as SessionMutationOperationJson;

    await expect(completeResourceFreeSessionMutation(completion, "test.bootstrap"))
      .resolves.toEqual({ ui_contract_version: 2, session_revision: 1 });
  });

  it("rejects a resource request from a pre-NAVKV mutation", async () => {
    const needsResources = JSON.stringify({
      state: "need_resources",
      resources: [{ id: "nav_db/artifact/0/root" }],
    }) as SessionMutationOperationJson;

    await expect(completeResourceFreeSessionMutation(needsResources, "test.bootstrap"))
      .rejects.toThrow("test.bootstrap must complete before NAVKV resource paging");
  });
});

describe("ResourceIngestCoordinator", () => {
  it("shares one in-flight ingestion between concurrent requesters", async () => {
    const coordinator = new ResourceIngestCoordinator();
    let finish!: () => void;
    const load = new Promise<void>((resolve) => {
      finish = resolve;
    });
    let loadCount = 0;

    const first = coordinator.run("live_feeds/state/metars/v1", () => {
      loadCount += 1;
      return load;
    });
    const second = coordinator.run("live_feeds/state/metars/v1", () => {
      loadCount += 1;
      return load;
    });

    expect(first).toBe(second);
    expect(loadCount).toBe(1);
    finish();
    await Promise.all([first, second]);
  });

  it("allows a failed resource to be retried", async () => {
    const coordinator = new ResourceIngestCoordinator();
    let loadCount = 0;

    await expect(coordinator.run("live_feeds/state/tafs/v1", async () => {
      loadCount += 1;
      throw new Error("temporary failure");
    })).rejects.toThrow("temporary failure");
    await coordinator.run("live_feeds/state/tafs/v1", async () => {
      loadCount += 1;
    });

    expect(loadCount).toBe(2);
  });
});

describe("NavKvStore page fetching", () => {
  it("does not inspect resource timings with production diagnostics disabled", async () => {
    vi.stubGlobal("window", undefined);
    vi.stubGlobal("location", { protocol: "http:", href: "http://fixture.test/worker.js" });
    vi.stubGlobal("__aerobagDebugLogEnabled", false);
    vi.stubGlobal("__aerobagPerfRunId", undefined);
    vi.stubGlobal("fetch", vi.fn<typeof fetch>(async () => new Response(new Uint8Array([1, 2, 3]))));
    const getEntries = vi.spyOn(performance, "getEntriesByName");
    const records: DebugLogRecord[] = [];
    const unobserve = observeDebugLog((record) => records.push(record));
    const insert = vi.fn();
    const store = Reflect.construct(NavKvStore, [
      { nav_kv_insert_resource: insert }, 17, "http://fixture.test/nav_db/root",
    ]) as TestableNavKvStore;
    try {
      await store.ensureNavKvPage(1);
      expect(insert).toHaveBeenCalledOnce();
      expect(getEntries).not.toHaveBeenCalled();
      expect(records).toHaveLength(0);
    } finally {
      unobserve();
      getEntries.mockRestore();
      vi.unstubAllGlobals();
    }
  });

  it.each([0, 423])("records worker resource timing with transfer size %i", async (transferSize) => {
    vi.stubGlobal("window", undefined);
    vi.stubGlobal("location", { protocol: "http:", href: "http://fixture.test/worker.js" });
    vi.stubGlobal("__aerobagDebugLogEnabled", true);
    const fetchPage = vi.fn<typeof fetch>(async () => new Response(new Uint8Array([1, 2, 3])));
    vi.stubGlobal("fetch", fetchPage);
    const getEntries = vi.spyOn(performance, "getEntriesByName").mockReturnValue([{
      duration: 15, startTime: 100, responseStart: 110, responseEnd: 115,
      transferSize, encodedBodySize: 123, decodedBodySize: 123,
    } as PerformanceResourceTiming]);
    const records: DebugLogRecord[] = [];
    const unobserve = observeDebugLog((record) => records.push(record));
    const store = Reflect.construct(NavKvStore, [
      { nav_kv_insert_resource: vi.fn() }, 17, "http://fixture.test/nav_db/root",
    ]) as TestableNavKvStore;
    try {
      await store.ensureNavKvPage(1);
      expect(records.find((record) => record.tag === "nav_kv.page.fetch_detail")?.data).toMatchObject({
        transfer_size: transferSize, encoded_body_size: 123, resource_duration_ms: 15,
      });
      expect(getEntries).toHaveBeenCalledWith(fetchPage.mock.calls[0]?.[0], "resource");
    } finally {
      unobserve();
      getEntries.mockRestore();
      vi.unstubAllGlobals();
    }
  });

  it("yields between page installations without scheduling clamped timers", async () => {
    const timer = vi.spyOn(globalThis, "setTimeout");
    const originalFetch = globalThis.fetch;
    globalThis.fetch = vi.fn(async () => new Response(new Uint8Array([1, 2, 3]))) as typeof fetch;
    const insert = vi.fn();
    const store = Reflect.construct(NavKvStore, [
      { nav_kv_insert_resource: insert }, 17, "http://fixture.test/nav_db/root",
    ]) as TestableNavKvStore;
    try {
      await Promise.all([store.ensureNavKvPage(1), store.ensureNavKvPage(2)]);
      expect(insert).toHaveBeenCalledTimes(2);
      expect(timer).not.toHaveBeenCalled();
    } finally {
      globalThis.fetch = originalFetch;
      timer.mockRestore();
    }
  });

  it("dispatches the entire frontier without waiting for responses or installations", async () => {
    const responses: Array<() => void> = [];
    const fetchPage = vi.fn(() => new Promise<Response>((resolve) => {
      responses.push(() => resolve(new Response(new Uint8Array([1, 2, 3]))));
    }));
    vi.stubGlobal("fetch", fetchPage);
    let finishInsert!: () => void;
    const blockedInsert = new Promise<void>((resolve) => { finishInsert = resolve; });
    const insert = vi.fn(() => blockedInsert);
    const store = Reflect.construct(NavKvStore, [
      { nav_kv_insert_resource: insert }, 17, "http://fixture.test/nav_db/root",
    ]) as TestableNavKvStore;
    const requests = Array.from({ length: 32 }, (_, page) => store.ensureNavKvPage(page));
    try {
      expect(fetchPage).toHaveBeenCalledTimes(32);
      expect(store.ensureNavKvPage(8)).toBe(requests[8]);
      expect(fetchPage).toHaveBeenCalledTimes(32);
      responses.forEach((resolve) => resolve());
      await vi.waitFor(() => expect(insert).toHaveBeenCalledOnce());
      requests.push(store.ensureNavKvPage(32));
      expect(fetchPage).toHaveBeenCalledTimes(33);
      responses[32]();
      finishInsert();
      await Promise.all(requests);
      expect(insert).toHaveBeenCalledTimes(33);
    } finally {
      finishInsert();
      // Drain even a capped implementation so a red assertion does not leak work.
      for (let i = 0; i < 100; i += 1) {
        responses.splice(0).forEach((resolve) => resolve());
        await new Promise<void>((resolve) => setTimeout(resolve, 1));
      }
      await Promise.all(requests);
      vi.unstubAllGlobals();
    }
  });

  it("recovers one transient page transport failure without retrying the user action", async () => {
    const originalFetch = globalThis.fetch;
    const fetchPage = vi.fn()
      .mockRejectedValueOnce(new TypeError("temporary network failure"))
      .mockResolvedValueOnce(new Response(new Uint8Array([1, 2, 3]), { status: 200 }));
    globalThis.fetch = fetchPage as typeof fetch;
    const insertResource = vi.fn();
    const store = Reflect.construct(NavKvStore, [
      { nav_kv_insert_resource: insertResource },
      17,
      "http://fixture.test/nav_db/root",
    ]) as TestableNavKvStore;

    try {
      await expect(store.ensureNavKvPage(7)).resolves.toBeUndefined();

      expect(fetchPage).toHaveBeenCalledTimes(2);
      expect(insertResource).toHaveBeenCalledOnce();
    } finally {
      globalThis.fetch = originalFetch;
    }
  });

  it("evicts a page after its transport retry budget is exhausted", async () => {
    const originalFetch = globalThis.fetch;
    const fetchPage = vi.fn()
      .mockRejectedValueOnce(new TypeError("first failure"))
      .mockRejectedValueOnce(new TypeError("second failure"))
      .mockResolvedValueOnce(new Response(new Uint8Array([1, 2, 3]), { status: 200 }));
    globalThis.fetch = fetchPage as typeof fetch;
    const store = Reflect.construct(NavKvStore, [
      { nav_kv_insert_resource: vi.fn() },
      17,
      "http://fixture.test/nav_db/root",
    ]) as TestableNavKvStore;

    try {
      await expect(store.ensureNavKvPage(7)).rejects.toThrow("after 2 attempts: second failure");
      await expect(store.ensureNavKvPage(7)).resolves.toBeUndefined();
      expect(fetchPage).toHaveBeenCalledTimes(3);
    } finally {
      globalThis.fetch = originalFetch;
    }
  });
});

describe("resolvePublicResourceUrl", () => {
  it("resolves public live-feed member resources against the configured live-feed origin", () => {
    expect(resolvePublicResourceUrl(
      {
        id: "live_obstacle_had/obstacles-v1/root",
        source: {
          kind: "public_url",
          url: "/live-feeds/v3/states/obstacles/obstacles-v1/root",
        },
      },
      "http://feeds.example.test:18080",
      { location: { origin: "http://app.example.test" } },
    )).toBe("http://feeds.example.test:18080/live-feeds/v3/states/obstacles/obstacles-v1/root");
  });

  it("leaves non-live-feed public resources unchanged", () => {
    expect(resolvePublicResourceUrl(
      {
        id: "cycle/manifest",
        source: {
          kind: "public_url",
          url: "/packages/cycle/manifest.json",
        },
      },
      "http://feeds.example.test:18080",
      { location: { origin: "http://app.example.test" } },
    )).toBe("/packages/cycle/manifest.json");
  });
});
