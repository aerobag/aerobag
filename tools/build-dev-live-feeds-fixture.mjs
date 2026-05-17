#!/usr/bin/env node

import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import zlib from "node:zlib";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const defaultMetarFixtureRoot = path.join(
  repoRoot,
  "..",
  "aerobag-test-artifacts",
  "metars",
  "delta-three-hour",
);

const args = parseArgs(process.argv.slice(2));
const outputRoot = path.resolve(args.output ?? path.join(repoRoot, "..", "live-feeds-dev-fixture"));
const liveFeedsRoot = path.join(outputRoot, "live-feeds");
const metarFixtureRoot = path.resolve(args.metars ?? defaultMetarFixtureRoot);
const mergeRoot = args.mergeRoot ? path.resolve(args.mergeRoot) : null;

if (fs.existsSync(outputRoot)) {
  fs.rmSync(outputRoot, { recursive: true, force: true });
}
fs.mkdirSync(liveFeedsRoot, { recursive: true });

if (mergeRoot) {
  copyTree(mergeRoot, liveFeedsRoot);
}

const metarStates = loadMetarStates(metarFixtureRoot);
if (metarStates.length === 0) {
  throw new Error(`no METAR fixture zips found under ${metarFixtureRoot}`);
}

publishMetarStates(liveFeedsRoot, metarStates);
resetCurrentToFirstFixtureVersions(liveFeedsRoot);
console.log(`wrote ${metarStates.length} METAR live-feed states to ${liveFeedsRoot}`);

function parseArgs(argv) {
  const parsed = {};
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === "--output") {
      parsed.output = argv[++index];
    } else if (arg === "--metars") {
      parsed.metars = argv[++index];
    } else if (arg === "--merge-root") {
      parsed.mergeRoot = argv[++index];
    } else {
      throw new Error(`unknown argument: ${arg}`);
    }
  }
  return parsed;
}

function loadMetarStates(fixtureRoot) {
  return fs.readdirSync(fixtureRoot)
    .filter((name) => name.endsWith(".zip"))
    .sort()
    .map((name) => {
      const zipPath = path.join(fixtureRoot, name);
      const state = JSON.parse(readZipMember(zipPath, "metars.json").toString("utf8"));
      return { name, state, version: state.version_label };
    });
}

function publishMetarStates(root, states) {
  const stateDir = path.join(root, "states", "metars");
  const versionDir = path.join(root, "versions", "metars");
  const deltaDir = path.join(root, "deltas", "metars");
  fs.mkdirSync(stateDir, { recursive: true });
  fs.mkdirSync(versionDir, { recursive: true });
  fs.mkdirSync(deltaDir, { recursive: true });

  let previous = null;
  for (const entry of states) {
    const stateJson = canonicalJson(entry.state);
    const stateSha256 = sha256Hex(stateJson);
    const statePath = path.join(stateDir, `${entry.version}.json`);
    fs.writeFileSync(statePath, `${prettyJson(entry.state)}\n`);

    let deltaFromPrevious = null;
    if (previous) {
      const delta = buildMetarStationDelta(previous.state, entry.state);
      const deltaJson = canonicalJson(delta);
      const deltaPath = path.join(deltaDir, `${previous.version}__${entry.version}.json`);
      fs.writeFileSync(deltaPath, `${prettyJson(delta)}\n`);
      deltaFromPrevious = {
        from_version: previous.version,
        from_state_sha256: previous.stateSha256,
        to_version: entry.version,
        to_state_sha256: stateSha256,
        url: relativeUrl(root, deltaPath),
        blob_sha256: sha256Hex(fs.readFileSync(deltaPath)),
      };
    }

    const versionManifest = {
      schema_version: 1,
      product: "metars",
      version: entry.version,
      previous: previous?.version ?? null,
      state: {
        url: relativeUrl(root, statePath),
        bytes: fs.statSync(statePath).size,
        blob_sha256: sha256Hex(fs.readFileSync(statePath)),
        state_sha256: stateSha256,
      },
      delta_from_previous: deltaFromPrevious,
    };
    const versionPath = path.join(versionDir, entry.name.replace(/\.zip$/, ".json"));
    fs.writeFileSync(versionPath, `${prettyJson(versionManifest)}\n`);
    entry.stateSha256 = stateSha256;
    entry.versionManifestPath = versionPath;
    entry.statePath = statePath;
    previous = entry;
  }

  const first = states[0];
  const currentPath = path.join(root, "current.json");
  const current = readJsonIfExists(currentPath) ?? {
    schema_version: 1,
    generated_at_utc: new Date().toISOString(),
    products: {},
  };
  current.products.metars = {
    current: first.version,
    version_manifest_url: relativeUrl(root, first.versionManifestPath),
    state_url: relativeUrl(root, first.statePath),
    state_sha256: first.stateSha256,
  };
  fs.writeFileSync(currentPath, `${prettyJson(current)}\n`);
}

function buildMetarStationDelta(fromState, toState) {
  const fromVersion = fromState.version_label;
  const toVersion = toState.version_label;
  const fromRecords = fromState.metars_by_station ?? {};
  const toRecords = toState.metars_by_station ?? {};
  const changed = {};
  const removed = [];
  for (const [stationId, record] of Object.entries(toRecords)) {
    if (canonicalJson(fromRecords[stationId]) !== canonicalJson(record)) {
      changed[stationId] = record;
    }
  }
  for (const stationId of Object.keys(fromRecords)) {
    if (!(stationId in toRecords)) {
      removed.push(stationId);
    }
  }
  removed.sort();
  return {
    schema_version: 1,
    product: "metars",
    from_version: fromVersion,
    to_version: toVersion,
    changed,
    removed,
  };
}

