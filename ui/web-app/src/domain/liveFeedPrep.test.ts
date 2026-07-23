import { describe, expect, it, vi } from "vitest";
import { ingestPreparedLiveFeedResource } from "./liveFeedPrep";

describe("live-feed preparation routing", () => {
  it.each(["metars", "tafs", "tfrs", "notams"])(
    "routes %s state through the preparation boundary",
    async (product) => {
      const resourceId = `live_feeds/state/${product}/v1`;
      const rawBytes = new Uint8Array([1, 2, 3]);
      const preparedBytes = new Uint8Array([4, 5]);
      const prepare = vi.fn(async () => preparedBytes);
      const ingest = vi.fn(async () => undefined);
      const shouldPrepare = vi.fn(() => true);

      await expect(ingestPreparedLiveFeedResource(
        17,
        resourceId,
        rawBytes,
        ingest,
        shouldPrepare,
        prepare,
      )).resolves.toBe(true);

      expect(prepare).toHaveBeenCalledWith(resourceId, rawBytes);
      expect(ingest).toHaveBeenCalledWith(17, resourceId, preparedBytes);
      expect(shouldPrepare).toHaveBeenCalledWith(resourceId);
    },
  );

  it("leaves unrelated core resources on the ordinary ingestion path", async () => {
    const prepare = vi.fn(async () => new Uint8Array());
    const ingest = vi.fn(async () => undefined);

    await expect(ingestPreparedLiveFeedResource(
      17,
      "live_feeds/state/obstacles/v1",
      new Uint8Array(),
      ingest,
      () => false,
      prepare,
    )).resolves.toBe(false);

    expect(prepare).not.toHaveBeenCalled();
    expect(ingest).not.toHaveBeenCalled();
  });
});
