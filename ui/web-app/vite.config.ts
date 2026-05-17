import fs from "node:fs";
import type { IncomingMessage, ServerResponse } from "node:http";
import path from "node:path";
import zlib from "node:zlib";
import { defineConfig, type Plugin } from "vite";
import react from "@vitejs/plugin-react";

const repoRoot = process.env.AEROBAG_REPO_ROOT
  ? path.resolve(process.env.AEROBAG_REPO_ROOT)
  : path.resolve(__dirname, "../..");
const targetRootFile = path.join(repoRoot, "ui", "target-root.txt");
const configuredTargetRoot = fs.readFileSync(targetRootFile, "utf8").trim();
const uiTargetRoot = process.env.AEROBAG_UI_TARGET_ROOT
  ? path.resolve(process.env.AEROBAG_UI_TARGET_ROOT)
  : path.resolve(repoRoot, configuredTargetRoot);
const webSourceRoot = path.join(repoRoot, "ui", "web-app");
const webTargetRoot = path.join(uiTargetRoot, "web");
const workspaceRoot = path.join(webTargetRoot, "workspace");
const generatedRoot = path.join(webTargetRoot, "generated");
const iconsRoot = path.join(repoRoot, "ui", "icons");
const adsbTraceRoot = path.resolve(repoRoot, "..", "adsb-traces");
const liveFeedsRoot = process.env.AEROBAG_LIVE_FEEDS_ROOT
  ? path.resolve(process.env.AEROBAG_LIVE_FEEDS_ROOT)
  : path.resolve(repoRoot, "..", "live-feeds");
const liveFeedEventIntervalMs = Number.parseInt(process.env.AEROBAG_LIVE_FEED_EVENT_INTERVAL_MS ?? "5000", 10);
const sharedRoot = path.join(repoRoot, "ui", "shared");
const sharedFixturesRoot = path.join(repoRoot, "ui", "shared-fixtures");
const debugLogPath = path.join("/tmp", "aerobag-web-debug.log");
const requestLogPath = path.join("/tmp", "aerobag-web-requests.log");
const artifactReadPathConfigFile = path.join(repoRoot, ".aerobag-artifact-read-path");
const configuredArtifactRoot = fs.readFileSync(artifactReadPathConfigFile, "utf8").trim();
const configuredArtifactPath = path.isAbsolute(configuredArtifactRoot)
  ? configuredArtifactRoot
  : path.resolve(repoRoot, configuredArtifactRoot);
const artifactReadRoot = process.env.AEROBAG_ARTIFACT_READ_PATH
  ? path.resolve(process.env.AEROBAG_ARTIFACT_READ_PATH)
  : configuredArtifactPath;
function latestCurrentArtifacts(root: string): string | null {
  const current = path.join(root, "current_artifacts.json");
  return fs.existsSync(current) ? current : null;
}
const currentArtifactsPath = latestCurrentArtifacts(artifactReadRoot) ?? path.join(artifactReadRoot, "current_artifacts_missing.json");
const currentArtifacts = JSON.parse(fs.readFileSync(currentArtifactsPath, "utf8")) as {
  artifact_roots?: { packaged?: string; unpacked?: string };
};
const expectedArtifactRoots = {
  packaged: "published_packaged/",
  unpacked: "published_unpacked/",
};
if (
  currentArtifacts.artifact_roots?.packaged !== expectedArtifactRoots.packaged
  || currentArtifacts.artifact_roots?.unpacked !== expectedArtifactRoots.unpacked
) {
  throw new Error(`${currentArtifactsPath} has artifact_roots=${JSON.stringify(currentArtifacts.artifact_roots)}; expected ${JSON.stringify(expectedArtifactRoots)}`);
}
function appendRequestLog(entry: Record<string, unknown>) {
  fs.appendFileSync(requestLogPath, `${JSON.stringify({ ts: Date.now(), ...entry })}\n`);
}

