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
const staticRoot = path.join(webTargetRoot, "generated-static");
const sectionalRoot = path.join(staticRoot, "sectional-packages");
const plateRoot = path.join(staticRoot, "plates");
const csupRoot = path.join(staticRoot, "afd");
const thumbnailRoot = path.join(staticRoot, "thumbnails");
const navDbRoot = path.join(staticRoot, "nav-db");
const vectorRoot = path.join(staticRoot, "vectors");
const fastProductRoot = path.join(staticRoot, "fast-products");
const adsbTraceRoot = path.resolve(repoRoot, "..", "adsb-traces");
const sharedRoot = path.join(repoRoot, "ui", "shared");
const sharedFixturesRoot = path.join(repoRoot, "ui", "shared-fixtures");
const debugLogPath = path.join("/tmp", "aerobag-web-debug.log");
const requestLogPath = path.join("/tmp", "aerobag-web-requests.log");
const artifactReadPathConfigFile = path.join(repoRoot, ".aerobag-artifact-read-path");
const configuredArtifactRoot = fs.readFileSync(artifactReadPathConfigFile, "utf8").trim();
const configuredArtifactPath = path.isAbsolute(configuredArtifactRoot)
  ? configuredArtifactRoot
  : path.resolve(repoRoot, configuredArtifactRoot);
const packagedDir = "published-packaged";
const unpackedDir = "published-unpacked";
function latestCurrentArtifacts(root: string): string | null {
  const manifestDir = path.join(root, packagedDir);
  if (!fs.existsSync(manifestDir) || !fs.statSync(manifestDir).isDirectory()) {
    return null;
  }
  const manifests = fs.readdirSync(manifestDir)
    .filter((name) => name.startsWith("current_artifacts_") && name.endsWith(".json"))
    .sort();
  return manifests.length > 0 ? path.join(manifestDir, manifests[manifests.length - 1]) : null;
}
const artifactReadRoot = configuredArtifactPath;
const currentArtifactsPath = latestCurrentArtifacts(artifactReadRoot) ?? path.join(artifactReadRoot, packagedDir, "current_artifacts_missing.json");
const currentArtifacts = JSON.parse(fs.readFileSync(currentArtifactsPath, "utf8")) as { bundles?: Array<{ filename?: string }> };
const activeBundleFilename = currentArtifacts.bundles?.[currentArtifacts.bundles.length - 1]?.filename ?? "bundle_missing.json";
const productBuildPath = path.join(
  artifactReadRoot,
  packagedDir,
  activeBundleFilename,
);
function resolvePublishedFilename(rawPath: string): string {
  if (path.isAbsolute(rawPath)) {
    throw new Error(`expected published filename, got absolute path ${rawPath}`);
  }
  if (rawPath.includes("/") || rawPath.includes("\\")) {
    throw new Error(`expected flat published filename, got ${rawPath}`);
  }
  return path.join(artifactReadRoot, packagedDir, rawPath);
}

function resolveProductBuildOutput(nodeName: string, outputName: string): string {
  const payload = JSON.parse(fs.readFileSync(productBuildPath, "utf8")) as Record<string, unknown> & { nodes?: Array<Record<string, unknown>> };
  const topLevel = payload[nodeName];
  if (topLevel && typeof topLevel === "object") {
    const rawPath = (topLevel as Record<string, unknown>).relative_path;
    if (typeof rawPath === "string" && rawPath.length > 0) {
      return resolvePublishedFilename(rawPath);
    }
  }
  for (const node of payload.nodes ?? []) {
    if (node.name !== nodeName) {
      continue;
    }
    const outputs = node.outputs;
    if (!outputs || typeof outputs !== "object") {
      break;
    }
    const rawPath = (outputs as Record<string, unknown>)[outputName];
    if (typeof rawPath !== "string" || rawPath.length === 0) {
      break;
    }
    return resolvePublishedFilename(rawPath);
  }
  throw new Error(`missing product build output ${nodeName}.${outputName}`);
}
const resourceIndexPath = resolveProductBuildOutput("resource_index", "resource_index");
const catalogPath = resolveProductBuildOutput("catalog", "catalog");

