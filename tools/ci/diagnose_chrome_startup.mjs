#!/usr/bin/env node
// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

// Infrastructure diagnostic only: never writes a qualification receipt.
import { execFileSync, spawnSync } from "node:child_process";
import { createReadStream } from "node:fs";
import { mkdir, mkdtemp, readFile, readdir, rm, writeFile } from "node:fs/promises";
import { freemem, loadavg, release, totalmem } from "node:os";
import { dirname, join, resolve } from "node:path";
import { parseArgs } from "node:util";
import { chromeProcessDiagnostics, connectToBrowser, launchChrome, stopProcess } from "../../ui/web-app/scripts/chrome-cdp.mjs";

const { values } = parseArgs({ options: {
  output: { type: "string" }, iterations: { type: "string", default: "100" },
  transport: { type: "string", default: "pipe" },
  environment: { type: "string", default: "inherited" },
  "preload-browser-files": { type: "boolean", default: false },
} });
const iterations = Number(values.iterations);
if (!values.output || !Number.isInteger(iterations) || iterations < 1 || iterations > 500 ||
    !["pipe", "websocket"].includes(values.transport) ||
    !["inherited", "no-session-bus"].includes(values.environment)) {
  throw new Error("require --output DIR, --iterations 1..500, --transport pipe|websocket, --environment inherited|no-session-bus");
}
const output = resolve(values.output);
await mkdir(output, { recursive: true });
const chromeBin = process.env.CHROME_BIN || "google-chrome-stable";
const environment = { ...process.env };
if (values.environment === "no-session-bus") delete environment.DBUS_SESSION_BUS_ADDRESS;
const read = (path) => readFile(path, "utf8").catch((error) => `${error.code}: unavailable`);
const save = (path, value) => writeFile(path, `${JSON.stringify(value, null, 2)}\n`);
const identity = {
  kind: "chrome-startup-diagnostic", schema_version: 1,
  commit: execFileSync("git", ["rev-parse", "HEAD"], { encoding: "utf8" }).trim(),
  browser: execFileSync(chromeBin, ["--version"], { encoding: "utf8", timeout: 15000 }).trim(),
  browser_binary: chromeBin, node: process.version, kernel: release(),
  runner_image: process.env.ImageOS ?? null, runner_image_version: process.env.ImageVersion ?? null,
  transport: values.transport, environment: values.environment,
  // Record only whether this known input is present, never arbitrary env values.
  session_bus_present: Boolean(environment.DBUS_SESSION_BUS_ADDRESS),
  cpu_max: await read("/sys/fs/cgroup/cpu.max"), memory_max: await read("/sys/fs/cgroup/memory.max"),
};
if (values["preload-browser-files"]) {
  // Diagnostic control only: read the same installed browser's immutable
  // executable/resources, without starting it or warming a user profile.
  const binary = execFileSync("which", [chromeBin], { encoding: "utf8" }).trim();
  const installed = dirname(execFileSync("readlink", ["-f", binary], { encoding: "utf8" }).trim());
  const started = performance.now();
  const files = [];
  for (const name of (await readdir(installed)).sort()) {
    if (!/^(chrome|chrome_crashpad_handler|icudtl\.dat|.*\.pak|.*\.so|.*snapshot.*\.bin)$/.test(name)) continue;
    let bytes = 0;
    for await (const chunk of createReadStream(join(installed, name), { signal: AbortSignal.timeout(60000) })) {
      bytes += chunk.length;
    }
    files.push({ name, bytes });
  }
  identity.preload = { elapsed_ms: performance.now() - started, files };
}
await save(join(output, "identity.json"), identity);

