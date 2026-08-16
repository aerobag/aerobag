// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { describe, expect, it, vi } from "vitest";
import {
  UiSessionWorkCancelledError,
  WebUiSessionWorkRunner,
  type UiSessionWorkSchedulerBridge,
} from "./uiSessionWorkRunner";

type Request = {
  id: number;
  kind: string;
  coalesce_key: string | null;
  requested_at_ms: number;
};

class FakeCoreSchedulerBridge implements UiSessionWorkSchedulerBridge {
  active: Request | null = null;
  readonly pending = new Map<string, Request>();
  destroyCount = 0;

  create(): number {
    return 7;
  }

  request(_handle: number, requestJson: string): string {
    const request = JSON.parse(requestJson) as Request;
    if (!this.active) {
      this.active = request;
      return JSON.stringify({ kind: "start", request });
    }
    const key = request.coalesce_key ?? request.kind;
    const replaced = this.pending.get(key);
    this.pending.set(key, request);
    return JSON.stringify({
      kind: "queued",
      replaced_request_id: replaced?.id ?? null,
    });
  }

  complete(_handle: number, requestId: number): string {
    if (this.active?.id !== requestId) {
      throw new Error(`request ${requestId} is not active`);
    }
    const next = [...this.pending.values()]
      .sort((left, right) => left.requested_at_ms - right.requested_at_ms || left.id - right.id)[0] ?? null;
    if (next) {
      this.pending.delete(next.coalesce_key ?? next.kind);
    }
    this.active = next;
    return JSON.stringify({
      result_action: { kind: "land" },
      next,
    });
  }

  destroy(): void {
    this.destroyCount += 1;
    this.active = null;
    this.pending.clear();
  }
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

describe("WebUiSessionWorkRunner", () => {
  it("drops replaced pending work and runs core's selected successor", async () => {
    const bridge = new FakeCoreSchedulerBridge();
    const runner = await WebUiSessionWorkRunner.create(bridge);
    const firstGate = deferred<void>();
    const operations: string[] = [];
    const first = runner.run("map_overlay", "map_overlay", async () => {
      operations.push("first");
      await firstGate.promise;
      return "first-result";
    });
    await vi.waitFor(() => expect(operations).toEqual(["first"]));

    const replaced = runner.run("terrain_overlay", "terrain_overlay", () => {
      operations.push("replaced");
      return "replaced-result";
    });
    const replacement = runner.run("terrain_overlay", "terrain_overlay", () => {
      operations.push("replacement");
      return "replacement-result";
    });
    const replacedAssertion = expect(replaced).rejects.toMatchObject({
      name: "UiSessionWorkCancelledError",
      reason: "replaced_by_newer_pending",
    });

    firstGate.resolve();
    await expect(first).resolves.toBe("first-result");
    await replacedAssertion;
    await expect(replacement).resolves.toBe("replacement-result");
    expect(operations).toEqual(["first", "replacement"]);
    await runner.close();
  });

  it("completes failed work in core so the pending queue keeps draining", async () => {
    const bridge = new FakeCoreSchedulerBridge();
    const runner = await WebUiSessionWorkRunner.create(bridge);
    const firstGate = deferred<void>();
    const first = runner.run("map_overlay", "map_overlay", async () => {
      await firstGate.promise;
      throw new Error("overlay failed");
    });
    const second = runner.run("nexrad_overlay", "nexrad_overlay", () => "nexrad-result");
    const firstAssertion = expect(first).rejects.toThrow("overlay failed");

    firstGate.resolve();
    await firstAssertion;
    await expect(second).resolves.toBe("nexrad-result");
    await runner.close();
  });

  it("rejects retained work and destroys the core scheduler exactly once", async () => {
    const bridge = new FakeCoreSchedulerBridge();
    const runner = await WebUiSessionWorkRunner.create(bridge);
    const activeGate = deferred<string>();
    const active = runner.run("chart_asset", "chart_asset:asset:one", () => activeGate.promise);
    const pending = runner.run("chart_asset", "chart_asset:thumbnail:two", () => "unused");
    const activeAssertion = expect(active).rejects.toBeInstanceOf(UiSessionWorkCancelledError);
    const pendingAssertion = expect(pending).rejects.toBeInstanceOf(UiSessionWorkCancelledError);

    await runner.close();
    await runner.close();
    await activeAssertion;
    await pendingAssertion;
    activeGate.resolve("late");
    expect(bridge.destroyCount).toBe(1);
  });

  it("fails retained callers if the core decision contract is malformed", async () => {
    const bridge = new FakeCoreSchedulerBridge();
    bridge.request = () => "{not-json";
    const runner = await WebUiSessionWorkRunner.create(bridge);

    await expect(runner.run("map_overlay", "map_overlay", () => "unused"))
      .rejects.toBeInstanceOf(SyntaxError);
    await vi.waitFor(() => expect(bridge.destroyCount).toBe(1));
  });
});
