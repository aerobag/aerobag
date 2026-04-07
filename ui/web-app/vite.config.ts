import fs from "node:fs";
import path from "node:path";
import { defineConfig, type Plugin } from "vite";
import react from "@vitejs/plugin-react";

const sectionalRoot = path.resolve(__dirname, "generated-static", "sectional-packages");
const chartAssetRoot = path.resolve(__dirname, "generated-static", "chart-assets");

function mountStaticTree(sourceRoot: string) {
  return (req: { url?: string }, res: { statusCode: number; end: (body?: string) => void; setHeader: (name: string, value: string) => void }, next: () => void) => {
    const requestPath = decodeURIComponent((req.url ?? "/").split("?")[0] ?? "/");
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
    },
    writeBundle(outputOptions) {
      const outputDir = outputOptions.dir;
      if (!outputDir) {
        return;
      }
      for (const [sourceRoot, targetName] of [
        [sectionalRoot, "sectional-packages"],
        [chartAssetRoot, "chart-assets"],
      ] as const) {
        if (!fs.existsSync(sourceRoot)) {
          continue;
        }
        const targetRoot = path.join(outputDir, targetName);
        fs.rmSync(targetRoot, { recursive: true, force: true });
        fs.mkdirSync(path.dirname(targetRoot), { recursive: true });
        fs.cpSync(sourceRoot, targetRoot, { recursive: true });
      }
    },
  };
}

export default defineConfig({
  plugins: [react(), aerobagStaticPlugin()],
  server: {
    port: 4173,
    host: "0.0.0.0",
    allowedHosts: ["aerobag-dev.iac.jonh.net"],
  },
});
