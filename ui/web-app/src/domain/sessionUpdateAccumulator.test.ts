// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { describe, expect, it } from "vitest";
import conformance from "../generated/sessionUpdateConformance.json";
import {
  SessionUpdateAccumulator,
  SessionUpdateContractError,
  SessionUpdateProjectionMismatchError,
} from "./sessionUpdateAccumulator";

describe("SessionUpdateAccumulator", () => {
  it("matches the shared session-update conformance sequence", () => {
    const accumulator = new SessionUpdateAccumulator(
      conformance.initial_snapshot,
      conformance.expected_contract_version,
    );
    for (const step of conformance.steps) {
      expect(accumulator.apply(step.update), step.name).toBe(step.disposition);
      expect(accumulator.snapshot, step.name).toEqual(step.expected_snapshot);
    }
  });

  it("rejects every invalid update in the shared conformance data", () => {
    for (const invalid of conformance.invalid_updates) {
      const accumulator = new SessionUpdateAccumulator(
        conformance.initial_snapshot,
        conformance.expected_contract_version,
      );
      expect(() => accumulator.apply(invalid.update), invalid.name).toThrow(SessionUpdateContractError);
    }
  });

  it("uses the transitional full snapshot only for an explicit revision-gap resync", () => {
    const accumulator = new SessionUpdateAccumulator(
      conformance.initial_snapshot,
      conformance.expected_contract_version,
    );
    const result = {
      ...conformance.initial_snapshot,
      session_revision: 9,
      map_layer_state: { nexrad: true },
      session_update: {
        ui_contract_version: 1,
        session_revision: 9,
        map: { version: 3, fields: { map_layer_state: { nexrad: true } } },
      },
    };
    expect(accumulator.applyTransitionalMutationSnapshot(result)).toBe("resync_required");
    expect(accumulator.snapshot).toEqual({
      ...conformance.initial_snapshot,
      session_revision: 9,
      map_layer_state: { nexrad: true },
    });
  });

  it("detects a core patch that does not reproduce its transitional full snapshot", () => {
    const accumulator = new SessionUpdateAccumulator(
      conformance.initial_snapshot,
      conformance.expected_contract_version,
    );
    expect(() => accumulator.applyTransitionalMutationSnapshot({
      ...conformance.initial_snapshot,
      session_revision: 8,
      map_layer_state: { nexrad: true },
      session_update: {
        ui_contract_version: 1,
        session_revision: 8,
      },
    })).toThrow(SessionUpdateProjectionMismatchError);
  });
});
