// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { describe, expect, it, vi } from "vitest";
import { CoalescedAsyncRunner } from "./coalescedAsyncRunner";

describe("CoalescedAsyncRunner", () => {
  it("drains a request that arrives while prior work is in flight", async () => {
    let releaseFirst!: () => void;
    const firstBlocked = new Promise<void>((resolve) => { releaseFirst = resolve; });
    const work = vi.fn()
      .mockImplementationOnce(() => firstBlocked)
      .mockResolvedValue(undefined);
    const runner = new CoalescedAsyncRunner(work);

    const first = runner.request();
    const second = runner.request();
    expect(work).toHaveBeenCalledOnce();

    releaseFirst();
    await Promise.all([first, second]);

    expect(work).toHaveBeenCalledTimes(2);
  });

  it("coalesces multiple requests during one run into one follow-up drain", async () => {
    let releaseFirst!: () => void;
    const firstBlocked = new Promise<void>((resolve) => { releaseFirst = resolve; });
    const work = vi.fn()
      .mockImplementationOnce(() => firstBlocked)
      .mockResolvedValue(undefined);
    const runner = new CoalescedAsyncRunner(work);

    const first = runner.request();
    const second = runner.request();
    const third = runner.request();
    releaseFirst();
    await Promise.all([first, second, third]);

    expect(work).toHaveBeenCalledTimes(2);
  });

  it("reports idle only after automatically queued follow-up work drains", async () => {
    let releaseFirst!: () => void;
    const firstBlocked = new Promise<void>((resolve) => { releaseFirst = resolve; });
    let runner!: CoalescedAsyncRunner;
    const work = vi.fn()
      .mockImplementationOnce(async () => {
        await firstBlocked;
        void runner.request();
      })
      .mockResolvedValue(undefined);
    runner = new CoalescedAsyncRunner(work);

    void runner.request();
    let idle = false;
    const settled = runner.whenIdle().then(() => { idle = true; });
    await Promise.resolve();
    expect(idle).toBe(false);

    releaseFirst();
    await settled;
    expect(work).toHaveBeenCalledTimes(2);
    expect(idle).toBe(true);
  });
});
