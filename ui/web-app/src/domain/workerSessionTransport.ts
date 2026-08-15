// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import type {
  UiSession,
  UiSessionProjectionLanding,
  UiSessionSnapshot,
} from "./appCoreAdapter";
import type { WorkerSessionSnapshotMarker } from "./renderSessionProjectionAccumulator";

type ProjectionListener = (landing: UiSessionProjectionLanding) => void;

export class WorkerSessionProjectionRouter {
  private readonly listeners = new Map<number, ProjectionListener>();
  private readonly pending = new Map<number, UiSessionProjectionLanding[]>();

  setListener(sessionId: number, listener: ProjectionListener | null): void {
    if (!listener) {
      this.listeners.delete(sessionId);
      this.pending.delete(sessionId);
      return;
    }
    this.listeners.set(sessionId, listener);
    const queued = this.pending.get(sessionId);
    this.pending.delete(sessionId);
    queued?.forEach(listener);
  }

  deliver(sessionId: number, landing: UiSessionProjectionLanding): void {
    const listener = this.listeners.get(sessionId);
    if (listener) {
      listener(landing);
      return;
    }
    const queued = this.pending.get(sessionId) ?? [];
    queued.push(landing);
    this.pending.set(sessionId, queued);
  }

  clear(): void {
    this.listeners.clear();
    this.pending.clear();
  }
}

export function workerSessionResultForTransport(result: unknown): unknown {
  return isUiSessionSnapshot(result)
    ? { __aerobagSessionRevision: result.session_revision } satisfies WorkerSessionSnapshotMarker
    : result;
}

function isUiSessionSnapshot(value: unknown): value is ReturnType<UiSession["initialSnapshot"]> {
  if (!value || typeof value !== "object" || Array.isArray(value)) return false;
  const candidate = value as Partial<UiSessionSnapshot>;
  return Number.isSafeInteger(candidate.session_revision)
    && typeof candidate.app_ui_state === "object"
    && typeof candidate.debug_state === "object";
}
