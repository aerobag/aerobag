// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import type { CloudProviderRequest, CloudProviderResponse } from "./appCoreAdapter";

declare const __AEROBAG_GOOGLE_DRIVE_CLIENT_ID__: string;

const GOOGLE_IDENTITY_SCRIPT_ID = "aerobag-google-identity-services";
const GOOGLE_IDENTITY_SCRIPT_URL = "https://accounts.google.com/gsi/client";
const DRIVE_SCOPE = "https://www.googleapis.com/auth/drive.appdata";
const DRIVE_API_ROOT = "https://www.googleapis.com/drive/v3";
const DRIVE_UPLOAD_ROOT = "https://www.googleapis.com/upload/drive/v3";
const CLOUD_OBJECT_MIME = "application/vnd.aerobag.cloud-object";
const MAX_CLOUD_OBJECT_BYTES = 4 * 1024 * 1024;

type GoogleTokenResponse = {
  access_token?: string;
  expires_in?: number | string;
  error?: string;
  error_description?: string;
};

type GoogleOauth2 = {
  initTokenClient(config: {
    client_id: string;
    scope: string;
    callback(response: GoogleTokenResponse): void;
    error_callback?(error: { type?: string; message?: string }): void;
  }): { requestAccessToken(overrides?: { prompt?: string }): void };
  revoke(accessToken: string, callback?: () => void): void;
};

export type GoogleDriveAuthorization = {
  accessToken: string;
  expiresAtEpochMs: number;
};

function oauth2(): GoogleOauth2 | null {
  const google = (window as unknown as {
    google?: { accounts?: { oauth2?: GoogleOauth2 } };
  }).google;
  return google?.accounts?.oauth2 ?? null;
}

export function preloadGoogleDriveAuthorization(): Promise<void> {
  if (oauth2()) {
    return Promise.resolve();
  }
  const existing = document.getElementById(GOOGLE_IDENTITY_SCRIPT_ID) as HTMLScriptElement | null;
  if (existing) {
    return new Promise((resolve, reject) => {
      existing.addEventListener("load", () => resolve(), { once: true });
      existing.addEventListener("error", () => reject(new Error("Google Identity Services failed to load")), { once: true });
    });
  }
  return new Promise((resolve, reject) => {
    const script = document.createElement("script");
    script.id = GOOGLE_IDENTITY_SCRIPT_ID;
    script.src = GOOGLE_IDENTITY_SCRIPT_URL;
    script.async = true;
    script.defer = true;
    script.onload = () => resolve();
    script.onerror = () => reject(new Error("Google Identity Services failed to load"));
    document.head.append(script);
  });
}

export async function authorizeGoogleDrive(): Promise<GoogleDriveAuthorization> {
  await preloadGoogleDriveAuthorization();
  const clientId = __AEROBAG_GOOGLE_DRIVE_CLIENT_ID__.trim();
  if (!clientId) {
    throw new Error("This build has no Google Drive OAuth client ID.");
  }
  const googleOauth = oauth2();
  if (!googleOauth) {
    throw new Error("Google Identity Services did not initialize.");
  }
  return new Promise((resolve, reject) => {
    const client = googleOauth.initTokenClient({
      client_id: clientId,
      scope: DRIVE_SCOPE,
      callback: (response) => {
        if (!response.access_token) {
          reject(new Error(response.error_description ?? response.error ?? "Google Drive authorization failed."));
          return;
        }
        const lifetimeSeconds = Number(response.expires_in);
        const safeLifetimeSeconds = Number.isFinite(lifetimeSeconds) && lifetimeSeconds > 0
          ? lifetimeSeconds
          : 3_600;
        resolve({
          accessToken: response.access_token,
          expiresAtEpochMs: Date.now() + safeLifetimeSeconds * 1_000,
        });
      },
      error_callback: (error) => {
        reject(new Error(error.message ?? error.type ?? "Google Drive authorization popup failed."));
      },
    });
    client.requestAccessToken({ prompt: "consent" });
  });
}

