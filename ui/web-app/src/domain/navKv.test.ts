import { describe, expect, it } from "vitest";
import { NavKvRoot } from "./navKv";

const encoder = new TextEncoder();
const magic = encoder.encode("AEROBAGNAVKV0001");
const headerLen = 48;
const entryLen = 8;

function buildRoot(pairs: Array<[string, string]>, pageSize: number) {
  const sorted = [...pairs].sort((left, right) => left[0].localeCompare(right[0]));
  const keyParts: Uint8Array[] = [];
  const valueParts: Uint8Array[] = [];
  const entries: Array<{ keyOffset: number; valueOffset: number }> = [];
  let keyOffset = 0;
  let valueOffset = 0;
  for (const [key, value] of sorted) {
    entries.push({ keyOffset, valueOffset });
    const keyBytes = encoder.encode(key);
    const valueBytes = encoder.encode(value);
    keyParts.push(keyBytes);
    valueParts.push(valueBytes);
    keyOffset += keyBytes.byteLength;
    valueOffset += valueBytes.byteLength;
  }
  entries.push({ keyOffset, valueOffset });
  const keyBytes = concat(keyParts);
  const valueBytes = concat(valueParts);
  const root = new Uint8Array(headerLen + entries.length * entryLen + keyBytes.byteLength);
  root.set(magic, 0);
  const view = new DataView(root.buffer);
  view.setUint32(16, 1, true);
  view.setUint32(20, sorted.length, true);
  view.setUint32(24, pageSize, true);
  view.setUint32(28, headerLen, true);
  view.setUint32(32, headerLen + entries.length * entryLen, true);
  view.setUint32(36, keyBytes.byteLength, true);
  view.setUint32(40, valueBytes.byteLength, true);
  for (let index = 0; index < entries.length; index += 1) {
    const offset = headerLen + index * entryLen;
    view.setUint32(offset, entries[index].keyOffset, true);
    view.setUint32(offset + 4, entries[index].valueOffset, true);
  }
  root.set(keyBytes, headerLen + entries.length * entryLen);
  const pages: Uint8Array[] = [];
  for (let offset = 0; offset < valueBytes.byteLength; offset += pageSize) {
    pages.push(valueBytes.slice(offset, offset + pageSize));
  }
  return { root, pages };
}

function concat(parts: Uint8Array[]) {
  const out = new Uint8Array(parts.reduce((sum, part) => sum + part.byteLength, 0));
  let offset = 0;
  for (const part of parts) {
    out.set(part, offset);
    offset += part.byteLength;
  }
  return out;
}

describe("NavKvRoot", () => {
  it("extracts an exact value", async () => {
    const built = buildRoot([["waypoint/id/KRDD", "{\"id\":\"KRDD\"}"], ["chart/catalog", "{}"]], 8);
    const root = NavKvRoot.parse(built.root);
    const value = await root.extractValue("waypoint/id/KRDD", async (page) => built.pages[page]);
    expect(new TextDecoder().decode(value ?? new Uint8Array())).toBe("{\"id\":\"KRDD\"}");
  });

  it("returns null for missing values", async () => {
    const built = buildRoot([["chart/catalog", "{}"]], 8);
    const root = NavKvRoot.parse(built.root);
    expect(root.getValueRange("missing")).toBeNull();
    expect(await root.extractValue("missing", async (page) => built.pages[page])).toBeNull();
  });

  it("finds prefix keys without returning the sentinel", () => {
    const built = buildRoot([
      ["waypoint/id/KRDD", "1"],
      ["waypoint/id/KRNT", "2"],
      ["waypoint/suggest/KR", "3"],
    ], 8);
    const root = NavKvRoot.parse(built.root);
    expect(root.prefixKeys("waypoint/id/")).toEqual(["waypoint/id/KRDD", "waypoint/id/KRNT"]);
  });

  it("extracts values that cross page boundaries", async () => {
    const built = buildRoot([["k", "abcdefghijklmnop"]], 5);
    const root = NavKvRoot.parse(built.root);
    const value = await root.extractValue("k", async (page) => built.pages[page]);
    expect(new TextDecoder().decode(value ?? new Uint8Array())).toBe("abcdefghijklmnop");
  });

  it("rejects malformed magic", () => {
    const built = buildRoot([["a", "1"]], 8);
    built.root[0] = "X".charCodeAt(0);
    expect(() => NavKvRoot.parse(built.root)).toThrow(/invalid magic/);
  });

  it("rejects a bad sentinel", () => {
    const built = buildRoot([["a", "1"]], 8);
    const view = new DataView(built.root.buffer);
    view.setUint32(headerLen + entryLen, 0, true);
    expect(() => NavKvRoot.parse(built.root)).toThrow();
  });
});
