// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later
import { expect, test, vi } from "vitest";
import { BrowserGeolocationWatch } from "./browserGeolocationWatch";

test("pause stops the receiver and rejects queued callbacks; resume starts one fresh watch", () => {
  const callbacks: Array<[PositionCallback, PositionErrorCallback | null | undefined]> = [];
  const geo = { watchPosition: vi.fn((position: PositionCallback, error?: PositionErrorCallback | null) => {
    callbacks.push([position,error]); return callbacks.length;
  }), clearWatch: vi.fn() };
  const position = vi.fn(), error = vi.fn(), started = vi.fn();
  const owner = new BrowserGeolocationWatch(geo, position, error, started);
  owner.setPaused(true); expect(geo.watchPosition).not.toHaveBeenCalled();
  owner.setPaused(false); owner.setPaused(false); expect(geo.watchPosition).toHaveBeenCalledTimes(1);
  owner.setPaused(true); expect(geo.clearWatch).toHaveBeenCalledWith(1);
  callbacks[0][0]({} as GeolocationPosition); callbacks[0][1]?.({} as GeolocationPositionError);
  expect(position).not.toHaveBeenCalled(); expect(error).not.toHaveBeenCalled();
  owner.setPaused(false); callbacks[1][0]({} as GeolocationPosition); expect(position).toHaveBeenCalledTimes(1);
  owner.dispose(); expect(geo.clearWatch).toHaveBeenLastCalledWith(2);
  callbacks[1][1]?.({} as GeolocationPositionError); owner.setPaused(false);
  expect(error).not.toHaveBeenCalled(); expect(started).toHaveBeenCalledTimes(2);
});