export function revokeGoogleDrive(accessToken: string): Promise<void> {
  const googleOauth = oauth2();
  if (!googleOauth) {
    return Promise.resolve();
  }
  return new Promise((resolve) => googleOauth.revoke(accessToken, resolve));
}

export async function executeGoogleDriveCloudRequest(
  request: CloudProviderRequest,
  accessToken: string | null,
): Promise<CloudProviderResponse> {
  if (request.provider !== "google_drive") {
    return permanentError(`Unsupported cloud provider ${request.provider}.`);
  }
  if (!accessToken) {
    return unauthorizedError("Google Drive authorization is required.");
  }
  try {
    switch (request.operation.operation) {
      case "allocate_ids":
        return await allocateIds(accessToken, request.operation.count);
      case "read":
        return await readObject(accessToken, request.operation.id);
      case "create_once":
        return await createObjectOnce(
          accessToken,
          request.operation.id,
          request.operation.name,
          request.operation.bytes_base64,
        );
      case "delete":
        return await deleteObject(accessToken, request.operation.id);
      case "list":
        return await listObjects(accessToken, request.operation.page_token ?? null);
    }
  } catch (error) {
    return transientError(error instanceof Error ? error.message : String(error));
  }
}

async function allocateIds(accessToken: string, count: number): Promise<CloudProviderResponse> {
  const response = await fetch(
    `${DRIVE_API_ROOT}/files/generateIds?count=${encodeURIComponent(String(count))}&space=appDataFolder&type=files`,
    { headers: authorizationHeaders(accessToken) },
  );
  if (!response.ok) {
    return responseError(response, "allocate Google Drive object IDs");
  }
  const payload = await response.json() as { ids?: string[]; space?: string };
  if (payload.space !== "appDataFolder" || payload.ids?.length !== count) {
    return permanentError(`Google Drive returned an invalid generated-ID response.`);
  }
  return { result: "allocated_ids", ids: payload.ids };
}

async function readObject(accessToken: string, id: string): Promise<CloudProviderResponse> {
  const response = await fetch(
    `${DRIVE_API_ROOT}/files/${encodeURIComponent(id)}?alt=media`,
    { headers: authorizationHeaders(accessToken) },
  );
  if (response.status === 404) {
    return { result: "read", bytes_base64: null };
  }
  if (!response.ok) {
    return responseError(response, "read Google Drive cloud object");
  }
  const contentLength = Number(response.headers.get("content-length"));
  if (Number.isFinite(contentLength) && contentLength > MAX_CLOUD_OBJECT_BYTES) {
    return permanentError(`Google Drive cloud object exceeds ${MAX_CLOUD_OBJECT_BYTES} bytes.`);
  }
  const bytes = new Uint8Array(await response.arrayBuffer());
  if (bytes.byteLength > MAX_CLOUD_OBJECT_BYTES) {
    return permanentError(`Google Drive cloud object exceeds ${MAX_CLOUD_OBJECT_BYTES} bytes.`);
  }
  return { result: "read", bytes_base64: encodeBase64Url(bytes) };
}

async function createObjectOnce(
  accessToken: string,
  id: string,
  name: string,
  bytesBase64: string,
): Promise<CloudProviderResponse> {
  const bytes = decodeBase64Url(bytesBase64);
  if (bytes.byteLength > MAX_CLOUD_OBJECT_BYTES) {
    return permanentError(`Cloud object exceeds ${MAX_CLOUD_OBJECT_BYTES} bytes.`);
  }
  const boundary = `aerobag_cloud_${crypto.randomUUID().replaceAll("-", "")}`;
  const metadata = JSON.stringify({
    id,
    name,
    mimeType: CLOUD_OBJECT_MIME,
    parents: ["appDataFolder"],
  });
  const body = new Blob([
    `--${boundary}\r\nContent-Type: application/json; charset=UTF-8\r\n\r\n${metadata}\r\n`,
    `--${boundary}\r\nContent-Type: ${CLOUD_OBJECT_MIME}\r\n\r\n`,
    bytes.buffer as ArrayBuffer,
    `\r\n--${boundary}--\r\n`,
  ]);
  const response = await fetch(
    `${DRIVE_UPLOAD_ROOT}/files?uploadType=multipart&fields=id`,
    {
      method: "POST",
      headers: authorizationHeaders(accessToken, {
        "Content-Type": `multipart/related; boundary=${boundary}`,
      }),
      body,
    },
  );
  if (response.status === 409 || response.status === 412) {
    return { result: "already_exists" };
  }
  if (!response.ok) {
    return responseError(response, "create Google Drive cloud object");
  }
  return { result: "created" };
}

