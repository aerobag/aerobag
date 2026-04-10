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
const webTargetRoot = path.join(uiTargetRoot, "web");
const generatedRoot = path.join(webTargetRoot, "generated");
const sectionalRoot = path.join(webTargetRoot, "generated-static", "sectional-packages");
const chartAssetRoot = path.join(webTargetRoot, "generated-static", "chart-assets");
const chartAssetManifestPath = path.join(webTargetRoot, "generated-static", "chart-assets-manifest.json");

function lookupChartAsset(requestPath: string) {
  if (!fs.existsSync(chartAssetManifestPath)) {
    return null;
  }
  const manifest = JSON.parse(fs.readFileSync(chartAssetManifestPath, "utf8")) as Record<string, string>;
  const filePath = manifest[requestPath] ?? manifest[`/chart-assets${requestPath}`];
  if (!filePath || !fs.existsSync(filePath) || !fs.statSync(filePath).isFile()) {
    return null;
  }
  return filePath;
}

function mountStaticTree(sourceRoot: string) {
  return (req: { url?: string }, res: { statusCode: number; end: (body?: string) => void; setHeader: (name: string, value: string) => void }, next: () => void) => {
    const requestPath = decodeURIComponent((req.url ?? "/").split("?")[0] ?? "/");
    const manifestFilePath = sourceRoot === chartAssetRoot ? lookupChartAsset(requestPath) : null;
    if (manifestFilePath) {
      res.setHeader("Content-Type", "image/png");
      fs.createReadStream(manifestFilePath).pipe(res);
      return;
    }
    const relativePath = requestPath.replace(/^\/+/, "");
    const filePath = path.resolve(sourceRoot, relativePath);
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

function aerobagStaticPlugin(): Plugin {
  return {
    name: "aerobag-static-assets",
    configureServer(server) {
      server.middlewares.use("/sectional-packages", mountStaticTree(sectionalRoot));
      server.middlewares.use("/chart-assets", mountStaticTree(chartAssetRoot));
      server.middlewares.use("/chart-thumbnails", mountStaticTree(chartAssetRoot));
    },
    writeBundle(outputOptions) {
      const outputDir = outputOptions.dir;
      if (!outputDir) {
        return;
      }
      for (const [sourceRoot, targetName] of [
        [sectionalRoot, "sectional-packages"],
        [chartAssetRoot, "chart-assets"],
        [chartAssetRoot, "chart-thumbnails"],
      ] as const) {
        if (!fs.existsSync(sourceRoot)) {
          continue;
        }
        const targetRoot = path.join(outputDir, targetName);
        fs.rmSync(targetRoot, { recursive: true, force: true });
        fs.mkdirSync(targetRoot, { recursive: true });
        fs.cpSync(sourceRoot, targetRoot, { recursive: true });
      }
    },
  };
}

export default defineConfig({
  plugins: [react(), aerobagStaticPlugin()],
  resolve: {
    alias: {
      "@generated": generatedRoot,
    },
  },
  server: {
    port: 4173,
    host: "0.0.0.0",
    allowedHosts: ["aerobag-dev.iac.jonh.net"],
  },
  build: {
    outDir: path.join(webTargetRoot, "dist"),
    emptyOutDir: true,
  },
});
