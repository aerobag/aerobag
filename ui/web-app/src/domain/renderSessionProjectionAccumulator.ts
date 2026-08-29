// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import type {
  UiSessionProjectionLanding,
  UiSessionSnapshot,
} from "./appCoreAdapter";
import { UI_SESSION_PAGE_CONTRACTS_WIRE_VERSION } from "./appCoreAdapter";
import { SessionUpdateAccumulator } from "./sessionUpdateAccumulator";

export type WorkerSessionSnapshotMarker = {
  __aerobagSessionRevision: number;
};

export class RenderSessionProjectionAccumulator {
  private readonly accumulator: SessionUpdateAccumulator;
  private currentSnapshot: UiSessionSnapshot;

  constructor(initialSnapshot: UiSessionSnapshot) {
    this.accumulator = new SessionUpdateAccumulator(
      initialSnapshot,
      UI_SESSION_PAGE_CONTRACTS_WIRE_VERSION,
    );
    this.currentSnapshot = initialSnapshot;
  }

  get snapshot(): UiSessionSnapshot {
    return this.currentSnapshot;
  }

  land(landing: UiSessionProjectionLanding): UiSessionSnapshot {
    if (landing.kind === "full_snapshot") {
      this.accumulator.replaceFullSnapshot(landing.value);
    } else {
      const disposition = this.accumulator.apply(landing.value);
      if (disposition === "resync_required") {
        throw new Error("worker projection stream has a revision gap without a full snapshot");
      }
    }
    this.currentSnapshot = this.accumulator.snapshot as UiSessionSnapshot;
    return this.currentSnapshot;
  }

  complete(marker: WorkerSessionSnapshotMarker): UiSessionSnapshot {
    const responseRevision = marker?.__aerobagSessionRevision;
    const landedRevision = this.currentSnapshot.session_revision;
    if (!Number.isSafeInteger(responseRevision) || responseRevision > landedRevision) {
      throw new Error(
        `worker session response revision ${responseRevision ?? "missing"} `
          + `has not landed; current revision is ${landedRevision}`,
      );
    }
    // Concurrent session calls can commit in order but deliver the later call's
    // response first. Its projection is already the newest coherent view, so an
    // overtaken response must return that view rather than abort its caller.
    return this.currentSnapshot;
  }
}
