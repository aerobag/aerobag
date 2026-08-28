#!/usr/bin/env node

// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { createReadStream, existsSync, readFileSync, statSync } from "node:fs";
import { createHash } from "node:crypto";
import { createServer, request as httpRequest } from "node:http";
import { extname, join, resolve, sep } from "node:path";
import { pathToFileURL } from "node:url";

const LIVE_FEED_SCHEMA_VERSION = 3;
const LIVE_FEED_PREFIX = `/live-feeds/v${LIVE_FEED_SCHEMA_VERSION}`;

function parseArgs(argv) {
  const args = {
    fixture: process.env.AEROBAG_RELEASE_JOURNEY_FIXTURE ?? "",
    webDist: process.env.AEROBAG_RELEASE_WEB_DIST ?? "",
    liveFeedProfile: process.env.AEROBAG_RELEASE_LIVE_FEED_PROFILE ?? "fresh",
    cloudOrigin: process.env.AEROBAG_RELEASE_CLOUD_ORIGIN ?? "http://127.0.0.1:18094",
    host: process.env.HOST ?? "127.0.0.1",
    port: Number(process.env.PORT ?? 18093),
  };
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    if (argument === "--fixture") args.fixture = argv[++index];
    else if (argument === "--web-dist") args.webDist = argv[++index];
    else if (argument === "--live-feed-profile") args.liveFeedProfile = argv[++index];
    else if (argument === "--cloud-origin") args.cloudOrigin = argv[++index];
    else if (argument === "--host") args.host = argv[++index];
    else if (argument === "--port") args.port = Number(argv[++index]);
    else throw new Error(`unknown argument ${argument}`);
  }
  if (!args.fixture) throw new Error("--fixture is required");
  if (!Number.isInteger(args.port) || args.port < 1 || args.port > 65535) {
    throw new Error(`invalid --port ${args.port}`);
  }
  const cloudOrigin = new URL(args.cloudOrigin);
  if (cloudOrigin.protocol !== "http:" || cloudOrigin.pathname !== "/") {
    throw new Error(`--cloud-origin must be an HTTP origin, got ${args.cloudOrigin}`);
  }
  args.cloudOrigin = cloudOrigin.origin;
  return args;
}

function inside(root, relative) {
  const candidate = resolve(root, relative);
  const prefix = root.endsWith(sep) ? root : `${root}${sep}`;
  return candidate === root || candidate.startsWith(prefix) ? candidate : null;
}

export function decodeReleaseJourneyFixturePath(encoded) {
  if (!/^(?:[0-9a-f]{2})+$/.test(encoded)) return null;
  const decoded = Buffer.from(encoded, "hex").toString("utf8");
  return Buffer.from(decoded, "utf8").toString("hex") === encoded ? decoded : null;
}

export function webDistIndexSha256(webDist) {
  if (!webDist) return null;
  return createHash("sha256").update(readFileSync(join(webDist, "index.html"))).digest("hex");
}

function contentType(path) {
  switch (extname(path).toLowerCase()) {
    case ".html": return "text/html; charset=utf-8";
    case ".css": return "text/css; charset=utf-8";
    case ".js": case ".mjs": return "text/javascript; charset=utf-8";
    case ".json": case ".jsonl": return "application/json; charset=utf-8";
    case ".wasm": return "application/wasm";
    case ".svg": return "image/svg+xml";
    case ".png": return "image/png";
    case ".webp": return "image/webp";
    case ".jpg": case ".jpeg": return "image/jpeg";
    default: return "application/octet-stream";
  }
}

function sendFile(request, response, root, relative, cacheControl = "no-cache") {
  const file = inside(root, relative.replace(/^\/+/, ""));
  if (!file || !existsSync(file) || !statSync(file).isFile()) return false;
  response.statusCode = 200;
  response.setHeader("Content-Type", contentType(file));
  response.setHeader("Content-Length", String(statSync(file).size));
  response.setHeader("Cache-Control", cacheControl);
  if (request.method === "HEAD") response.end();
  else createReadStream(file).pipe(response);
  return true;
}

