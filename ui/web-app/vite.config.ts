import fs from "node:fs";
import path from "node:path";
import zlib from "node:zlib";
import { execFileSync } from "node:child_process";
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
const gpsCaptureRoot = process.env.AEROBAG_GPS_CAPTURE_ROOT
  ? path.resolve(process.env.AEROBAG_GPS_CAPTURE_ROOT)
  : path.join("/tmp", "aerobag-gps-captures");
const liveFeedsOrigin = process.env.AEROBAG_LIVE_FEEDS_ORIGIN ?? null;
const webDebugLogEnabled = /^(1|true|yes)$/i.test(process.env.AEROBAG_WEB_DEBUG_LOG_ENABLED ?? "");
const sharedRoot = path.join(repoRoot, "ui", "shared");
const sharedFixturesRoot = path.join(repoRoot, "ui", "shared-fixtures");
const productContractsPath = path.join(repoRoot, "crates", "product-contracts", "src", "lib.rs");
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
const currentArtifactsPath = latestCurrentArtifacts(artifactReadRoot);
if (currentArtifactsPath) {
  const currentArtifacts = JSON.parse(fs.readFileSync(currentArtifactsPath, "utf8")) as Array<{
    artifact_roots?: { packaged?: string; unpacked?: string };
  }>;
  if (!Array.isArray(currentArtifacts) || currentArtifacts.length === 0) {
    throw new Error(`${currentArtifactsPath} must be a non-empty current_artifacts list`);
  }
  for (const [index, manifest] of currentArtifacts.entries()) {
    for (const key of ["packaged", "unpacked"] as const) {
      const root = manifest.artifact_roots?.[key];
      if (
        typeof root !== "string"
        || root.trim() === ""
        || root.startsWith("/")
        || root.split("/").includes("..")
      ) {
        throw new Error(`${currentArtifactsPath}[${index}].artifact_roots.${key} is not a safe relative path: ${JSON.stringify(root)}`);
      }
    }
  }
} else {
  console.warn(`${path.join(artifactReadRoot, "current_artifacts.json")} does not exist yet; /packages will be empty until product publication completes.`);
}

function loadProductContracts(): Record<string, string> {
  const source = fs.readFileSync(productContractsPath, "utf8");
  const constants = new Map<string, string>();
  for (const match of source.matchAll(/pub const ([A-Z0-9_]+_CONTRACT_ID): &str = "([^"]+)";/g)) {
    constants.set(match[1], match[2]);
  }
  const contracts: Record<string, string> = {};
  for (const match of source.matchAll(
    /ProductContract\s*\{\s*family_id:\s*"([^"]+)",\s*contract_id:\s*([A-Z0-9_]+_CONTRACT_ID),\s*\}/g,
  )) {
    const contractId = constants.get(match[2]);
    if (!contractId) {
      throw new Error(`${productContractsPath} references unknown contract constant ${match[2]}`);
    }
    contracts[match[1]] = contractId;
  }
  if (!contracts["nav-db"]) {
    throw new Error(`${productContractsPath} did not declare a nav-db contract`);
  }
  return contracts;
}

const productContracts = loadProductContracts();

type ClientBuildInfo = {
  platform: string;
  version: string;
  built_at_utc: string;
  commit: string;
  dirty: boolean;
};

function gitOutput(args: string[]): string | null {
  try {
    const output = execFileSync("git", ["-C", repoRoot, ...args], {
      encoding: "utf8",
      stdio: ["ignore", "pipe", "ignore"],
    }).trim();
    return output || null;
  } catch {
    return null;
  }
}

function compactUtcMinute(date: Date): string {
  const pad = (value: number) => value.toString().padStart(2, "0");
  return [
    date.getUTCFullYear().toString(),
    pad(date.getUTCMonth() + 1),
    pad(date.getUTCDate()),
    pad(date.getUTCHours()),
    pad(date.getUTCMinutes()),
  ].join("");
}

function isoUtcSecond(date: Date): string {
  return date.toISOString().replace(/\.\d{3}Z$/, "Z");
}

function loadClientBuildInfo(platform: string): ClientBuildInfo {
  const builtAt = new Date();
  const commit = process.env.AEROBAG_GIT_COMMIT
    ?? gitOutput(["rev-parse", "HEAD"])
    ?? "unknown";
  const shortCommit = process.env.AEROBAG_SHORT_COMMIT
    ?? gitOutput(["rev-parse", "--short=8", "HEAD"])
    ?? (commit === "unknown" ? "unknown" : commit.slice(0, 8));
  const dirty = process.env.AEROBAG_BUILD_DIRTY === undefined
    ? (gitOutput(["status", "--porcelain"]) ?? "").length > 0
    : /^(1|true|yes|on)$/i.test(process.env.AEROBAG_BUILD_DIRTY);
  const buildId = `${shortCommit}${dirty ? ".dirty" : ""}`;
  const version = process.env.AEROBAG_VERSION_NAME
    ?? `0.1.${process.env.AEROBAG_BUILD_STAMP_UTC ?? compactUtcMinute(builtAt)}+${buildId}`;
  return {
    platform,
    version,
    built_at_utc: process.env.AEROBAG_BUILT_AT_UTC ?? isoUtcSecond(builtAt),
    commit,
    dirty,
  };
}

