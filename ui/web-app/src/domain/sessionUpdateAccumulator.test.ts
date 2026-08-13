// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { describe, expect, it, vi } from "vitest";
import conformance from "../generated/sessionUpdateConformance.json";
import {
  SessionUpdateAccumulator,
  SessionUpdateContractError,
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

  it("leaves its snapshot untouched when a revision gap requires resynchronization", () => {
    const accumulator = new SessionUpdateAccumulator(
      conformance.initial_snapshot,
      conformance.expected_contract_version,
    );
    const update = {
      ui_contract_version: 1,
      session_revision: 9,
      map: { version: 3, fields: { map_layer_state: { nexrad: true } } },
    };
    expect(accumulator.apply(update)).toBe("resync_required");
    expect(accumulator.snapshot).toEqual(conformance.initial_snapshot);
  });

  it("loads and installs an explicit full snapshot after a revision gap", async () => {
    const accumulator = new SessionUpdateAccumulator(
      conformance.initial_snapshot,
      conformance.expected_contract_version,
    );
    const fullSnapshot = {
      ...conformance.initial_snapshot,
      session_revision: 9,
      map_layer_state: { nexrad: true },
    };
    const loadFullSnapshot = vi.fn(async () => fullSnapshot);

    await expect(accumulator.applyOrResync({
      ui_contract_version: 1,
      session_revision: 9,
      map: { version: 3, fields: { map_layer_state: { nexrad: true } } },
    }, loadFullSnapshot)).resolves.toBe("resync_required");
    expect(loadFullSnapshot).toHaveBeenCalledOnce();
    expect(accumulator.snapshot).toEqual(fullSnapshot);
  });
});
