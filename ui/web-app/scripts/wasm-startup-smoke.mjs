// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { copyFile, mkdtemp, readFile, rm } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import assert from "node:assert/strict";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = process.env.AEROBAG_REPO_ROOT
  ? path.resolve(process.env.AEROBAG_REPO_ROOT)
  : path.resolve(scriptDir, "../../..");
const generatedDir = process.env.AEROBAG_WASM_GENERATED_DIR
  ? path.resolve(process.env.AEROBAG_WASM_GENERATED_DIR)
  : process.env.AEROBAG_UI_TARGET_ROOT
    ? path.resolve(process.env.AEROBAG_UI_TARGET_ROOT, "web/generated")
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

  if (typeof wasmModule.startup_smoke_test === "function") {
    wasmModule.startup_smoke_test();
    console.log("wasm startup smoke test passed");
  } else {
    console.log("wasm startup smoke init passed");
  }

  const files = new Map();
  const writes = [];
  globalThis.__aerobagReadDocument = key => ({ bytes: files.get(key) ?? null, error: null });
  globalThis.__aerobagWriteDocument = (key, bytes, complete) => writes.push({ key, bytes, complete });
  const create = () => {
    const {handle} = JSON.parse(wasmModule.create_ui_session("[]", "null", "null", 1000));
    wasmModule.configure_platform_capabilities_in_session(handle, JSON.stringify({display_policy: {}}));
    return handle;
  };
  const snapshot = handle => {
    const result = JSON.parse(wasmModule.get_session_snapshot_paged(handle));
    assert.equal(result.state, "complete");
    return result.result;
  };
  const completeWrites = error => {
    while (writes.length) {
      const write = writes.shift();
      if (!error) files.set(write.key, write.bytes);
      write.complete(error);
    }
  };
  const session = create();
  wasmModule.perform_settings_action_in_session(session, JSON.stringify({action_id: "display_dim_timeout", value_id: "30s"}), 1000n);
  assert(writes.length > 0, "Rust must submit a host write");
  completeWrites(null);
  wasmModule.destroy_session(session);
  const restored = create();
  assert.equal(snapshot(restored).display_policy.dim_after_ms, 30000);
  const old = files.get("aerobag.core.settings.v1").slice();
  wasmModule.perform_settings_action_in_session(restored, JSON.stringify({action_id: "display_dim_timeout", value_id: "10s"}), 1100n);
  completeWrites("injected browser quota failure");
  assert.deepEqual(files.get("aerobag.core.settings.v1"), old);
  const failed = snapshot(restored);
  assert.equal(failed.display_policy.dim_after_ms, 10000, "local edit survives failed storage");
  assert(JSON.stringify(failed.data_status_state).includes("injected browser quota failure"), "host failure must reach core warning UI");
  wasmModule.destroy_session(restored);
  const recovered = create();
  assert.equal(snapshot(recovered).display_policy.dim_after_ms, 30000, "failed replacement preserves previous saved state");
  wasmModule.destroy_session(recovered);
  console.log("wasm local-document restart and failure boundary passed");
} finally {
  await rm(tempDir, { recursive: true, force: true });
}
