// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import type {
  CloudAuthorizationRequest,
  CloudAuthorizationResponse,
  CloudHttpRequest,
  CloudHttpResponse,
  CloudProviderKind,
} from "../generated/cloudWire";
import {
  authorizeGoogleDrive,
  GoogleDriveAuthorizationError,
} from "./googleDriveCloudProvider";

export type CloudProviderAuthorization = {
  provider: CloudProviderKind;
  credential: string;
  expiresAtEpochMs: number;
};

export type CloudAuthorizationExecution = {
  authorization: CloudProviderAuthorization | null;
  response: CloudAuthorizationResponse;
};

export function beginInteractiveCloudAuthorization(
  provider: CloudProviderKind,
  scopes: string[],
): Promise<CloudAuthorizationExecution> {
  return executeCloudAuthorization(provider, "interactive", scopes);
}

export async function executeCloudAuthorizationRequest(
  request: CloudAuthorizationRequest,
): Promise<CloudAuthorizationExecution> {
  return executeCloudAuthorization(request.provider, request.mode, request.scopes);
}

async function executeCloudAuthorization(
  provider: CloudProviderKind,
  mode: CloudAuthorizationRequest["mode"],
  scopes: string[],
): Promise<CloudAuthorizationExecution> {
  switch (provider) {
    case "google_drive":
      try {
        const authorization = await authorizeGoogleDrive(mode, scopes);
        return {
          authorization: {
            provider,
            credential: authorization.accessToken,
            expiresAtEpochMs: authorization.expiresAtEpochMs,
          },
          response: {
            state: "authorized",
            expires_at_epoch_ms: authorization.expiresAtEpochMs,
            principal: authorization.principal,
          },
        };
      } catch (error) {
        return { authorization: null, response: authorizationFailure(error) };
      }
    case "aerobag_cloud":
      return {
        authorization: null,
        response: {
          state: "permanent_failure",
          diagnostic: "Aerobag Cloud authorization is not available in this build.",
        },
      };
  }
}

export async function executeCloudHttpRequest(
  request: CloudHttpRequest,
  authorization: CloudProviderAuthorization | null,
): Promise<CloudHttpResponse> {
  if (request.provider === "google_drive"
      && (authorization?.provider !== request.provider || authorization.expiresAtEpochMs <= Date.now())) {
    return { result: "transport_error", detail: "Cloud provider authorization is unavailable or expired." };
  }

  try {
    const headers = new Headers(request.headers.map((header) => [header.name, header.value]));
    if (request.provider === "google_drive") {
      headers.set("Authorization", `Bearer ${authorization!.credential}`);
    }
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

function authorizationFailure(error: unknown): CloudAuthorizationResponse {
  const diagnostic = error instanceof Error ? error.message : String(error);
  if (!(error instanceof GoogleDriveAuthorizationError)) {
    return { state: "permanent_failure", diagnostic };
  }
  switch (error.kind) {
    case "interaction_required":
      return { state: "interaction_required", diagnostic };
    case "denied":
      return { state: "denied", diagnostic };
    case "transient":
      return { state: "transient_failure", diagnostic };
    case "permanent":
      return { state: "permanent_failure", diagnostic };
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
