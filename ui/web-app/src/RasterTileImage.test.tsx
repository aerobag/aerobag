// @vitest-environment jsdom
// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import { act, StrictMode } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { RasterTileImage } from "./RasterTileImage";

describe("raster image request lifecycle", () => {
  let container: HTMLDivElement;
  let root: Root;
  const callbacks = { onLoaded: vi.fn(), onFailed: vi.fn(), onError: vi.fn(), onRecovery: vi.fn() };
  beforeEach(() => {
    vi.useFakeTimers();
    vi.clearAllMocks();
    vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
    container = document.createElement("div");
    document.body.append(container);
    root = createRoot(container);
  });
  afterEach(async () => {
    await act(async () => root.unmount());
    container.remove();
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });
  async function render(src = "/tile.webp") {
    await act(async () => root.render(<StrictMode><RasterTileImage src={src} {...callbacks} /></StrictMode>));
  }
  const images = () => [...container.querySelectorAll("img")];
  const advance = async (ms: number) => act(async () => { vi.advanceTimersByTime(ms); });
  const finish = async (image: HTMLImageElement, event: "load" | "error") => act(async () => { image.dispatchEvent(new Event(event)); });

  it("keeps the original image through arbitrarily slow successful loads", async () => {
    await render();
    const original = images()[0];
    await advance(60_000);
    expect(images()).toHaveLength(2);
    expect(images()[0]).toBe(original);
    expect(original.getAttribute("src")).toBe("/tile.webp");
    expect(original.getAttribute("fetchpriority")).toBe("high");
    expect(images()[1].getAttribute("fetchpriority")).toBe("low");
    expect(callbacks.onFailed).not.toHaveBeenCalled();
    expect(callbacks.onRecovery).toHaveBeenCalledTimes(1);
    await finish(original, "load");
    expect(images()).toEqual([original]);
    expect(callbacks.onLoaded).toHaveBeenCalledTimes(1);
  });

  it("lets the recovery request win when the original never responds", async () => {
    await render();
    await advance(1_500);
    const recovery = images()[1];
    expect(recovery.getAttribute("src")).toBe("/tile.webp?aerobag_retry=1");
    await finish(recovery, "load");
    await advance(60_000);
    expect(images()).toEqual([recovery]);
    expect(callbacks.onLoaded).toHaveBeenCalledTimes(1);
    expect(callbacks.onFailed).not.toHaveBeenCalled();
  });

  it.each([0, 1])("does not fail when attempt %i errors while its sibling is pending", async (failedAttempt) => {
    await render();
    await advance(1_500);
    const attempts = images();
    await finish(attempts[failedAttempt], "error");
    await advance(60_000);
    expect(images()).toEqual([attempts[1 - failedAttempt]]);
    expect(callbacks.onFailed).not.toHaveBeenCalled();
    await finish(attempts[1 - failedAttempt], "load");
    expect(callbacks.onLoaded).toHaveBeenCalledTimes(1);
  });

  it("fails only after both requests error, without an unbounded retry loop", async () => {
    await render();
    await finish(images()[0], "error");
    expect(callbacks.onRecovery).toHaveBeenCalledWith(1, "error");
    await advance(60_000);
    expect(images()).toHaveLength(1);
    expect(callbacks.onFailed).not.toHaveBeenCalled();
    await finish(images()[0], "error");
    await advance(60_000);
    expect(images()).toHaveLength(0);
    expect(callbacks.onFailed).toHaveBeenCalledTimes(1);
    expect(callbacks.onRecovery).toHaveBeenCalledTimes(1);
  });

  it("does not retry an image that has already loaded or report it twice", async () => {
    await render();
    const image = images()[0];
    await finish(image, "load");
    await finish(image, "load");
    await advance(60_000);
    expect(images()).toEqual([image]);
    expect(callbacks.onLoaded).toHaveBeenCalledTimes(1);
    expect(callbacks.onRecovery).not.toHaveBeenCalled();
  });

  it("does not restart pending requests or timers on parent rerenders", async () => {
    await render();
    const original = images()[0];
    await advance(1_000);
    await render();
    await advance(500);
    expect(images()).toHaveLength(2);
    expect(images()[0]).toBe(original);
  });

  it("starts a fresh lifecycle for a new immutable resource", async () => {
    await render();
    await advance(1_500);
    const old = images()[0];
    await render("/new-tile.webp");
    expect(images()).toHaveLength(1);
    expect(images()[0]).not.toBe(old);
    expect(images()[0].getAttribute("src")).toBe("/new-tile.webp");
    await advance(1_500);
    expect(images()).toHaveLength(2);
  });

  it("cancels its recovery timer when removed from the viewport", async () => {
    await render();
    await act(async () => root.render(null));
    await advance(60_000);
    expect(callbacks.onRecovery).not.toHaveBeenCalled();
  });
});
