import fs from "node:fs";
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
const liveFeedsOrigin = process.env.AEROBAG_LIVE_FEEDS_ORIGIN ?? null;
const liveFeedsFixtureRoot = process.env.AEROBAG_LIVE_FEEDS_FIXTURE_ROOT
  ? path.resolve(process.env.AEROBAG_LIVE_FEEDS_FIXTURE_ROOT)
  : path.resolve(repoRoot, "..", "live-feeds-dev-fixture", "live-feeds");
const metarBakeoffState = process.env.AEROBAG_METAR_BAKEOFF_STATE ?? "e91c842b86246281";
const metarBakeoffDeltaFrom = process.env.AEROBAG_METAR_BAKEOFF_DELTA_FROM ?? "f7cfb829c95fa022";
const metarBakeoffDeltaTo = process.env.AEROBAG_METAR_BAKEOFF_DELTA_TO ?? "1bd77ad5a9393345";
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
    server.middlewares.use("/__metar_bakeoff_fixture", (req, res, next) => {
      const requestPath = decodeURIComponent((req.url ?? "/").split("?")[0] ?? "/");
      const fixtureFile =
        requestPath === "/metars-state.json"
          ? path.join(liveFeedsFixtureRoot, "states", "metars", `${metarBakeoffState}.json`)
          : requestPath === "/metars-delta-from-state.json"
            ? path.join(liveFeedsFixtureRoot, "states", "metars", `${metarBakeoffDeltaFrom}.json`)
            : requestPath === "/metars-delta.json"
              ? path.join(liveFeedsFixtureRoot, "deltas", "metars", `${metarBakeoffDeltaFrom}__${metarBakeoffDeltaTo}.json`)
              : null;
      if (!fixtureFile) {
        next();
        return;
      }
      const filePath = fixtureFile;
      if (!filePath.startsWith(liveFeedsFixtureRoot) || !fs.existsSync(filePath)) {
        res.statusCode = 404;
        appendRequestLog({
          kind: "metar_bakeoff_fixture.missing",
          file_path: filePath,
          state: metarBakeoffState,
          delta_from: metarBakeoffDeltaFrom,
          delta_to: metarBakeoffDeltaTo,
        });
        res.end("not found");
        return;
      }
      appendRequestLog({
        kind: "metar_bakeoff_fixture.hit",
        file_path: filePath,
        state: metarBakeoffState,
        delta_from: metarBakeoffDeltaFrom,
        delta_to: metarBakeoffDeltaTo,
      });
      res.setHeader("Content-Type", "application/json");
      fs.createReadStream(filePath).pipe(res);
    });
    server.middlewares.use("/packages", mountStaticTree(artifactReadRoot, { missingStatus: 404, logPrefix: "packages" }));
    server.middlewares.use("/icons", mountStaticTree(iconsRoot, { missingStatus: 404, logPrefix: "icons" }));
    server.middlewares.use("/adsb-traces", mountStaticTree(adsbTraceRoot, { missingStatus: 404, logPrefix: "adsb_traces" }));
    for (const legacyPrefix of [
      "/afd",
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
    proxy: liveFeedsOrigin
      ? {
          "/live-feeds": {
            target: liveFeedsOrigin,
            changeOrigin: true,
          },
        }
      : undefined,
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
    proxy: liveFeedsOrigin
      ? {
          "/live-feeds": {
            target: liveFeedsOrigin,
            changeOrigin: true,
          },
        }
      : undefined,
  },
  build: {
    outDir: path.join(webTargetRoot, "dist"),
    emptyOutDir: true,
  },
  worker: {
    format: "es",
  },
});
