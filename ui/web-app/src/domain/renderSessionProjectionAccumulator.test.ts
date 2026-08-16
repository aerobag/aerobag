// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { describe, expect, it } from "vitest";
import conformance from "../generated/sessionUpdateConformance.json";
import type { UiSessionSnapshot } from "./appCoreAdapter";
import { RenderSessionProjectionAccumulator } from "./renderSessionProjectionAccumulator";

const asSnapshot = (value: unknown) => value as UiSessionSnapshot;

describe("RenderSessionProjectionAccumulator", () => {
  it("lands narrow worker updates before accepting their response marker", () => {
    const accumulator = new RenderSessionProjectionAccumulator(
      asSnapshot(conformance.initial_snapshot),
    );
    const first = conformance.steps[0];

    accumulator.land({ kind: "update", value: first.update });

    expect(accumulator.complete({ __aerobagSessionRevision: 8 }))
      .toEqual(first.expected_snapshot);
  });

  it("rejects a command response whose projection was not delivered", () => {
    const accumulator = new RenderSessionProjectionAccumulator(
      asSnapshot(conformance.initial_snapshot),
    );

    expect(() => accumulator.complete({ __aerobagSessionRevision: 8 }))
      .toThrow("does not match landed revision 7");
  });

  it("requires an explicit full snapshot to recover a projection gap", () => {
    const accumulator = new RenderSessionProjectionAccumulator(
      asSnapshot(conformance.initial_snapshot),
    );
    const gapUpdate = {
      ui_contract_version: conformance.expected_contract_version,
      session_revision: 9,
      status: {
        version: 1,
        assignments: [{ path: ["debug_state"], value: { tile_labels: true } }],
      },
    };

    expect(() => accumulator.land({ kind: "update", value: gapUpdate }))
      .toThrow("revision gap without a full snapshot");

    const recovered = {
      ...conformance.initial_snapshot,
      session_revision: 9,
      debug_state: { tile_labels: true },
    };
    accumulator.land({ kind: "full_snapshot", value: asSnapshot(recovered) });
    expect(accumulator.complete({ __aerobagSessionRevision: 9 })).toEqual(recovered);
  });
});
