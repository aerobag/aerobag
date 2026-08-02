// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import type { CloudAuthorizationMode } from "../generated/cloudWire";

declare const __AEROBAG_GOOGLE_DRIVE_CLIENT_ID__: string;

const GOOGLE_IDENTITY_SCRIPT_ID = "aerobag-google-identity-services";
const GOOGLE_IDENTITY_SCRIPT_URL = "https://accounts.google.com/gsi/client";
const DRIVE_SCOPE = "https://www.googleapis.com/auth/drive.appdata";
const DRIVE_API_ROOT = "https://www.googleapis.com/drive/v3";

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
  principal: {
    stable_id: string;
    display_label: string;
  };
};

export type GoogleDriveAuthorizationFailureKind =
  | "interaction_required"
  | "denied"
  | "transient"
  | "permanent";

export class GoogleDriveAuthorizationError extends Error {
  constructor(
    readonly kind: GoogleDriveAuthorizationFailureKind,
    message: string,
  ) {
    super(message);
    this.name = "GoogleDriveAuthorizationError";
  }
}

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

export async function authorizeGoogleDrive(
  mode: CloudAuthorizationMode,
  scopes: string[],
): Promise<GoogleDriveAuthorization> {
  await preloadGoogleDriveAuthorization();
  const clientId = __AEROBAG_GOOGLE_DRIVE_CLIENT_ID__.trim();
  if (!clientId) {
    throw new Error("This build has no Google Drive OAuth client ID.");
  }
  const googleOauth = oauth2();
  if (!googleOauth) {
    throw new Error("Google Identity Services did not initialize.");
  }
  const token = await new Promise<{ accessToken: string; expiresAtEpochMs: number }>((resolve, reject) => {
    const client = googleOauth.initTokenClient({
      client_id: clientId,
      scope: scopes.join(" ") || DRIVE_SCOPE,
      callback: (response) => {
        if (!response.access_token) {
          reject(authorizationError(
            response.error,
            response.error_description ?? response.error ?? "Google Drive authorization failed.",
            mode,
          ));
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
        reject(authorizationError(
          error.type,
          error.message ?? error.type ?? "Google Drive authorization popup failed.",
          mode,
        ));
      },
    });
    client.requestAccessToken({ prompt: mode === "interactive" ? "consent" : "" });
  });
  return {
    ...token,
    principal: await readGoogleDrivePrincipal(token.accessToken),
  };
}

export async function readGoogleDrivePrincipal(accessToken: string): Promise<GoogleDriveAuthorization["principal"]> {
  const response = await fetch(
    `${DRIVE_API_ROOT}/about?fields=user(permissionId,displayName,emailAddress)`,
    { headers: authorizationHeaders(accessToken) },
  );
  if (!response.ok) {
    const detail = (await response.text()).slice(0, 500).trim();
    const message = `Could not identify the authorized Google Drive account: HTTP ${response.status}${detail ? ` ${detail}` : ""}`;
    throw new GoogleDriveAuthorizationError(
      response.status === 401 || response.status === 403
        ? "interaction_required"
        : response.status === 408 || response.status === 425 || response.status === 429 || response.status >= 500
          ? "transient"
          : "permanent",
      message,
    );
  }
  const payload = await response.json() as {
    user?: { permissionId?: string; displayName?: string; emailAddress?: string };
  };
  const stableId = payload.user?.permissionId?.trim();
  if (!stableId) {
    throw new GoogleDriveAuthorizationError(
      "permanent",
      "Google Drive did not provide a stable account identifier.",
    );
  }
  return {
    stable_id: stableId,
    display_label: payload.user?.emailAddress?.trim()
      || payload.user?.displayName?.trim()
      || "Google Drive user",
  };
}

function authorizationHeaders(accessToken: string, extra: Record<string, string> = {}): HeadersInit {
  return { Authorization: `Bearer ${accessToken}`, ...extra };
}

function authorizationError(
  code: string | undefined,
  message: string,
  mode: CloudAuthorizationMode,
): GoogleDriveAuthorizationError {
  switch (code) {
    case "access_denied":
    case "popup_closed":
      return new GoogleDriveAuthorizationError("denied", message);
    case "temporarily_unavailable":
    case "server_error":
      return new GoogleDriveAuthorizationError("transient", message);
    case "interaction_required":
    case "consent_required":
    case "login_required":
      return new GoogleDriveAuthorizationError("interaction_required", message);
    case "invalid_client":
    case "invalid_request":
    case "invalid_scope":
      return new GoogleDriveAuthorizationError("permanent", message);
    default:
      return new GoogleDriveAuthorizationError(
        mode === "silent" ? "interaction_required" : "permanent",
        message,
      );
  }
}
