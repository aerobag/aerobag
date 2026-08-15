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
    if (!marker || marker.__aerobagSessionRevision !== this.currentSnapshot.session_revision) {
      throw new Error(
        `worker session response revision ${marker?.__aerobagSessionRevision ?? "missing"} `
          + `does not match landed revision ${this.currentSnapshot.session_revision}`,
      );
    }
    return this.currentSnapshot;
  }
}
