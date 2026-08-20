// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import type { NexradOverlayCachePlan } from "../generated/nexradOverlayWire";

type CacheEntry = {
  frameVersion: string;
  objectUrl: string | null;
  load: Promise<string>;
  abortController: AbortController;
};

export type NexradFrameCacheLoadResult = {
  loaded: number;
  failed: number;
};

type BlobLoader = (src: string, signal: AbortSignal) => Promise<Blob>;
type ObjectUrlFactory = (blob: Blob) => string;
type ObjectUrlReleaser = (url: string) => void;

async function fetchImageBlob(src: string, signal: AbortSignal): Promise<Blob> {
  const response = await fetch(src, { cache: "force-cache", signal });
  if (!response.ok) {
    throw new Error(`failed to load NEXRAD image ${src}: HTTP ${response.status}`);
  }
  return response.blob();
}

/**
 * Executes core's NEXRAD cache plan while keeping browser resource ownership here.
 * Core decides which frame versions remain useful and which tile URLs to fetch.
 */
export class NexradFrameImageCache {
  private readonly entries = new Map<string, CacheEntry>();

  constructor(
    private readonly loadBlob: BlobLoader = fetchImageBlob,
    private readonly createObjectUrl: ObjectUrlFactory = (blob) => URL.createObjectURL(blob),
    private readonly revokeObjectUrl: ObjectUrlReleaser = (url) => URL.revokeObjectURL(url),
  ) {}

  async applyPlan(plan: NexradOverlayCachePlan): Promise<NexradFrameCacheLoadResult> {
    const retainedVersions = new Set(plan.retained_frame_versions);
    this.prune(retainedVersions);

    const resources = new Map(
      plan.fetch_resources
        .filter((resource) => retainedVersions.has(resource.frame_version))
        .map((resource) => [resource.src, resource] as const),
    );
    const settled = await Promise.allSettled(
      Array.from(resources.values(), (resource) => this.ensureLoaded(resource.frame_version, resource.src)),
    );
    return {
      loaded: settled.filter((result) => result.status === "fulfilled").length,
      failed: settled.filter((result) => result.status === "rejected").length,
    };
  }

  imageUrlFor(src: string): string | null {
    return this.entries.get(src)?.objectUrl ?? null;
  }

  clear(): void {
    for (const src of Array.from(this.entries.keys())) {
      this.remove(src);
    }
  }

  cancelPendingLoads(): void {
    for (const [src, entry] of this.entries) {
      if (!entry.objectUrl) {
        this.remove(src);
      }
    }
  }

  private ensureLoaded(frameVersion: string, src: string): Promise<string> {
    const existing = this.entries.get(src);
    if (existing?.frameVersion === frameVersion) {
      return existing.load;
    }
    if (existing) {
      this.remove(src);
    }

    const entry: CacheEntry = {
      frameVersion,
      objectUrl: null,
      load: Promise.resolve(""),
      abortController: new AbortController(),
    };
    entry.load = this.loadBlob(src, entry.abortController.signal)
      .then((blob) => {
        const objectUrl = this.createObjectUrl(blob);
        if (this.entries.get(src) !== entry) {
          this.revokeObjectUrl(objectUrl);
          return objectUrl;
        }
        entry.objectUrl = objectUrl;
        return objectUrl;
      })
      .catch((error: unknown) => {
        if (this.entries.get(src) === entry) {
          this.entries.delete(src);
        }
        throw error;
      });
    this.entries.set(src, entry);
    return entry.load;
  }

  private prune(retainedVersions: ReadonlySet<string>): void {
    for (const [src, entry] of this.entries) {
      if (!retainedVersions.has(entry.frameVersion)) {
        this.remove(src);
      }
    }
  }

  private remove(src: string): void {
    const entry = this.entries.get(src);
    if (!entry) {
      return;
    }
    this.entries.delete(src);
    entry.abortController.abort();
    if (entry.objectUrl) {
      this.revokeObjectUrl(entry.objectUrl);
    }
  }
}
