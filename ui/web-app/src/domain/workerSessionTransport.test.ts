// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { describe, expect, it } from "vitest";
import conformance from "../generated/sessionUpdateConformance.json";
import {
  WorkerSessionProjectionRouter,
  workerSessionResultForTransport,
} from "./workerSessionTransport";

describe("workerSessionResultForTransport", () => {
  it("replaces session snapshots with lightweight revision markers", () => {
    expect(workerSessionResultForTransport(conformance.initial_snapshot)).toEqual({
      __aerobagSessionRevision: 7,
    });
  });

  it("leaves non-snapshot query results untouched", () => {
    const queryResult = { visible_features: [], debug_state: {} };
    expect(workerSessionResultForTransport(queryResult)).toBe(queryResult);
  });

  it("delivers projections queued before session facade construction in order", () => {
    const router = new WorkerSessionProjectionRouter();
    const received: unknown[] = [];
    const first = { kind: "update", value: conformance.steps[0].update } as const;
    const second = { kind: "update", value: { sequence: 2 } } as const;

    router.deliver(12, first);
    router.deliver(12, second);
    router.setListener(12, (landing) => received.push(landing));

    expect(received).toEqual([first, second]);
  });
});