function sendBytes(request, response, bytes, contentTypeValue = "application/json; charset=utf-8") {
  response.statusCode = 200;
  response.setHeader("Content-Type", contentTypeValue);
  response.setHeader("Content-Length", String(bytes.length));
  response.setHeader("Cache-Control", "no-cache");
  if (request.method === "HEAD") response.end();
  else response.end(bytes);
}

function proxyCloudRequest(request, response, cloudOrigin) {
  const target = new URL(request.url ?? "/cloud/", cloudOrigin);
  const upstream = httpRequest(target, {
    method: request.method,
    headers: { ...request.headers, host: target.host },
  }, (upstreamResponse) => {
    response.writeHead(upstreamResponse.statusCode ?? 502, upstreamResponse.headers);
    upstreamResponse.pipe(response);
  });
  upstream.on("error", (error) => {
    if (response.headersSent) {
      response.destroy(error);
      return;
    }
    response.statusCode = 502;
    response.end(`cloud proxy failed: ${error.message}\n`);
  });
  request.pipe(upstream);
}

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

export function publicationVariants(publicationRoot) {
  const primaryBytes = readFileSync(join(publicationRoot, "current_artifacts.json"));
  const primary = JSON.parse(primaryBytes.toString("utf8"));
  if (!Array.isArray(primary) || primary.length === 0) {
    throw new Error("fixture current_artifacts.json must be a non-empty list");
  }
  const active = primary.at(-1);
  const bundleDescriptor = active?.bundles?.[0];
  const packagedRoot = active?.artifact_roots?.packaged;
  if (!bundleDescriptor || typeof packagedRoot !== "string") {
    throw new Error("fixture publication has no packaged bundle");
  }
  const originalBundleRelative = `${packagedRoot}${bundleDescriptor.relative_path}`;
  const originalBundle = JSON.parse(readFileSync(join(publicationRoot, originalBundleRelative), "utf8"));
  const packageIndex = originalBundle.packages.findIndex((entry) =>
    entry.family_id === "csup" && entry.region_id === "nw");
  if (packageIndex < 0) throw new Error("fixture bundle has no NW CSUP package for update testing");
  const originalPackage = originalBundle.packages[packageIndex];
  const updatedFilename = originalPackage.filename.replace(/\.zip$/, ".release-e2e-update.zip");
  const updatedPackage = {
    ...originalPackage,
    id: `${originalPackage.id}_RELEASE_E2E_UPDATE`,
    filename: updatedFilename,
    relative_path: updatedFilename,
    cycle: active.bundles[0].cycle,
    cycle_version: "02",
  };
  const updatedBundle = {
    ...originalBundle,
    bundle_id: `${originalBundle.bundle_id}_release_e2e_update`,
    cycle_version: "02",
    packages: originalBundle.packages.map((entry, index) =>
      index === packageIndex ? updatedPackage : entry),
  };
  const updatedBundleBytes = Buffer.from(`${JSON.stringify(updatedBundle)}\n`);
  const updatedBundleHash = sha256(updatedBundleBytes);
  const updatedBundleFilename = `bundle_release_e2e_update_${updatedBundleHash}.json`;
  const updated = JSON.parse(JSON.stringify(primary));
  updated[updated.length - 1] = {
    ...updated.at(-1),
    bundles: [{
      ...bundleDescriptor,
      id: updatedBundle.bundle_id,
      filename: updatedBundleFilename,
      relative_path: updatedBundleFilename,
      cycle_version: "02",
      checksum_sha256: updatedBundleHash,
      size_bytes: updatedBundleBytes.length,
    }],
  };
  const unsupported = JSON.parse(JSON.stringify(primary));
  for (const manifest of unsupported) manifest.contracts["nav-db"] = "NAV_UNSUPPORTED";
  return {
    primary: primaryBytes,
    updated: Buffer.from(`${JSON.stringify(updated)}\n`),
    unsupported: Buffer.from(`${JSON.stringify(unsupported)}\n`),
    updatedBundlePath: `${packagedRoot}${updatedBundleFilename}`,
    updatedBundleBytes,
    updatedArtifactPath: `${packagedRoot}${updatedFilename}`,
    updatedArtifactFilename: updatedFilename,
    updatedArtifactId: updatedPackage.id,
    originalArtifactPath: `${packagedRoot}${originalPackage.relative_path}`,
  };
}

