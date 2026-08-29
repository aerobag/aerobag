// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { describe, expect, it, vi } from "vitest";
import type { UiSessionProjectionPublication, UiSessionSnapshot } from "./appCoreAdapter";
import {
  CLOUD_EFFECT_SESSION_UPDATE_GROUPS,
  HIGH_RATE_SESSION_UPDATE_GROUPS,
  SHELL_SESSION_UPDATE_GROUPS,
  SessionRenderStore,
  RenderValueStore,
} from "./sessionRenderStore";

function snapshot(revision: number): UiSessionSnapshot {
  return { session_revision: revision } as UiSessionSnapshot;
}

function update(
  revision: number,
  changedGroups: UiSessionProjectionPublication["changedGroups"],
): UiSessionProjectionPublication {
  return {
    landing: { kind: "update", value: {} },
    snapshot: snapshot(revision),
    changedGroups,
    fullSnapshot: false,
  };
}

describe("SessionRenderStore", () => {
  it("routes high-rate ownship updates without notifying the application shell", () => {
    const store = new SessionRenderStore(snapshot(0));
    const shell = vi.fn();
    const highRate = vi.fn();
    store.subscribe(SHELL_SESSION_UPDATE_GROUPS, shell);
    store.subscribe(HIGH_RATE_SESSION_UPDATE_GROUPS, highRate);

    store.publish(update(1, ["ownship", "situation"]));

    expect(store.snapshot.session_revision).toBe(1);
    expect(highRate).toHaveBeenCalledOnce();
    expect(shell).not.toHaveBeenCalled();
  });

  it("notifies both owners when one publication spans both scopes", () => {
    const store = new SessionRenderStore(snapshot(0));
    const shell = vi.fn();
    const highRate = vi.fn();
    store.subscribe(SHELL_SESSION_UPDATE_GROUPS, shell);
    store.subscribe(HIGH_RATE_SESSION_UPDATE_GROUPS, highRate);

    store.publish(update(1, ["ownship", "status"]));

    expect(shell).toHaveBeenCalledOnce();
    expect(highRate).toHaveBeenCalledOnce();
  });

  it("lands revision-only updates without notifying either render owner", () => {
    const store = new SessionRenderStore(snapshot(0));
    const listener = vi.fn();
    store.subscribe(SHELL_SESSION_UPDATE_GROUPS, listener);
    store.subscribe(HIGH_RATE_SESSION_UPDATE_GROUPS, listener);

    store.publish(update(1, []));

    expect(store.snapshot.session_revision).toBe(1);
    expect(listener).not.toHaveBeenCalled();
  });

  it("wakes cloud effects directly from the core cloud publication", () => {
    const store = new SessionRenderStore(snapshot(0));
    const wakeProvider = vi.fn();
    store.subscribe(CLOUD_EFFECT_SESSION_UPDATE_GROUPS, wakeProvider);

    store.publish(update(1, ["flight_plan", "cloud"]));

    expect(wakeProvider).toHaveBeenCalledOnce();
  });
});

describe("RenderValueStore", () => {
  it("publishes page-owned values without requiring parent React state", () => {
    const store = new RenderValueStore(1);
    const listener = vi.fn();
    store.subscribe(listener);

    store.publish(2);
    store.publish(2);

    expect(store.value).toBe(2);
    expect(listener).toHaveBeenCalledOnce();
  });
});
