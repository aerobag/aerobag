// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { describe, expect, it } from "vitest";
import { ResourceIngestCoordinator, resolvePublicResourceUrl } from "./navKv";

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