async function snapshot(pid) {
  const base = pid ? `/proc/${pid}` : null;
  return {
    at: new Date().toISOString(), load: loadavg(), free_memory: freemem(), total_memory: totalmem(),
    cpu_pressure: await read("/proc/pressure/cpu"), memory_pressure: await read("/proc/pressure/memory"),
    io_pressure: await read("/proc/pressure/io"),
    process: base ? {
      stat: await read(`${base}/stat`), status: await read(`${base}/status`),
      io: await read(`${base}/io`), schedstat: await read(`${base}/schedstat`),
      threads: await Promise.all((await readdir(`${base}/task`).catch(() => [])).map(async (tid) => ({
        tid, stat: await read(`${base}/task/${tid}/stat`),
        wchan: await read(`${base}/task/${tid}/wchan`), children: await read(`${base}/task/${tid}/children`),
      }))),
    } : null,
  };
}

const results = [];
const suiteStarted = performance.now();
for (let index = 1; index <= iterations && performance.now() - suiteStarted < 360000; index += 1) {
  const directory = join(output, `attempt-${index}`);
  await mkdir(directory);
  const profile = await mkdtemp(join(directory, "profile-"));
  const started = performance.now();
  let chrome, browser, child, phase = "launch", result;
  const samples = [];
  let sampling = false;
  const sampler = setInterval(async () => {
    if (sampling) return;
    sampling = true;
    try { samples.push(await snapshot(child?.pid)); } finally { sampling = false; }
  }, 1000);
  try {
    chrome = await launchChrome({
      chromeBin, userDataDir: profile, transport: values.transport, env: environment,
      width: 1000, height: 900, netLogPath: join(directory, "netlog.json"),
      onSpawn: (process) => { child = process; },
    });
    const launchedMs = performance.now() - started;
    phase = "connect";
    browser = await connectToBrowser(chrome.endpoint);
    result = { index, status: "passed", launch_ms: launchedMs, ready_ms: performance.now() - started };
  } catch (error) {
    result = {
      index, status: "failed", phase, elapsed_ms: performance.now() - started,
      error: { name: error.name, message: error.message, stack: error.stack },
      chrome: chromeProcessDiagnostics(chrome ?? { process: child }), samples,
      at_failure: await snapshot(child?.pid),
      pipe_bytes_sent: chrome?.pipeWrite?.bytesWritten ?? null,
      pipe_bytes_received: chrome?.pipeRead?.bytesRead ?? null,
    };
    // Only our fresh, blank browser is traced; syscall counts contain no payloads.
    if (child?.pid && child.exitCode === null && child.signalCode === null) {
      const command = ["timeout", "5s", "strace", "-f", "-c", "-p", String(child.pid)];
      if (process.env.GITHUB_ACTIONS === "true") command.unshift("sudo", "-n");
      const trace = spawnSync(command[0], command.slice(1), {
        encoding: "utf8", timeout: 7000, maxBuffer: 1024 * 1024,
      });
      result.syscall_summary = trace.stderr ?? trace.error?.message ?? "unavailable";
    }
  } finally {
    clearInterval(sampler);
    await browser?.close();
    await stopProcess(child);
    if (result?.status === "failed") {
      result.after_teardown = chromeProcessDiagnostics(chrome ?? { process: child });
    } else {
      await rm(join(directory, "netlog.json"), { force: true });
    }
    // Only this attempt's newly created profile, never a developer browser profile.
    await rm(profile, { recursive: true, force: true, maxRetries: 5, retryDelay: 100 });
  }
  await save(join(directory, "result.json"), result);
  results.push(result);
  console.log(`${index}/${iterations} ${result.status} ${Math.round(result.ready_ms ?? result.elapsed_ms)}ms`);
  await save(join(output, "summary.json"), {
    ...identity, requested: iterations, attempted: results.length,
    failures: results.filter((item) => item.status === "failed").length,
    elapsed_ms: performance.now() - suiteStarted,
    results: results.map(({ index, status, ready_ms, elapsed_ms, phase }) => ({ index, status, ready_ms, elapsed_ms, phase })),
  });
}
if (results.length !== iterations || results.some((result) => result.status !== "passed")) process.exitCode = 1;
