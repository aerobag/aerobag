// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import type {
  CloudHttpRequest,
  CloudHttpResponse,
} from "../generated/cloudWire";

export async function executeCloudHttpRequest(
  request: CloudHttpRequest,
): Promise<CloudHttpResponse> {
  try {
    const headers = new Headers(request.headers.map((header) => [header.name, header.value]));
    const response = await fetch(request.url, {
      method: request.method.toUpperCase(),
      headers,
      body: request.body_base64 == null ? undefined : decodeBase64Url(request.body_base64),
    });
    const declaredLength = Number(response.headers.get("content-length"));
    if (Number.isFinite(declaredLength) && declaredLength > request.max_response_bytes) {
      await response.body?.cancel();
      return { result: "response_too_large", limit_bytes: request.max_response_bytes };
    }
    const bytes = await readBoundedBody(response, request.max_response_bytes);
    if (bytes === null) {
      return { result: "response_too_large", limit_bytes: request.max_response_bytes };
    }
    return {
      result: "completed",
      status_code: response.status,
      body_base64: encodeBase64Url(bytes),
    };
  } catch (error) {
    return {
      result: "transport_error",
      detail: error instanceof Error ? error.message : String(error),
    };
  }
}

async function readBoundedBody(response: Response, limit: number): Promise<Uint8Array | null> {
  if (!response.body) {
    return new Uint8Array();
  }
  const reader = response.body.getReader();
  const chunks: Uint8Array[] = [];
  let length = 0;
  while (true) {
    const { done, value } = await reader.read();
    if (done) {
      break;
    }
    length += value.byteLength;
    if (length > limit) {
      await reader.cancel();
      return null;
    }
    chunks.push(value);
  }
  const bytes = new Uint8Array(length);
  let offset = 0;
  for (const chunk of chunks) {
    bytes.set(chunk, offset);
    offset += chunk.byteLength;
  }
  return bytes;
}

function decodeBase64Url(value: string): ArrayBuffer {
  const padded = value.replaceAll("-", "+").replaceAll("_", "/").padEnd(Math.ceil(value.length / 4) * 4, "=");
  const binary = atob(padded);
  return Uint8Array.from(binary, (character) => character.charCodeAt(0)).buffer;
}

function encodeBase64Url(bytes: Uint8Array): string {
  let binary = "";
  for (let offset = 0; offset < bytes.length; offset += 0x8000) {
    binary += String.fromCharCode(...bytes.subarray(offset, offset + 0x8000));
  }
  return btoa(binary).replaceAll("+", "-").replaceAll("/", "_").replace(/=+$/, "");
}