function mountStaticTree(sourceRoot: string, options: { missingStatus?: number; logPrefix?: string } = {}) {
  return (req: { headers?: Record<string, string | string[] | undefined>; url?: string }, res: { statusCode: number; end: (body?: string) => void; setHeader: (name: string, value: string) => void }, next: () => void) => {
    const requestPath = decodeURIComponent((req.url ?? "/").split("?")[0] ?? "/");
    const relativePath = requestPath.replace(/^\/+/, "");
    const filePath = path.resolve(sourceRoot, relativePath);
    if (!filePath.startsWith(sourceRoot)) {
      res.statusCode = 403;
      if (options.logPrefix) {
        appendRequestLog({ kind: `${options.logPrefix}.forbidden`, url: req.url ?? "", file_path: filePath });
      }
      res.end("forbidden");
      return;
    }
    if (!fs.existsSync(filePath) || !fs.statSync(filePath).isFile()) {
      if (options.missingStatus) {
        res.statusCode = options.missingStatus ?? 404;
        if (options.logPrefix) {
          appendRequestLog({ kind: `${options.logPrefix}.missing`, url: req.url ?? "", file_path: filePath, status: res.statusCode });
        }
        res.end("not found");
        return;
      }
      next();
      return;
    }
    const extension = path.extname(filePath).toLowerCase();
    const contentType =
      extension === ".webp"
        ? "image/webp"
        : extension === ".png"
          ? "image/png"
          : extension === ".terrain"
            ? "application/vnd.aerobag.terrain"
          : extension === ".db"
            ? "application/vnd.sqlite3"
          : extension === ".json"
            ? "application/json"
          : extension === ".html"
            ? "text/html; charset=utf-8"
            : "application/octet-stream";
    const acceptEncoding = req.headers?.["accept-encoding"] ?? "";
    const shouldCompress = extension === ".db" || extension === ".json";
    const supportsBrotli = typeof acceptEncoding === "string" && /\bbr\b/.test(acceptEncoding);
    const supportsGzip = typeof acceptEncoding === "string" && /\bgzip\b/.test(acceptEncoding);
    res.setHeader("Content-Type", contentType);
    res.setHeader("Vary", "Accept-Encoding");
    const stream = fs.createReadStream(filePath);
    if (extension === ".terrain") {
      res.setHeader("Content-Encoding", "gzip");
      stream.pipe(res);
      return;
    }
    if (shouldCompress && supportsBrotli) {
      res.setHeader("Content-Encoding", "br");
      stream.pipe(zlib.createBrotliCompress()).pipe(res);
      return;
    }
    if (shouldCompress && supportsGzip) {
      res.setHeader("Content-Encoding", "gzip");
      stream.pipe(zlib.createGzip()).pipe(res);
      return;
    }
    stream.pipe(res);
  };
}

type LiveFeedCurrentManifest = {
  products?: Record<string, LiveFeedCurrentEntry>;
};

type LiveFeedCurrentEntry = {
  current?: string;
  version_manifest_url?: string;
  state_url?: string;
  state_sha256?: string;
};

type LiveFeedSseEvent = {
  id: string;
  product: string;
  version: string;
  version_manifest_url: string;
  state_url: string;
  state_sha256: string;
};

type LiveFeedVersionManifest = {
  product?: string;
  version?: string;
  state?: {
    url?: string;
    state_sha256?: string;
  };
};

type OrderedLiveFeedSseEvent = {
  sort_key: string;
  event: LiveFeedSseEvent;
};

function readJsonFile<T>(filePath: string): T | null {
  try {
    return JSON.parse(fs.readFileSync(filePath, "utf8")) as T;
  } catch {
    return null;
  }
}

