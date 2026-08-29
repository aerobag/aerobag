// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

export const TEXT_RESOURCE_FETCH_ATTEMPTS = 2;

export type TextResourceFetchResult = {
  text: string;
  attempts: number;
};

class TextResourceHttpError extends Error {}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export async function fetchTextResource(
  source: string,
  fetchResource: typeof fetch = globalThis.fetch,
): Promise<TextResourceFetchResult> {
  let lastTransportError: unknown = new Error("unknown transport error");
  for (let attempt = 1; attempt <= TEXT_RESOURCE_FETCH_ATTEMPTS; attempt += 1) {
    try {
      const response = await fetchResource(source);
      if (!response.ok) {
        throw new TextResourceHttpError(`HTTP ${response.status}`);
      }
      return {
        text: await response.text(),
        attempts: attempt,
      };
    } catch (error) {
      if (error instanceof TextResourceHttpError) {
        throw new Error(`failed to fetch ${source}: ${error.message}`);
      }
      lastTransportError = error;
    }
  }
  throw new Error(
    `failed to fetch ${source} after ${TEXT_RESOURCE_FETCH_ATTEMPTS} attempts: ${errorMessage(lastTransportError)}`,
  );
}
