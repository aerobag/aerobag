import { describe, expect, it } from "vitest";
import { resolvePublicResourceUrl } from "./navKv";

describe("resolvePublicResourceUrl", () => {
  it("resolves public live-feed member resources against the configured live-feed origin", () => {
    expect(resolvePublicResourceUrl(
      {
        id: "live_obstacle_had/obstacles-v1/root",
        source: {
          kind: "public_url",
          url: "/live-feeds/v2/states/obstacles/obstacles-v1/root",
        },
      },
      "http://feeds.example.test:18080",
      { location: { origin: "http://app.example.test" } },
    )).toBe("http://feeds.example.test:18080/live-feeds/v2/states/obstacles/obstacles-v1/root");
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
