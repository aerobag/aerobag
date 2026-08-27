// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { describe, expect, expectTypeOf, it, vi } from "vitest";
import {
  completeResourceFreeSessionMutation,
  NAV_KV_PAGE_FETCH_CONCURRENCY,
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
  it("bounds concurrent page fetches while retaining every request", async () => {
    const originalFetch = globalThis.fetch;
    const fetchPage = vi.fn(() => Promise.resolve(
      new Response(new Uint8Array([1, 2, 3]), { status: 200 }),
    ));
    globalThis.fetch = fetchPage as typeof fetch;
    const store = Reflect.construct(NavKvStore, [
      { nav_kv_insert_resource: vi.fn() },
      17,
      "http://fixture.test/nav_db/root",
    ]) as TestableNavKvStore;

    try {
      const requests = Array.from(
        { length: NAV_KV_PAGE_FETCH_CONCURRENCY + 1 },
        (_, pageIndex) => store.ensureNavKvPage(pageIndex),
      );
      expect(fetchPage).toHaveBeenCalledTimes(NAV_KV_PAGE_FETCH_CONCURRENCY);
      await Promise.all(requests);
      expect(fetchPage).toHaveBeenCalledTimes(NAV_KV_PAGE_FETCH_CONCURRENCY + 1);
    } finally {
      globalThis.fetch = originalFetch;
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
