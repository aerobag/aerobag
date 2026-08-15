// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { describe, expect, it } from "vitest";
import { shouldLandCompletedCoalescedWork } from "./coalescedViewportWork";

type TestRequest = {
  id: number;
  session: string;
  navDataEpoch: number;
  altitudeBucket: number;
};

function request(id: number, overrides: Partial<TestRequest> = {}): TestRequest {
  return {
    id,
    session: "session-a",
    navDataEpoch: 4,
    altitudeBucket: 2_000,
    ...overrides,
  };
}

function compatible(completed: TestRequest, latest: TestRequest): boolean {
  return completed.session === latest.session
    && completed.navDataEpoch === latest.navDataEpoch
    && completed.altitudeBucket === latest.altitudeBucket;
}

describe("coalesced viewport work", () => {
  it("lands useful progress under continuous latest-only request churn", () => {
    let active: TestRequest | null = null;
    let pending: TestRequest | null = null;
    let lastLandedId = 0;
    const landedIds: number[] = [];

    const submit = (next: TestRequest) => {
      if (active === null) {
        active = next;
      } else {
        pending = next;
      }
    };
    const completeActive = () => {
      expect(active).not.toBeNull();
      const completed = active!;
      const latest = pending ?? completed;
      if (shouldLandCompletedCoalescedWork(completed, latest, lastLandedId, compatible)) {
        landedIds.push(completed.id);
        lastLandedId = completed.id;
      }
      active = pending;
      pending = null;
    };

    submit(request(1));
    for (let id = 2; id <= 10; id += 1) {
      submit(request(id));
    }
    completeActive();

    for (let id = 11; id <= 20; id += 1) {
      submit(request(id));
    }
    completeActive();
    completeActive();

    expect(landedIds).toEqual([1, 10, 20]);
  });

  it("drops completed work from an incompatible state or an already-landed generation", () => {
    expect(shouldLandCompletedCoalescedWork(
      request(3),
      request(4, { altitudeBucket: 2_200 }),
      0,
      compatible,
    )).toBe(false);
    expect(shouldLandCompletedCoalescedWork(
      request(3),
      request(4, { navDataEpoch: 5 }),
      0,
      compatible,
    )).toBe(false);
    expect(shouldLandCompletedCoalescedWork(request(3), request(4), 3, compatible)).toBe(false);
  });
});
