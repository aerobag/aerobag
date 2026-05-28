import init, * as wasmModule from "@generated/app_wasm.js";
import { debugLog, installRustDebugLogBridge } from "./domain/debugLog";

type BakeoffCandidate = {
  name: string;
  serializer: string;
  indexing_strategy: string;
  encoded_bytes: number;
  encode_ms: number;
  avg_decode_install_ms: number;
  min_decode_install_ms: number;
  max_decode_install_ms: number;
  checksum: number;
  tile_count: number;
  tile_ref_count: number;
};

type BakeoffReport = {
  rounds: number;
  fixture_json_bytes: number;
  metar_count: number;
  pirep_count: number;
  candidates: BakeoffCandidate[];
  js_json_parse?: {
    avg_ms: number;
    min_ms: number;
    max_ms: number;
    checksum: number;
  };
};

type BakeoffWasm = typeof wasmModule & {
  metar_bakeoff_run: (
    stateJson: string,
    fromStateJson: string,
    deltaJson: string,
    rounds: number,
  ) => string;
};

declare global {
  interface Window {
    __aerobagMetarBakeoffReport?: BakeoffReport;
  }
}

export async function runMetarBakeoff(rootNode: HTMLElement): Promise<void> {
  const params = new URLSearchParams(window.location.search);
  const rounds = clampInt(Number(params.get("rounds") ?? 12), 1, 200);
  rootNode.innerHTML = `<main style="font-family: system-ui, sans-serif; padding: 24px; line-height: 1.35">
    <h1>METAR Serialization Bakeoff</h1>
    <p id="metar-bakeoff-status">Loading fixture...</p>
    <pre id="metar-bakeoff-output" style="white-space: pre-wrap; font-size: 12px"></pre>
  </main>`;
  window.__aerobag_hide_startup_shell?.();
  const status = document.getElementById("metar-bakeoff-status");
  const output = document.getElementById("metar-bakeoff-output");
  const startedAt = performance.now();
  try {
    const [fixtureText, fromStateText, deltaText] = await Promise.all([
      fetchFixtureText("/__metar_bakeoff_fixture/metars-state.json"),
      fetchFixtureText("/__metar_bakeoff_fixture/metars-delta-from-state.json"),
      fetchFixtureText("/__metar_bakeoff_fixture/metars-delta.json"),
    ]);
    status!.textContent = `Loaded ${formatBytes(fixtureText.length)} fixture. Initializing WASM...`;
    installRustDebugLogBridge();
    await init();
    wasmModule.install_rust_debug_logger?.();

    status!.textContent = `Running ${rounds} rounds...`;
    const jsJsonParse = benchmarkJsJsonParse(fixtureText, rounds);
    const wasm = wasmModule as BakeoffWasm;
    const wasmStartedAt = performance.now();
    const report = JSON.parse(wasm.metar_bakeoff_run(
      fixtureText,
      fromStateText,
      deltaText,
      rounds,
    )) as BakeoffReport;
    const wasmElapsedMs = performance.now() - wasmStartedAt;
    report.js_json_parse = jsJsonParse;
    window.__aerobagMetarBakeoffReport = report;

    const elapsedMs = performance.now() - startedAt;
    const payload = {
      elapsed_ms: Math.round(elapsedMs),
      wasm_elapsed_ms: Math.round(wasmElapsedMs),
      report,
    };
    debugLog("metar.bakeoff.report", payload);
    await uploadDebugLog("metar.bakeoff.report", payload);
    status!.textContent = `Done in ${Math.round(elapsedMs)}ms. Report uploaded to /tmp/aerobag-web-debug.log.`;
    output!.textContent = renderReport(report);
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    const payload = { message };
    debugLog("metar.bakeoff.error", payload);
    await uploadDebugLog("metar.bakeoff.error", payload);
    status!.textContent = `Failed: ${message}`;
    throw error;
  }
}

async function fetchFixtureText(url: string): Promise<string> {
  const response = await fetch(url, {
    cache: "no-cache",
  });
  if (!response.ok) {
    throw new Error(`fixture fetch failed for ${url}: ${response.status}`);
  }
  return response.text();
}

function benchmarkJsJsonParse(fixtureText: string, rounds: number) {
  const elapsed: number[] = [];
  let checksum = 0;
  for (let round = 0; round < rounds; round += 1) {
    const startedAt = performance.now();
    const parsed = JSON.parse(fixtureText) as {
      version_label?: string;
      metars_by_station?: Record<string, unknown>;
    };
    elapsed.push(performance.now() - startedAt);
    checksum ^= Object.keys(parsed.metars_by_station ?? {}).length;
    checksum ^= parsed.version_label?.length ?? 0;
  }
  return {
    avg_ms: round2(elapsed.reduce((sum, value) => sum + value, 0) / rounds),
    min_ms: round2(Math.min(...elapsed)),
    max_ms: round2(Math.max(...elapsed)),
    checksum,
  };
}

function renderReport(report: BakeoffReport): string {
  const rows = [
    `Fixture JSON: ${formatBytes(report.fixture_json_bytes)}`,
    `Records: ${report.metar_count} METARs, ${report.pirep_count} PIREPs`,
    `Rounds: ${report.rounds}`,
    "",
    `JS JSON.parse: avg ${report.js_json_parse?.avg_ms}ms, min ${report.js_json_parse?.min_ms}ms, max ${report.js_json_parse?.max_ms}ms`,
    "",
    [
      "serializer".padEnd(17),
      "indexing".padEnd(14),
      "bytes".padStart(11),
      "encode".padStart(9),
      "avg install".padStart(12),
      "min".padStart(9),
      "max".padStart(9),
      "tiles".padStart(8),
      "refs".padStart(8),
    ].join("  "),
  ];
  for (const candidate of report.candidates) {
    rows.push([
      candidate.serializer.padEnd(17),
      candidate.indexing_strategy.padEnd(14),
      formatBytes(candidate.encoded_bytes).padStart(11),
      `${candidate.encode_ms}ms`.padStart(9),
      `${candidate.avg_decode_install_ms}ms`.padStart(12),
      `${candidate.min_decode_install_ms}ms`.padStart(9),
      `${candidate.max_decode_install_ms}ms`.padStart(9),
      String(candidate.tile_count).padStart(8),
      String(candidate.tile_ref_count).padStart(8),
    ].join("  "));
  }
  return rows.join("\n");
}

async function uploadDebugLog(tag: string, data: unknown): Promise<void> {
  await fetch("/__debug_log", {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
    },
    body: JSON.stringify([{
      seq: 0,
      ts_ms: Math.round(performance.now()),
      tag,
      data,
    }]),
  });
}

function clampInt(value: number, min: number, max: number): number {
  if (!Number.isFinite(value)) {
    return min;
  }
  return Math.min(max, Math.max(min, Math.round(value)));
}

function round2(value: number): number {
  return Math.round(value * 100) / 100;
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) {
    return `${bytes} B`;
  }
  if (bytes < 1024 * 1024) {
    return `${round2(bytes / 1024)} KiB`;
  }
  return `${round2(bytes / 1024 / 1024)} MiB`;
}
