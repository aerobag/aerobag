import fs from "node:fs";
import path from "node:path";
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
const chartAssetRoot = path.join(staticRoot, "chart-assets");
const chartThumbnailRoot = path.join(staticRoot, "chart-thumbnails");
const navDbRoot = path.join(staticRoot, "nav-db");
const vectorRoot = path.join(staticRoot, "vectors");
const sharedRoot = path.join(repoRoot, "ui", "shared");
const sharedFixturesRoot = path.join(repoRoot, "ui", "shared-fixtures");
const debugLogPath = path.join("/tmp", "aerobag-web-debug.log");
const requestLogPath = path.join("/tmp", "aerobag-web-requests.log");
const artifactRootConfigFile = path.join(repoRoot, ".aerobag-artifact-root");
const configuredArtifactRoot = fs.readFileSync(artifactRootConfigFile, "utf8").trim();
const configuredArtifactPath = path.isAbsolute(configuredArtifactRoot)
  ? configuredArtifactRoot
  : path.resolve(repoRoot, configuredArtifactRoot);
const productionManifestDir = path.join("product-builds", "production");
function latestCurrentArtifacts(root: string): string | null {
  const manifestDir = path.join(root, productionManifestDir);
  if (!fs.existsSync(manifestDir) || !fs.statSync(manifestDir).isDirectory()) {
    return null;
  }
  const manifests = fs.readdirSync(manifestDir)
    .filter((name) => name.startsWith("current_artifacts_") && name.endsWith(".json"))
    .sort();
  return manifests.length > 0 ? path.join(manifestDir, manifests[manifests.length - 1]) : null;
}
const artifactRoot = configuredArtifactPath;
const currentArtifactsPath = latestCurrentArtifacts(artifactRoot) ?? path.join(artifactRoot, productionManifestDir, "current_artifacts_missing.json");
const currentArtifacts = JSON.parse(fs.readFileSync(currentArtifactsPath, "utf8")) as { bundles?: Array<{ filename?: string }> };
const productBuildPath = path.join(
  artifactRoot,
  productionManifestDir,
  currentArtifacts.bundles?.[currentArtifacts.bundles.length - 1]?.filename ?? "bundle_missing.json",
);

function resolveProductBuildOutput(nodeName: string, outputName: string): string {
  const payload = JSON.parse(fs.readFileSync(productBuildPath, "utf8")) as Record<string, unknown> & { nodes?: Array<Record<string, unknown>> };
  const topLevel = payload[nodeName];
  if (topLevel && typeof topLevel === "object") {
    const rawPath = (topLevel as Record<string, unknown>).relative_path;
    if (typeof rawPath === "string" && rawPath.length > 0) {
      return path.join(artifactRoot, rawPath.startsWith("product-builds/") ? rawPath : path.join("product-builds", rawPath));
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
    return path.join(artifactRoot, rawPath);
  }
  throw new Error(`missing product build output ${nodeName}.${outputName}`);
}

const resourceIndexPath = resolveProductBuildOutput("resource_index", "resource_index");

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
      next();
      return;
    }
    const extension = path.extname(filePath).toLowerCase();
    const contentType =
      extension === ".webp"
        ? "image/webp"
        : extension === ".png"
          ? "image/png"
          : extension === ".json"
            ? "application/json"
            : "application/octet-stream";
    res.setHeader("Content-Type", contentType);
    fs.createReadStream(filePath).pipe(res);
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
      server.middlewares.use("/chart-assets", mountStaticTree(chartAssetRoot));
      server.middlewares.use("/chart-thumbnails", mountStaticTree(chartThumbnailRoot));
      server.middlewares.use("/nav-db", mountStaticTree(navDbRoot));
      server.middlewares.use("/vectors", mountStaticTree(vectorRoot));
    },
    writeBundle(outputOptions) {
      const outputDir = outputOptions.dir;
      if (!outputDir) {
        return;
      }
      for (const [sourceRoot, targetName] of [
        [sectionalRoot, "sectional-packages"],
        [chartAssetRoot, "chart-assets"],
        [chartThumbnailRoot, "chart-thumbnails"],
        [navDbRoot, "nav-db"],
        [vectorRoot, "vectors"],
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
    alias: {
      "@generated": generatedRoot,
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
      allow: [workspaceRoot, webSourceRoot, sharedRoot, sharedFixturesRoot, generatedRoot, staticRoot, path.dirname(resourceIndexPath), artifactRoot],
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
