import { copyFile, mkdtemp, readFile, rm } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = process.env.AEROBAG_REPO_ROOT
  ? path.resolve(process.env.AEROBAG_REPO_ROOT)
  : path.resolve(scriptDir, "../../..");
const generatedDir = process.env.AEROBAG_WASM_GENERATED_DIR
  ? path.resolve(process.env.AEROBAG_WASM_GENERATED_DIR)
  : path.resolve(repoRoot, "../ui-target/web/generated");

const modulePath = path.join(generatedDir, "app_wasm.js");
const wasmPath = path.join(generatedDir, "app_wasm_bg.wasm");
const tempDir = await mkdtemp(path.join(os.tmpdir(), "aerobag-wasm-smoke-"));

try {
  const tempModulePath = path.join(tempDir, "app_wasm.mjs");
  const tempWasmPath = path.join(tempDir, "app_wasm_bg.wasm");
  await copyFile(modulePath, tempModulePath);
  await copyFile(wasmPath, tempWasmPath);

  const wasmModule = await import(pathToFileURL(tempModulePath).href);
  const wasmBytes = await readFile(tempWasmPath);
  await wasmModule.default(wasmBytes);

  if (typeof wasmModule.startup_smoke_test !== "function") {
    throw new Error("debug wasm module does not export startup_smoke_test");
  }

  wasmModule.startup_smoke_test();
  console.log("wasm startup smoke test passed");
} finally {
  await rm(tempDir, { recursive: true, force: true });
}
