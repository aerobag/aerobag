declare const __AEROBAG_LIVE_FEEDS_ORIGIN__: string | null;

export type LiveFeedSourceRuntime = {
  window?: { location?: { origin?: string | null } } | null;
  location?: { origin?: string | null } | null;
};

export function resolveLiveFeedSourceUrl(
  configuredOrigin: string | null | undefined,
  runtime: LiveFeedSourceRuntime = globalThis as unknown as LiveFeedSourceRuntime,
): string {
  const configured = configuredOrigin?.trim();
  if (configured) {
    return configured.replace(/\/+$/, "");
  }
  const origin = runtime.window?.location?.origin ?? runtime.location?.origin ?? "";
  if (origin) {
    return origin;
  }
  return "";
}

export function resolveLiveFeedResourceUrl(
  resourceUrl: string,
  configuredOrigin: string | null | undefined = __AEROBAG_LIVE_FEEDS_ORIGIN__,
  runtime: LiveFeedSourceRuntime = globalThis as unknown as LiveFeedSourceRuntime,
): string {
  if (/^[a-z][a-z0-9+.-]*:/i.test(resourceUrl)) {
    return resourceUrl;
  }
  const liveFeedsPrefix = "/live-feeds";
  if (resourceUrl !== liveFeedsPrefix && !resourceUrl.startsWith(`${liveFeedsPrefix}/`)) {
    return resourceUrl;
  }
  const sourceUrl = resolveLiveFeedSourceUrl(configuredOrigin, runtime);
  if (!sourceUrl) {
    return resourceUrl;
  }
  return `${sourceUrl}${resourceUrl}`;
}

export function liveFeedSourceUrl(): string {
  return resolveLiveFeedSourceUrl(__AEROBAG_LIVE_FEEDS_ORIGIN__);
}