const clientBuildInfo = loadClientBuildInfo("Web");

function appendRequestLog(entry: Record<string, unknown>) {
  fs.appendFileSync(requestLogPath, `${JSON.stringify({ ts: Date.now(), ...entry })}\n`);
}

function mountStaticTree(sourceRoot: string, options: { missingStatus?: number; logPrefix?: string; direct?: boolean } = {}) {
  return (req: { headers?: Record<string, string | string[] | undefined>; method?: string; url?: string }, res: { statusCode: number; end: (body?: string | Buffer) => void; setHeader: (name: string, value: string) => void }, next: () => void) => {
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
    const fileStat = fs.existsSync(filePath) ? fs.statSync(filePath) : null;
    if (!fileStat?.isFile()) {
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
        : extension === ".jpg" || extension === ".jpeg"
          ? "image/jpeg"
        : extension === ".png"
          ? "image/png"
          : extension === ".terrain"
            ? "application/vnd.aerobag.terrain"
          : extension === ".db"
            ? "application/vnd.sqlite3"
          : extension === ".json" || extension === ".jsonl"
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
    if (/^[^/]+\/[^/]+\/(packaged|unpacked)\//.test(relativePath)) {
      res.setHeader("Cache-Control", "public, max-age=31536000, immutable");
    } else if (relativePath === "current_artifacts.json") {
      res.setHeader("Cache-Control", "no-cache");
    }
    const sendFile = (stream: fs.ReadStream) => {
      if (req.method === "HEAD") {
        stream.destroy();
        res.end();
        return;
      }
      stream.pipe(res);
    };
    if (options.direct) {
      res.setHeader("Content-Length", String(fileStat.size));
      if (req.method === "HEAD") {
        res.end();
        return;
      }
      res.end(fs.readFileSync(filePath));
      return;
    }
    const stream = fs.createReadStream(filePath);
    if (extension === ".terrain") {
      res.setHeader("Content-Encoding", "gzip");
      res.setHeader("Content-Length", String(fileStat.size));
      sendFile(stream);
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
    res.setHeader("Content-Length", String(fileStat.size));
    sendFile(stream);
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
    server.middlewares.use("/packages", mountStaticTree(artifactReadRoot, { missingStatus: 404, logPrefix: "packages" }));
    server.middlewares.use("/live-feeds", (req: { url?: string }, res: { statusCode: number; end: (body?: string) => void }) => {
      res.statusCode = 404;
      appendRequestLog({ kind: "live_feeds_wrong_origin", url: req.url ?? "", status: 404 });
      res.end("live feeds are served by the configured live-feed origin, not Vite");
    });
    server.middlewares.use("/icons", mountStaticTree(iconsRoot, { missingStatus: 404, logPrefix: "icons" }));
    server.middlewares.use("/adsb-traces", mountStaticTree(adsbTraceRoot, { missingStatus: 404, logPrefix: "adsb_traces", direct: true }));
    server.middlewares.use("/gps-captures", mountStaticTree(gpsCaptureRoot, { missingStatus: 404, logPrefix: "gps_captures", direct: true }));
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

function aerobagProductContractsPlugin(): Plugin {
  return {
    name: "aerobag-product-contracts",
    transformIndexHtml(html) {
      return html.replace(
        "__AEROBAG_PRODUCT_CONTRACTS__",
        JSON.stringify(productContracts),
      );
    },
  };
}

export default defineConfig({
  plugins: [aerobagProductContractsPlugin(), react(), aerobagStaticPlugin()],
  define: {
    __AEROBAG_DEBUG_LOG_ENABLED__: JSON.stringify(webDebugLogEnabled),
    __AEROBAG_LIVE_FEEDS_ORIGIN__: JSON.stringify(liveFeedsOrigin),
    __AEROBAG_CLIENT_BUILD_INFO__: JSON.stringify(clientBuildInfo),
  },
  resolve: {
    preserveSymlinks: true,
    alias: {
      "@generated": generatedRoot,
      "@shared": sharedRoot,
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
        ...(fs.existsSync(gpsCaptureRoot) ? [gpsCaptureRoot] : []),
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
  worker: {
    format: "es",
  },
});
