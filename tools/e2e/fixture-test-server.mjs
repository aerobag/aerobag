// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import { EventEmitter, once } from "node:events";
import { mkdtemp, mkdir, writeFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { request } from "node:http";
import { createReleaseJourneyFixtureServer } from "./serve-release-journey-fixture.mjs";
import { liveFeedPath } from "./live-feed-contract-paths.mjs";

// Tiny server-control input, not a relabeled NAVDB/app qualification fixture.
export async function fixtureServer(t) {
  const directory = await mkdtemp(join(tmpdir(), "aerobag-fixture-isolation-"));
  await mkdir(join(directory, "publication"));
  await mkdir(join(directory, "live"));
  const json = (file, data) => writeFile(join(directory, file), JSON.stringify(data));
  await json("fixture.json", { fixture: "server-control-test", publication_root: "publication", capabilities: { live_feeds: { fresh: "live" } } });
  await json("publication/current_artifacts.json", [{ artifact_roots: { packaged: "" }, bundles: [{ relative_path: "bundle.json", cycle: "test" }], contracts: {} }]);
  await json("publication/bundle.json", { bundle_id: "test", packages: [{ id: "test", family_id: "csup", region_id: "nw", filename: "test.zip", relative_path: "test.zip" }] });
  await json("live/current.json", { schema_version: 3, generated_at_utc: "2026-01-01T00:00:00Z", products: {} });
  await writeFile(join(directory, "payload.txt"), "payload");
  await mkdir(join(directory, "publication/map/tiles"), { recursive: true });
  await writeFile(join(directory, "publication/map/tiles/test.webp"), "tile bytes");
  const server = createReleaseJourneyFixtureServer({ fixture: join(directory, "fixture.json"), liveFeedProfile: "fresh" });
  server.listen(0, "127.0.0.1");
  await once(server, "listening");
  t.after(async () => {
    server.closeAllConnections();
    await new Promise((resolve) => server.close(resolve));
    await rm(directory, { recursive: true, force: true });
  });
  return { server, origin: `http://127.0.0.1:${server.address().port}` };
}

export async function liveFeedStream(t, origin, host = new URL(origin).host) {
  const events = new EventEmitter();
  const hints = [];
  let pending = "", response;
  const req = request(new URL(liveFeedPath("events"), origin), {
    agent: false, headers: { host },
  });
  req.on("error", () => {});
  t.after(() => req.destroy());
  req.end();
  [response] = await once(req, "response", { signal: AbortSignal.timeout(2_000) });
  response.setEncoding("utf8");
  response.on("error", () => {}); // Fixture reset deliberately tears down SSE.
  response.on("data", (chunk) => {
    pending += chunk;
    let end;
    while ((end = pending.indexOf("\n\n")) >= 0) {
      const frame = pending.slice(0, end);
      pending = pending.slice(end + 2);
      if (!frame.includes("event: service-bulletins\n")) continue;
      const data = frame.split("\n").find((line) => line.startsWith("data: "));
      hints.push(JSON.parse(data.slice(6)));
      events.emit("hint");
    }
  });
  return {
    response, hints,
    close: () => req.destroy(),
    async waitForHint(revision) {
      const signal = AbortSignal.timeout(2_000);
      while (!hints.some((hint) => hint.revision === revision)) {
        await once(events, "hint", { signal });
      }
      return hints.find((hint) => hint.revision === revision);
    },
  };
}