function listLiveFeedVersionEvents(root: string): LiveFeedSseEvent[] {
  const versionsRoot = path.join(root, "versions");
  if (!fs.existsSync(versionsRoot) || !fs.statSync(versionsRoot).isDirectory()) {
    return [];
  }
  const events: OrderedLiveFeedSseEvent[] = [];
  for (const product of fs.readdirSync(versionsRoot).sort()) {
    const productRoot = path.join(versionsRoot, product);
    if (!fs.statSync(productRoot).isDirectory()) {
      continue;
    }
    for (const fileName of fs.readdirSync(productRoot).sort()) {
      if (!fileName.endsWith(".json")) {
        continue;
      }
      const versionManifestPath = path.join(productRoot, fileName);
      const manifest = readJsonFile<LiveFeedVersionManifest>(versionManifestPath);
      const manifestProduct = manifest?.product ?? product;
      const version = manifest?.version ?? fileName.replace(/\.json$/, "");
      const stateUrl = manifest?.state?.url;
      const stateSha256 = manifest?.state?.state_sha256;
      if (!stateUrl || !stateSha256) {
        continue;
      }
      events.push({
        sort_key: `${fileName}:${manifestProduct}`,
        event: {
          id: `${manifestProduct}:${version}`,
          product: manifestProduct,
          version,
          version_manifest_url: `versions/${manifestProduct}/${fileName}`,
          state_url: stateUrl,
          state_sha256: stateSha256,
        },
      });
    }
  }
  return events.sort((a, b) => a.sort_key.localeCompare(b.sort_key)).map(({ event }) => event);
}

function listLiveFeedSseEvents(root: string): LiveFeedSseEvent[] {
  const versionEvents = listLiveFeedVersionEvents(root);
  if (versionEvents.length > 0) {
    return versionEvents;
  }
  const current = readJsonFile<LiveFeedCurrentManifest>(path.join(root, "current.json"));
  const events: LiveFeedSseEvent[] = [];
  for (const [product, entry] of Object.entries(current?.products ?? {})) {
    if (!entry.current || !entry.version_manifest_url || !entry.state_url || !entry.state_sha256) {
      continue;
    }
    events.push({
      id: `${product}:${entry.current}`,
      product,
      version: entry.current,
      version_manifest_url: entry.version_manifest_url,
      state_url: entry.state_url,
      state_sha256: entry.state_sha256,
    });
  }
  return events.sort((a, b) => a.id.localeCompare(b.id));
}

function installLiveFeedFixtureServer(server: { middlewares: { use: (...args: unknown[]) => void } }) {
  server.middlewares.use("/live-feeds/events", (_req: IncomingMessage, res: ServerResponse) => {
    const events = listLiveFeedSseEvents(liveFeedsRoot);
    appendRequestLog({ kind: "live_feeds.events.open", root: liveFeedsRoot, event_count: events.length });
    res.statusCode = 200;
    res.setHeader("Content-Type", "text/event-stream; charset=utf-8");
    res.setHeader("Cache-Control", "no-cache, no-transform");
    res.setHeader("Connection", "keep-alive");
    res.write(`: aerobag live-feed fixture root ${liveFeedsRoot}\n\n`);
    if (events.length === 0) {
      res.write("event: heartbeat\n");
      res.write(`data: ${JSON.stringify({ schema_version: 1, products: [] })}\n\n`);
      return;
    }
    let index = 0;
    const writeNext = () => {
      if (index >= events.length) {
        return;
      }
      const event = events[index];
      index += 1;
      res.write(`id: ${event.id}\n`);
      res.write("event: live-feed-current\n");
      res.write(`data: ${JSON.stringify({ schema_version: 1, ...event })}\n\n`);
    };
    writeNext();
    const interval = setInterval(writeNext, Number.isFinite(liveFeedEventIntervalMs) ? liveFeedEventIntervalMs : 5000);
    res.on("close", () => {
      clearInterval(interval);
      appendRequestLog({ kind: "live_feeds.events.close" });
    });
  });
  server.middlewares.use("/live-feeds", mountStaticTree(liveFeedsRoot, { missingStatus: 404, logPrefix: "live_feeds" }));
}