async function deleteObject(accessToken: string, id: string): Promise<CloudProviderResponse> {
  const response = await fetch(
    `${DRIVE_API_ROOT}/files/${encodeURIComponent(id)}`,
    { method: "DELETE", headers: authorizationHeaders(accessToken) },
  );
  if (response.status === 404) {
    return { result: "deleted", existed: false };
  }
  if (!response.ok) {
    return responseError(response, "delete Google Drive cloud object");
  }
  return { result: "deleted", existed: true };
}

async function listObjects(
  accessToken: string,
  pageToken: string | null,
): Promise<CloudProviderResponse> {
  const params = new URLSearchParams({
    spaces: "appDataFolder",
    pageSize: "1000",
    q: `trashed = false and mimeType = '${CLOUD_OBJECT_MIME}'`,
    fields: "nextPageToken,files(id,size,createdTime)",
  });
  if (pageToken) {
    params.set("pageToken", pageToken);
  }
  const response = await fetch(
    `${DRIVE_API_ROOT}/files?${params.toString()}`,
    { headers: authorizationHeaders(accessToken) },
  );
  if (!response.ok) {
    return responseError(response, "list Google Drive cloud objects");
  }
  const payload = await response.json() as {
    files?: Array<{ id?: string; size?: string; createdTime?: string }>;
    nextPageToken?: string;
  };
  const objects = (payload.files ?? []).map((file) => {
    const sizeBytes = Number(file.size);
    if (!file.id || !Number.isSafeInteger(sizeBytes) || sizeBytes < 0) {
      throw new Error("Google Drive returned invalid cloud object metadata.");
    }
    return {
      id: file.id,
      size_bytes: sizeBytes,
      ...(file.createdTime ? { created_at: file.createdTime } : {}),
    };
  });
  return {
    result: "listed",
    objects,
    ...(payload.nextPageToken ? { next_page_token: payload.nextPageToken } : {}),
  };
}

function authorizationHeaders(accessToken: string, extra: Record<string, string> = {}): HeadersInit {
  return { Authorization: `Bearer ${accessToken}`, ...extra };
}

async function responseError(response: Response, operation: string): Promise<CloudProviderResponse> {
  const detailBody = (await response.text()).slice(0, 500).trim();
  const detail = `${operation} failed: HTTP ${response.status}${detailBody ? ` ${detailBody}` : ""}`;
  if (response.status === 401 || response.status === 403) {
    return unauthorizedError(detail);
  }
  if (response.status === 408 || response.status === 425 || response.status === 429 || response.status >= 500) {
    return transientError(detail);
  }
  return permanentError(detail);
}

function unauthorizedError(detail: string): CloudProviderResponse {
  return { result: "error", kind: "unauthorized", detail };
}

function transientError(detail: string): CloudProviderResponse {
  return { result: "error", kind: "transient", detail };
}

function permanentError(detail: string): CloudProviderResponse {
  return { result: "error", kind: "permanent", detail };
}

function decodeBase64Url(value: string): Uint8Array {
  const padded = value.replaceAll("-", "+").replaceAll("_", "/").padEnd(Math.ceil(value.length / 4) * 4, "=");
  const binary = atob(padded);
  return Uint8Array.from(binary, (character) => character.charCodeAt(0));
}

function encodeBase64Url(bytes: Uint8Array): string {
  let binary = "";
  for (let offset = 0; offset < bytes.length; offset += 0x8000) {
    binary += String.fromCharCode(...bytes.subarray(offset, offset + 0x8000));
  }
  return btoa(binary).replaceAll("+", "-").replaceAll("/", "_").replace(/=+$/, "");
}