export function liveFeedEventsFromCurrent(current) {
  if (current?.schema_version !== LIVE_FEED_SCHEMA_VERSION) {
    throw new Error(`fixture live-feed schema is ${current?.schema_version}; expected ${LIVE_FEED_SCHEMA_VERSION}`);
  }
  return [{
    id: `catalog:${current.generated_at_utc}`,
    event: "live-feed-catalog",
    payload: current,
  }];
}

export function fixtureServerConfiguration(args) {
  const fixturePath = resolve(args.fixture);
  const fixtureRoot = resolve(fixturePath, "..");
  const fixture = JSON.parse(readFileSync(fixturePath, "utf8"));
  const publicationRoot = inside(fixtureRoot, fixture.publication_root);
  const profileRelative = fixture.capabilities?.live_feeds?.[args.liveFeedProfile];
  const liveFeedRoot = typeof profileRelative === "string" ? inside(fixtureRoot, profileRelative) : null;
  if (!publicationRoot || !existsSync(join(publicationRoot, "current_artifacts.json"))) {
    throw new Error(`fixture publication root is unavailable: ${fixture.publication_root}`);
  }
  if (!liveFeedRoot || !existsSync(join(liveFeedRoot, "current.json"))) {
    throw new Error(`fixture live-feed profile is unavailable: ${args.liveFeedProfile}`);
  }
  const webDist = args.webDist ? resolve(args.webDist) : null;
  if (webDist && !existsSync(join(webDist, "index.html"))) {
    throw new Error(`web dist has no index.html: ${webDist}`);
  }
  const webDistIndexHash = webDistIndexSha256(webDist);
  const current = JSON.parse(readFileSync(join(liveFeedRoot, "current.json"), "utf8"));
  return {
    fixture,
    fixtureRoot,
    publicationRoot,
    liveFeedRoot,
    current,
    events: liveFeedEventsFromCurrent(current),
    webDist,
    webDistIndexSha256: webDistIndexHash,
    publicationVariants: publicationVariants(publicationRoot),
  };
}