function aerobagStaticPlugin(): Plugin {
  function installMiddlewares(server: { middlewares: { use: (...args: unknown[]) => void } }) {
    server.middlewares.use("/__debug_log", (req, res, next) => {
      if (req.method !== "POST") {
        next();
        return;
      }
      const chunks: Buffer[] = [];
      req.on("data", (chunk) => {
        chunks.push(Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk));
      });
      req.on("end", () => {
        try {
          const payload = JSON.parse(Buffer.concat(chunks).toString("utf8")) as unknown[];
          const lines = payload.map((entry) => JSON.stringify(entry)).join("\n");
          if (lines.length > 0) {
            fs.appendFileSync(debugLogPath, `${lines}\n`);
            appendRequestLog({ kind: "client_debug_post", count: payload.length });
          }
          res.statusCode = 204;
          res.end();
        } catch (error) {
          res.statusCode = 400;
          res.end(error instanceof Error ? error.message : String(error));
        }
      });
    });
    server.middlewares.use("/__ping", (req, res, next) => {
      const requestPath = decodeURIComponent((req.url ?? "/").split("?")[0] ?? "/");
      if (requestPath !== "/") {
        next();
        return;
      }
      appendRequestLog({ kind: "ping", url: req.url ?? "" });
      res.statusCode = 204;
      res.end();
    });
    server.middlewares.use("/packages", mountStaticTree(artifactReadRoot, { missingStatus: 404, logPrefix: "packages" }));
    installLiveFeedFixtureServer(server);
    server.middlewares.use("/icons", mountStaticTree(iconsRoot, { missingStatus: 404, logPrefix: "icons" }));
    server.middlewares.use("/adsb-traces", mountStaticTree(adsbTraceRoot, { missingStatus: 404, logPrefix: "adsb_traces" }));
    for (const legacyPrefix of [
      "/afd",
      "/fast-products",
      "/files",
      "/nav-db",
      "/nav-kv",
      "/plates",
      "/sectional-packages",
      "/shaded-relief-products",
      "/thumbnails",
      "/world-basemap-products",
    ]) {
      server.middlewares.use(legacyPrefix, (req: { url?: string }, res: { statusCode: number; end: (body?: string) => void }) => {
        res.statusCode = 404;
        appendRequestLog({ kind: "legacy_artifact_route", prefix: legacyPrefix, url: req.url ?? "", status: 404 });
        res.end("artifact route moved under /packages");
      });
    }
  }

  return {
    name: "aerobag-static-assets",
    configureServer(server) {
      installMiddlewares(server);
    },
    configurePreviewServer(server) {
      installMiddlewares(server);
    },
    // Product artifacts are exposed only as the publication contract root:
    // /packages -> artifact root. Vite should not synthesize legacy product
    // routes or copy staged artifacts into dist.
  };
}

export default defineConfig({
  plugins: [react(), aerobagStaticPlugin()],
  resolve: {
    preserveSymlinks: true,
    alias: {
      "@generated": generatedRoot,
      "@shared-bootstrap": path.join(sharedRoot, "dev-bootstrap.json"),
      "@shared-ui-theme": path.join(sharedFixturesRoot, "ui-theme.json"),
    },
  },
  server: {
    port: 4173,
    host: "0.0.0.0",
    allowedHosts: ["aerobag-dev.iac.jonh.net"],
    fs: {
      allow: [
        workspaceRoot,
        webSourceRoot,
        sharedRoot,
        sharedFixturesRoot,
        generatedRoot,
        artifactReadRoot,
        ...(fs.existsSync(adsbTraceRoot) ? [adsbTraceRoot] : []),
      ],
    },
  },
  preview: {
    allowedHosts: ["aerobag-dev.iac.jonh.net"],
  },
  build: {
    outDir: path.join(webTargetRoot, "dist"),
    emptyOutDir: true,
  },
});
