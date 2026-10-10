// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import { describe, expect, it, vi } from "vitest";
import { executeDocumentRequest, prepareLocalDocuments, setLocalDocumentHost } from "./localDocuments";

function memoryStorage() {
  const values = new Map<string, string>();
  return {
    getItem: (key: string) => values.get(key) ?? null,
    setItem: (key: string, value: string) => { values.set(key, value); },
    removeItem: (key: string) => { values.delete(key); },
  } as Storage;
}

describe("generic local document host", () => {
  it("preserves existing text documents and roundtrips opaque binary and deletion", () => {
    const storage = memoryStorage();
    storage.setItem("old", '{"version":1}');
    expect(executeDocumentRequest(storage, { kind: "localDocument", id: 1, key: "old", operation: "read" }).bytes)
      .toEqual(Array.from(new TextEncoder().encode('{"version":1}')));
    for (const bytes of [[255, 0, 254], [0xef, 0xbb, 0xbf, 65], [], Array.from(new TextEncoder().encode("aerobag-bytes-v1:literal"))]) {
      expect(executeDocumentRequest(storage, { kind: "localDocument", id: 2, key: "binary", operation: "write", bytes }).error).toBeNull();
      expect(executeDocumentRequest(storage, { kind: "localDocument", id: 3, key: "binary", operation: "read" }).bytes).toEqual(bytes);
    }
    executeDocumentRequest(storage, { kind: "localDocument", id: 4, key: "binary", operation: "write", bytes: null });
    expect(storage.getItem("binary")).toBeNull();
  });

  it("returns quota, read and deletion failures instead of reporting success", () => {
    const storage = memoryStorage();
    storage.setItem("a", "old");
    storage.setItem = () => { throw new Error("quota full"); };
    expect(executeDocumentRequest(storage, { kind: "localDocument", id: 1, key: "a", operation: "write", bytes: [65] }).error).toBe("quota full");
    expect(storage.getItem("a")).toBe("old");
    storage.removeItem = () => { throw new Error("delete denied"); };
    expect(executeDocumentRequest(storage, { kind: "localDocument", id: 1, key: "a", operation: "write", bytes: null }).error).toBe("delete denied");
    storage.getItem = () => { throw new Error("read denied"); };
    expect(executeDocumentRequest(storage, { kind: "localDocument", id: 1, key: "a", operation: "read" })).toEqual({bytes: null, error: "read denied"});
  });

  it("reports completion to the core callback only after the host write finishes", async () => {
    let finish!: (value: {bytes: null; error: string | null}) => void;
    setLocalDocumentHost(async (_key, operation) => operation === "read"
      ? { bytes: null, error: "startup read denied" }
      : new Promise(resolve => { finish = resolve; }));
    await prepareLocalDocuments(["a"]);
    const globals = globalThis as unknown as {
      __aerobagReadDocument(key: string): {error: string | null};
      __aerobagWriteDocument(key: string, bytes: Uint8Array, done: (error: string | null) => void): void;
    };
    expect(globals.__aerobagReadDocument("a").error).toBe("startup read denied");
    const completed = vi.fn();
    globals.__aerobagWriteDocument("a", new Uint8Array([65]), completed);
    expect(completed).not.toHaveBeenCalled();
    finish({ bytes: null, error: "write denied" });
    await vi.waitFor(() => expect(completed).toHaveBeenCalledWith("write denied"));
  });
});
