// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import type {
  CloudAuthorizationResponse,
  CloudProviderKind,
  CloudProviderRequest,
  CloudProviderResponse,
} from "../generated/cloudWire";
import {
  authorizeGoogleDrive,
  executeGoogleDriveCloudRequest,
  preloadGoogleDriveAuthorization,
} from "./googleDriveCloudProvider";

export type CloudProviderAuthorization = {
  provider: CloudProviderKind;
  credential: string;
  response: CloudAuthorizationResponse;
};

export function prepareCloudProvider(provider: CloudProviderKind): Promise<void> {
  switch (provider) {
    case "google_drive":
      return preloadGoogleDriveAuthorization();
    case "aerobag_cloud":
      return Promise.resolve();
  }
}

export async function authorizeCloudProvider(
  provider: CloudProviderKind,
): Promise<CloudProviderAuthorization> {
  switch (provider) {
    case "google_drive": {
      const authorization = await authorizeGoogleDrive();
      return {
        provider,
        credential: authorization.accessToken,
        response: {
          state: "authorized",
          expires_at_epoch_ms: authorization.expiresAtEpochMs,
          principal: authorization.principal,
        },
      };
    }
    case "aerobag_cloud":
      throw new Error("Aerobag Cloud authorization is not available in this build.");
  }
}

export async function executeCloudProviderRequest(
  request: CloudProviderRequest,
  authorization: CloudProviderAuthorization | null,
): Promise<CloudProviderResponse> {
  if (authorization?.provider !== request.provider) {
    return {
      result: "error",
      kind: "unauthorized",
      detail: "Cloud provider authorization is required.",
    };
  }
  switch (request.provider) {
    case "google_drive":
      return executeGoogleDriveCloudRequest(request, authorization.credential);
    case "aerobag_cloud":
      return {
        result: "error",
        kind: "permanent",
        detail: "Aerobag Cloud is not available in this build.",
      };
  }
}
