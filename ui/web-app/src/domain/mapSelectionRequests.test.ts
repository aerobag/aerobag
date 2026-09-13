// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { describe, expect, it, vi } from "vitest";
import { MapSelectionRequests } from "./mapSelectionRequests";

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: Error) => void;
  const promise = new Promise<T>((done, fail) => { resolve = done; reject = fail; });
  return { promise, resolve, reject };
}

describe("map selection request ownership (controlled completion, no timers)", () => {
  for (const kind of ["point", "nav-ref"] as const) {
    it(`${kind}: viewport movement invalidates only a frame-bound hit test`, async () => {
      const requests = new MapSelectionRequests();
      const lookup = deferred<string>();
      const apply = vi.fn();
      const fail = vi.fn();
      const result = requests.run(kind, () => lookup.promise, apply, fail);
      requests.viewportChanged(); // automatic ownship-follow, before worker reply
      lookup.resolve("KSEA");
      await result;
      expect(apply.mock.calls).toEqual(kind === "nav-ref" ? [["KSEA"]] : []);
      expect(fail).not.toHaveBeenCalled();
    });

    for (const newerKind of ["point", "nav-ref"] as const) {
      for (const staleError of [false, true]) {
        it(`${newerKind} supersedes ${kind}, including its late ${staleError ? "error" : "success"}`, async () => {
          const requests = new MapSelectionRequests();
          const older = deferred<string>();
          const newer = deferred<string>();
          const apply = vi.fn();
          const fail = vi.fn();
          const first = requests.run(kind, () => older.promise, apply, fail);
          const second = requests.run(newerKind, () => newer.promise, apply, fail);
          newer.resolve("KPAE");
          await second;
          if (staleError) older.reject(new Error("old failure"));
          else older.resolve("KSEA");
          await first;
          expect(apply.mock.calls).toEqual([["KPAE"]]);
          expect(fail).not.toHaveBeenCalled();
        });
      }
    }

    for (const staleError of [false, true]) {
      it(`${kind}: cancellation suppresses late ${staleError ? "errors" : "success and search clearing"}`, async () => {
        const requests = new MapSelectionRequests();
        const lookup = deferred<string>();
        const apply = vi.fn();
        const fail = vi.fn();
        const result = requests.run(kind, () => lookup.promise, apply, fail);
        requests.cancel(); // gesture, edited query, dismissal, mode/session/page change or unmount
        if (staleError) lookup.reject(new Error("old failure"));
        else lookup.resolve("KSEA");
        await result;
        expect(apply).not.toHaveBeenCalled();
        expect(fail).not.toHaveBeenCalled();
      });
    }
  }

  it("does not start a stale identifier's inspection after a newer suggestion was chosen", async () => {
    const requests = new MapSelectionRequests();
    const identifier = deferred<string>();
    const inspect = vi.fn(async (id: string) => id);
    const apply = vi.fn();
    const fail = vi.fn();
    const older = requests.run("nav-ref", async (isCurrent) => {
      const id = await identifier.promise;
      return isCurrent() ? inspect(id) : null;
    }, apply, fail);
    await requests.run("nav-ref", () => inspect("KPAE"), apply, fail);
    identifier.resolve("KSEA");
    await older;
    expect(inspect.mock.calls).toEqual([["KPAE"]]);
    expect(apply.mock.calls).toEqual([["KPAE"]]);
    expect(fail).not.toHaveBeenCalled();
  });

  it("does not land an older result even while the newer request is still pending", async () => {
    const requests = new MapSelectionRequests();
    const older = deferred<string>();
    const newer = deferred<string>();
    const apply = vi.fn();
    const fail = vi.fn();
    const first = requests.run("nav-ref", () => older.promise, apply, fail);
    const second = requests.run("nav-ref", () => newer.promise, apply, fail);
    older.resolve("KSEA");
    await first;
    expect(apply).not.toHaveBeenCalled();
    newer.resolve("KPAE");
    await second;
    expect(apply.mock.calls).toEqual([["KPAE"]]);
    expect(fail).not.toHaveBeenCalled();
  });

  it("reports a current failure even after automatic follow updates", async () => {
    const requests = new MapSelectionRequests();
    const lookup = deferred<string>();
    const apply = vi.fn();
    const fail = vi.fn();
    const result = requests.run("nav-ref", () => lookup.promise, apply, fail);
    requests.viewportChanged();
    const error = new Error("worker lookup failed");
    lookup.reject(error);
    await result;
    expect(apply).not.toHaveBeenCalled();
    expect(fail).toHaveBeenCalledExactlyOnceWith(error);
  });
});
