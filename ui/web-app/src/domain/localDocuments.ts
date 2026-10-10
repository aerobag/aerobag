// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

export type DocumentRead = { bytes: number[] | null; error: string | null };
export type DocumentRequest = { kind: "localDocument"; id: number; key: string; operation: "read" | "write"; bytes?: number[] | null };
export type DocumentResponse = { kind: "localDocumentResult"; id: number; result: DocumentRead };

const binaryPrefix = "aerobag-bytes-v1:";

// The existing text documents keep their keys and encoding. Binary documents
// use an unambiguous envelope; neither host interprets application schemas.
export function executeDocumentRequest(storage: Storage, request: DocumentRequest): DocumentRead {
  try {
    if (request.operation === "read") {
      const text = storage.getItem(request.key);
      const bytes = text === null ? null : text.startsWith(binaryPrefix)
        ? Uint8Array.from(atob(text.slice(binaryPrefix.length)), c => c.charCodeAt(0))
        : new TextEncoder().encode(text);
      return { bytes: bytes === null ? null : Array.from(bytes), error: null };
    }
    if (request.bytes === null) storage.removeItem(request.key);
    else {
      const bytes = new Uint8Array(request.bytes!);
      let encoded: string;
      try {
        encoded = new TextDecoder("utf-8", { fatal: true, ignoreBOM: true }).decode(bytes);
        if (encoded.startsWith(binaryPrefix)) throw new Error("reserved prefix");
      } catch {
        let binary = "";
        for (const byte of bytes) binary += String.fromCharCode(byte);
        encoded = binaryPrefix + btoa(binary);
      }
      storage.setItem(request.key, encoded);
    }
    return { bytes: null, error: null };
  } catch (error) {
    return { bytes: null, error: error instanceof Error ? error.message : String(error) };
  }
}

type Host = (key: string, operation: "read" | "write", bytes?: number[] | null) => Promise<DocumentRead>;
type Globals = {
  __aerobagReadDocument?: (key: string) => DocumentRead;
  __aerobagWriteDocument?: (key: string, bytes: Uint8Array | null, complete: (error: string | null) => void) => void;
};
let host: Host = async (key, operation, bytes) => {
  try { return executeDocumentRequest(globalThis.localStorage, { kind: "localDocument", id: 0, key, operation, bytes }); }
  catch (error) { return { bytes: null, error: String(error) }; }
};

export function setLocalDocumentHost(value: Host): void { host = value; }

export async function prepareLocalDocuments(keys: string[]): Promise<void> {
  const documents = new Map<string, DocumentRead>();
  for (const key of keys) documents.set(key, await host(key, "read"));
  const globals = globalThis as Globals;
  globals.__aerobagReadDocument = key => documents.get(key) ?? { bytes: null, error: `Document not preloaded: ${key}` };
  globals.__aerobagWriteDocument = (key, bytes, complete) => {
    void host(key, "write", bytes === null ? null : Array.from(bytes)).then(
      result => complete(result.error),
      error => complete(String(error)),
    );
  };
}