export function createReleaseJourneyFixtureServer(args) {
  const config = fixtureServerConfiguration(args);
  const recentRequests = [];
  const control = {
    publication: "primary",
    artifact_fault: "none",
    dropped_artifact_requests: 0,
    completed_update_artifact_requests: 0,
  };
  return createServer((request, response) => {
    const requestDiagnostic = {
      method: request.method,
      url: request.url,
      started_at_ms: Date.now(),
      status: null,
      outcome: "active",
    };
    recentRequests.push(requestDiagnostic);
    if (recentRequests.length > 500) recentRequests.splice(0, recentRequests.length - 500);
    response.once("finish", () => {
      requestDiagnostic.status = response.statusCode;
      requestDiagnostic.outcome = "finished";
      requestDiagnostic.finished_at_ms = Date.now();
    });
    response.once("close", () => {
      if (requestDiagnostic.outcome === "active") {
        requestDiagnostic.status = response.statusCode;
        requestDiagnostic.outcome = "closed";
        requestDiagnostic.finished_at_ms = Date.now();
      }
    });
    response.setHeader("Access-Control-Allow-Origin", "*");
    response.setHeader("Access-Control-Allow-Headers", "content-type");
    response.setHeader("Access-Control-Allow-Methods", "GET, HEAD, OPTIONS");
    if (request.method === "OPTIONS") {
      response.statusCode = 204;
      response.end();
      return;
    }
    const url = new URL(request.url ?? "/", "http://fixture.invalid");
    const pathname = decodeURIComponent(url.pathname);
    if (request.method === "POST" && pathname === "/__control") {
      let body = "";
      request.setEncoding("utf8");
      request.on("data", (chunk) => {
        body += chunk;
        if (body.length > 16_384) request.destroy();
      });
      request.on("end", () => {
        try {
          const update = JSON.parse(body || "{}");
          if (update.reset === true) {
            control.publication = "primary";
            control.artifact_fault = "none";
            control.dropped_artifact_requests = 0;
            control.completed_update_artifact_requests = 0;
            recentRequests.splice(0, Math.max(0, recentRequests.length - 1));
          }
          if (update.publication !== undefined) {
            if (!["primary", "updated", "unsupported"].includes(update.publication)) {
              throw new Error(`unsupported publication mode ${update.publication}`);
            }
            control.publication = update.publication;
          }
          if (update.artifact_fault !== undefined) {
            if (!["none", "drop"].includes(update.artifact_fault)) {
              throw new Error(`unsupported artifact fault ${update.artifact_fault}`);
            }
            control.artifact_fault = update.artifact_fault;
          }
          response.setHeader("Content-Type", "application/json; charset=utf-8");
          response.end(JSON.stringify({
            ...control,
            updated_artifact_filename: config.publicationVariants.updatedArtifactFilename,
            updated_artifact_id: config.publicationVariants.updatedArtifactId,
          }));
        } catch (error) {
          response.statusCode = 400;
          response.end(`${error.message}\n`);
        }
      });
      return;
    }
    if (pathname === "/cloud" || pathname.startsWith("/cloud/")) {
      proxyCloudRequest(request, response, args.cloudOrigin);
      return;
    }
    if (request.method !== "GET" && request.method !== "HEAD") {
      response.statusCode = 405;
      response.end("method not allowed\n");
      return;
    }
    if (pathname === "/__health") {
      response.setHeader("Content-Type", "application/json; charset=utf-8");
      response.end(JSON.stringify({
        status: "ok",
        fixture: config.fixture.fixture,
        live_feed_profile: args.liveFeedProfile,
        serves_web_app: Boolean(config.webDist),
        web_dist_index_sha256: config.webDistIndexSha256,
        cloud_origin: args.cloudOrigin,
        product_count: Object.keys(config.current.products ?? {}).length,
        control,
        updated_artifact_filename: config.publicationVariants.updatedArtifactFilename,
      }));
      return;
    }
    if (pathname === "/__requests") {
      response.setHeader("Content-Type", "application/json; charset=utf-8");
      response.end(JSON.stringify(recentRequests));
      return;
    }
    if (pathname.endsWith(".aerobag-e2e-stall")) {
      // Deliberately leave the image request unresolved. The browser closes it
      // when the product watchdog remounts that tile with its recovery URL.
      request.on("close", () => response.destroy());
      return;
    }
    if (pathname === "/live-feeds/status.html") {
      response.setHeader("Content-Type", "text/html; charset=utf-8");
      response.end("<!doctype html><title>Aerobag release-journey live feeds</title><p>Deterministic fixture.</p>");
      return;
    }
    if (pathname === "/live-feeds/status.json") {
      response.setHeader("Content-Type", "application/json; charset=utf-8");
      response.end(JSON.stringify({ schema_version: 1, fixture: true, profile: args.liveFeedProfile }));
      return;
    }
    if (pathname === `${LIVE_FEED_PREFIX}/events`) {
      response.writeHead(200, {
        "Content-Type": "text/event-stream; charset=utf-8",
        "Cache-Control": "no-cache, no-transform",
        Connection: "keep-alive",
        "Access-Control-Allow-Origin": "*",
      });
      response.write(": aerobag deterministic release fixture\n\n");
      for (const event of config.events) {
        response.write(`id: ${event.id}\n`);
        response.write(`event: ${event.event}\n`);
        response.write(`data: ${JSON.stringify(event.payload)}\n\n`);
      }
      const heartbeat = setInterval(() => {
        response.write(`event: live-feed-heartbeat\ndata: ${JSON.stringify({ schema_version: LIVE_FEED_SCHEMA_VERSION, products: [] })}\n\n`);
      }, 15_000);
      request.on("close", () => clearInterval(heartbeat));
      return;
    }
    if (pathname === `${LIVE_FEED_PREFIX}/current.json`) {
      if (sendFile(request, response, config.liveFeedRoot, "current.json")) return;
    }
    if (pathname.startsWith(`${LIVE_FEED_PREFIX}/`)) {
      if (sendFile(
        request,
        response,
        config.liveFeedRoot,
        pathname.slice(`${LIVE_FEED_PREFIX}/`.length),
        "public, max-age=31536000, immutable",
      )) return;
    }
    if (pathname === "/packages" || pathname.startsWith("/packages/")) {
      const relative = pathname === "/packages"
        ? "current_artifacts.json"
        : pathname.slice("/packages/".length);
      if (relative === "current_artifacts.json") {
        sendBytes(request, response, config.publicationVariants[control.publication]);
        return;
      }
      if (relative === config.publicationVariants.updatedBundlePath) {
        sendBytes(request, response, config.publicationVariants.updatedBundleBytes);
        return;
      }
      if (relative === config.publicationVariants.updatedArtifactPath) {
        const original = join(config.publicationRoot, config.publicationVariants.originalArtifactPath);
        if (control.artifact_fault === "drop" && request.method !== "HEAD") {
          control.dropped_artifact_requests += 1;
          response.writeHead(200, {
            "Content-Type": "application/octet-stream",
            "Content-Length": String(statSync(original).size),
          });
          const stream = createReadStream(original, { start: 0, end: 16_383 });
          stream.pipe(response, { end: false });
          stream.on("end", () => response.destroy());
          return;
        }
        control.completed_update_artifact_requests += 1;
        if (sendFile(request, response, config.publicationRoot, config.publicationVariants.originalArtifactPath,
          "public, max-age=31536000, immutable")) return;
      }
      if (sendFile(request, response, config.publicationRoot, relative,
        relative === "current_artifacts.json" ? "no-cache" : "public, max-age=31536000, immutable")) return;
    }
    if (pathname.startsWith("/release-journey/")) {
      if (sendFile(request, response, config.fixtureRoot, pathname.slice("/release-journey/".length))) return;
    }
    if (pathname.startsWith("/releasejourney/")) {
      const relative = decodeReleaseJourneyFixturePath(pathname.slice("/releasejourney/".length));
      if (relative && sendFile(request, response, config.fixtureRoot, relative)) return;
    }
    if (config.webDist) {
      const relative = pathname === "/" ? "index.html" : pathname.slice(1);
      if (sendFile(request, response, config.webDist, relative)) return;
      if (!extname(pathname) && sendFile(request, response, config.webDist, "index.html")) return;
    }
    response.statusCode = 404;
    response.end("not found\n");
  });
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  const server = createReleaseJourneyFixtureServer(args);
  await new Promise((resolvePromise, reject) => {
    server.once("error", reject);
    server.listen(args.port, args.host, resolvePromise);
  });
  process.stdout.write(`release-journey fixture server http://${args.host}:${args.port}/ profile=${args.liveFeedProfile}\n`);
  for (const signal of ["SIGINT", "SIGTERM"]) {
    process.on(signal, () => server.close(() => process.exit(0)));
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main().catch((error) => {
    console.error(error.stack ?? error.message);
    process.exitCode = 1;
  });
}