for (const [label, resolvedPath] of [
  ["catalog", catalogPath],
  ["resource index", resourceIndexPath],
] as const) {
  if (!fs.existsSync(resolvedPath) || !fs.statSync(resolvedPath).isFile()) {
    throw new Error(`missing ${label} artifact at ${resolvedPath}`);
  }
}

function mountStaticTree(sourceRoot: string) {
  return (req: { url?: string }, res: { statusCode: number; end: (body?: string) => void; setHeader: (name: string, value: string) => void }, next: () => void) => {
    const requestPath = decodeURIComponent((req.url ?? "/").split("?")[0] ?? "/");
    const relativePath = requestPath.replace(/^\/+/, "");
    const filePath = path.resolve(sourceRoot, relativePath);
    if (sourceRoot === vectorRoot) {
      fs.appendFileSync(
        requestLogPath,
        `${JSON.stringify({ ts: Date.now(), kind: "vector_request", requestPath, filePath, exists: fs.existsSync(filePath) })}\n`,
      );
    }
    if (!filePath.startsWith(sourceRoot)) {
      res.statusCode = 403;
      res.end("forbidden");
      return;
    }
    if (!fs.existsSync(filePath) || !fs.statSync(filePath).isFile()) {
      if (sourceRoot === vectorRoot) {
        res.statusCode = 404;
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
          : extension === ".db"
            ? "application/vnd.sqlite3"
          : extension === ".json"
            ? "application/json"
            : "application/octet-stream";
    const acceptEncoding = req.headers?.["accept-encoding"] ?? "";
    const shouldCompress = extension === ".db" || extension === ".json";
    const supportsBrotli = typeof acceptEncoding === "string" && /\bbr\b/.test(acceptEncoding);
    const supportsGzip = typeof acceptEncoding === "string" && /\bgzip\b/.test(acceptEncoding);
    res.setHeader("Content-Type", contentType);
    res.setHeader("Vary", "Accept-Encoding");
    const stream = fs.createReadStream(filePath);
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

function unpackedDirFromRelativeZip(filename: string): string {
  if (!filename.endsWith(".zip")) {
    throw new Error(`expected zip filename, got ${filename}`);
  }
  return path.join(artifactReadRoot, unpackedDir, filename.slice(0, -".zip".length));
}

function resolveCurrentFastProductRoot(productId: string): string | null {
  const manifestPath = latestCurrentArtifacts(artifactReadRoot);
  if (!manifestPath) {
    return null;
  }
  const manifest = JSON.parse(fs.readFileSync(manifestPath, "utf8")) as {
    fast_products?: Array<{ id?: string; filename?: string }>;
  };
  const product = manifest.fast_products?.find((candidate) => candidate.id === productId);
  if (!product?.filename) {
    return null;
  }
  const productRoot = unpackedDirFromRelativeZip(product.filename);
  if (!fs.existsSync(productRoot) || !fs.statSync(productRoot).isDirectory()) {
    return null;
  }
  return productRoot;
}

function mountFastProducts() {
  return (req: { url?: string }, res: { statusCode: number; end: (body?: string) => void; setHeader: (name: string, value: string) => void }, next: () => void) => {
    const requestPath = decodeURIComponent((req.url ?? "/").split("?")[0] ?? "/");
    const parts = requestPath.replace(/^\/+/, "").split("/");
    const productId = parts.shift();
    if (!productId || parts.length === 0) {
      next();
      return;
    }
    const productRoot = resolveCurrentFastProductRoot(productId);
    if (!productRoot) {
      res.statusCode = 404;
      res.end("fast product unavailable");
      return;
    }
    const filePath = path.resolve(productRoot, parts.join("/"));
    if (!filePath.startsWith(productRoot)) {
      res.statusCode = 403;
      res.end("forbidden");
      return;
    }
    return mountStaticTree(productRoot)({ url: `/${parts.join("/")}` }, res, next);
  };
}

function ensureLinkedFile(sourcePath: string, targetPath: string) {
  fs.mkdirSync(path.dirname(targetPath), { recursive: true });
  fs.rmSync(targetPath, { force: true, recursive: true });
  try {
    fs.linkSync(sourcePath, targetPath);
  } catch (error) {
    const code = (error as NodeJS.ErrnoException).code;
    if (code === "EXDEV") {
      fs.copyFileSync(sourcePath, targetPath);
      return;
    }
    throw error;
  }
}

function ensureLinkedTree(sourceRoot: string, targetRoot: string) {
  fs.rmSync(targetRoot, { recursive: true, force: true });
  fs.mkdirSync(targetRoot, { recursive: true });
  for (const entry of fs.readdirSync(sourceRoot, { withFileTypes: true })) {
    const sourcePath = path.join(sourceRoot, entry.name);
    const targetPath = path.join(targetRoot, entry.name);
    const stats = fs.lstatSync(sourcePath);
    if (stats.isSymbolicLink()) {
      const linkTarget = fs.readlinkSync(sourcePath);
      fs.symlinkSync(linkTarget, targetPath);
      continue;
    }
    if (stats.isDirectory()) {
      ensureLinkedTree(sourcePath, targetPath);
      continue;
    }
    if (stats.isFile()) {
      ensureLinkedFile(sourcePath, targetPath);
      continue;
    }
  }
}

function aerobagStaticPlugin(): Plugin {
  return {
    name: "aerobag-static-assets",
    configureServer(server) {
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
              fs.appendFileSync(requestLogPath, `${JSON.stringify({ ts: Date.now(), kind: "client_debug_post", count: payload.length })}\n`);
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
        fs.appendFileSync(
          requestLogPath,
          `${JSON.stringify({ ts: Date.now(), kind: "ping", url: req.url ?? "" })}\n`,
        );
        res.statusCode = 204;
        res.end();
      });
      server.middlewares.use("/sectional-packages", mountStaticTree(sectionalRoot));
      server.middlewares.use("/plates", mountStaticTree(plateRoot));
      server.middlewares.use("/afd", mountStaticTree(csupRoot));
      server.middlewares.use("/thumbnails", mountStaticTree(thumbnailRoot));
      server.middlewares.use("/nav-db", mountStaticTree(navDbRoot));
      server.middlewares.use("/vectors", mountStaticTree(vectorRoot));
      server.middlewares.use("/fast-products", mountFastProducts());
      server.middlewares.use("/adsb-traces", mountStaticTree(adsbTraceRoot));
    },
    writeBundle(outputOptions) {
      const outputDir = outputOptions.dir;
      if (!outputDir) {
        return;
      }
      for (const [sourceRoot, targetName] of [
        [sectionalRoot, "sectional-packages"],
        [plateRoot, "plates"],
        [csupRoot, "afd"],
        [thumbnailRoot, "thumbnails"],
        [navDbRoot, "nav-db"],
        [vectorRoot, "vectors"],
        [fastProductRoot, "fast-products"],
        [adsbTraceRoot, "adsb-traces"],
      ] as const) {
        if (!fs.existsSync(sourceRoot)) {
          continue;
        }
        const targetRoot = path.join(outputDir, targetName);
        ensureLinkedTree(sourceRoot, targetRoot);
      }
    },
  };
}

export default defineConfig({
  plugins: [react(), aerobagStaticPlugin()],
  resolve: {
    preserveSymlinks: true,
    alias: {
      "@generated": generatedRoot,
      "@product-catalog": catalogPath,
      "@product-resource-index": resourceIndexPath,
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
        staticRoot,
        path.dirname(resourceIndexPath),
        path.dirname(catalogPath),
        artifactReadRoot,
        path.join(artifactReadRoot, unpackedDir),
        fastProductRoot,
        adsbTraceRoot,
      ],
    },
  },
  build: {
    outDir: path.join(webTargetRoot, "dist"),
    emptyOutDir: true,
  },
  optimizeDeps: {
    exclude: ["@sqlite.org/sqlite-wasm"],
  },
});