function canonicalJson(value) {
  return JSON.stringify(sortJson(value));
}

function prettyJson(value) {
  return JSON.stringify(sortJson(value), null, 2);
}

function sortJson(value) {
  if (Array.isArray(value)) {
    return value.map(sortJson);
  }
  if (!value || typeof value !== "object") {
    return value;
  }
  return Object.fromEntries(
    Object.keys(value)
      .sort()
      .map((key) => [key, sortJson(value[key])]),
  );
}

function sha256Hex(value) {
  return crypto.createHash("sha256").update(value).digest("hex");
}

function relativeUrl(root, filePath) {
  return path.relative(root, filePath).split(path.sep).join("/");
}

function readJsonIfExists(filePath) {
  if (!fs.existsSync(filePath)) {
    return null;
  }
  return JSON.parse(fs.readFileSync(filePath, "utf8"));
}

function copyTree(source, destination) {
  fs.mkdirSync(destination, { recursive: true });
  for (const entry of fs.readdirSync(source, { withFileTypes: true })) {
    const sourcePath = path.join(source, entry.name);
    const destinationPath = path.join(destination, entry.name);
    if (entry.isDirectory()) {
      copyTree(sourcePath, destinationPath);
    } else if (entry.isFile()) {
      fs.mkdirSync(path.dirname(destinationPath), { recursive: true });
      try {
        fs.linkSync(sourcePath, destinationPath);
      } catch {
        fs.copyFileSync(sourcePath, destinationPath);
      }
    }
  }
}

function resetCurrentToFirstFixtureVersions(root) {
  const versionsRoot = path.join(root, "versions");
  const currentPath = path.join(root, "current.json");
  const current = readJsonIfExists(currentPath) ?? {
    schema_version: 1,
    generated_at_utc: new Date().toISOString(),
    products: {},
  };
  current.products ??= {};
  if (!fs.existsSync(versionsRoot)) {
    fs.writeFileSync(currentPath, `${prettyJson(current)}\n`);
    return;
  }
  for (const product of fs.readdirSync(versionsRoot).sort()) {
    const productRoot = path.join(versionsRoot, product);
    if (!fs.statSync(productRoot).isDirectory()) {
      continue;
    }
    const versionFile = fs.readdirSync(productRoot).filter((name) => name.endsWith(".json")).sort()[0];
    if (!versionFile) {
      continue;
    }
    const versionPath = path.join(productRoot, versionFile);
    const manifest = readJsonIfExists(versionPath);
    if (!manifest?.version || !manifest?.state?.url || !manifest?.state?.state_sha256) {
      continue;
    }
    current.products[product] = {
      current: manifest.version,
      version_manifest_url: relativeUrl(root, versionPath),
      state_url: manifest.state.url,
      state_sha256: manifest.state.state_sha256,
    };
  }
  fs.writeFileSync(currentPath, `${prettyJson(current)}\n`);
}

function readZipMember(zipPath, memberName) {
  const bytes = fs.readFileSync(zipPath);
  const eocdOffset = findEndOfCentralDirectory(bytes);
  const centralDirectorySize = bytes.readUInt32LE(eocdOffset + 12);
  const centralDirectoryOffset = bytes.readUInt32LE(eocdOffset + 16);
  let offset = centralDirectoryOffset;
  const end = centralDirectoryOffset + centralDirectorySize;
  while (offset < end) {
    if (bytes.readUInt32LE(offset) !== 0x02014b50) {
      throw new Error(`${zipPath}: invalid central directory entry at ${offset}`);
    }
    const compressionMethod = bytes.readUInt16LE(offset + 10);
    const compressedSize = bytes.readUInt32LE(offset + 20);
    const fileNameLength = bytes.readUInt16LE(offset + 28);
    const extraLength = bytes.readUInt16LE(offset + 30);
    const commentLength = bytes.readUInt16LE(offset + 32);
    const localHeaderOffset = bytes.readUInt32LE(offset + 42);
    const fileName = bytes.subarray(offset + 46, offset + 46 + fileNameLength).toString("utf8");
    if (fileName === memberName) {
      if (bytes.readUInt32LE(localHeaderOffset) !== 0x04034b50) {
        throw new Error(`${zipPath}: invalid local header for ${memberName}`);
      }
      const localFileNameLength = bytes.readUInt16LE(localHeaderOffset + 26);
      const localExtraLength = bytes.readUInt16LE(localHeaderOffset + 28);
      const dataOffset = localHeaderOffset + 30 + localFileNameLength + localExtraLength;
      const compressed = bytes.subarray(dataOffset, dataOffset + compressedSize);
      if (compressionMethod === 0) {
        return Buffer.from(compressed);
      }
      if (compressionMethod === 8) {
        return zlib.inflateRawSync(compressed);
      }
      throw new Error(`${zipPath}: unsupported compression method ${compressionMethod} for ${memberName}`);
    }
    offset += 46 + fileNameLength + extraLength + commentLength;
  }
  throw new Error(`${zipPath}: missing ${memberName}`);
}

function findEndOfCentralDirectory(bytes) {
  const minimumOffset = Math.max(0, bytes.length - 65557);
  for (let offset = bytes.length - 22; offset >= minimumOffset; offset -= 1) {
    if (bytes.readUInt32LE(offset) === 0x06054b50) {
      return offset;
    }
  }
  throw new Error("missing ZIP end of central directory");
}
