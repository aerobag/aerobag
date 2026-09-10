// @vitest-environment jsdom
// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { CloudPage, NavigationPageOptionsContext } from "./App";
import type { UiCloudAction, UiCloudPageState, CloudUiActionId } from "./generated/cloudWire";

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: Error) => void;
  const promise = new Promise<T>((done, fail) => { resolve = done; reject = fail; });
  return { promise, resolve, reject };
}

function action(id: CloudUiActionId, label: string): UiCloudAction {
  return { id, label, enabled: true, required_fields: [] };
}

function pageState(showCode: boolean): UiCloudPageState {
  return {
    title: "Cloud", summary: "", action_revision: showCode ? 2 : 1,
    sync_account_heading: "Sync Account", provider_heading: "Provider",
    overall_status_label: "Status",
    overall_status: { id: "overall_status", title: "Cloud active", state: "complete", actions: [], time_facts: [] },
    sync_account_panels: [{
      id: showCode ? "backup_code" : "linked", title: showCode ? "Back up Device Setup Code" : "Sync Account linked",
      state: "active", time_facts: [],
      actions: [action("backup_setup_code", "Back up Device Setup Code")],
      control: showCode ? {
        kind: "device_setup_code_output", setup_code: "AB3.test-only",
        qr_code: { accessibility_label: "Test setup code", quiet_zone_modules: 1, rows: ["1"] },
        copy_action: action("copy_setup_code", "Copy Device Setup Code"),
      } : null,
    }],
  };
}

describe("CloudPage async action feedback ownership", () => {
  let container: HTMLDivElement;
  let root: Root;
  let backup: ReturnType<typeof deferred<string | null>>;
  let copy: ReturnType<typeof deferred<string | null>>;
  let onAction: ReturnType<typeof vi.fn<(id: CloudUiActionId) => Promise<string | null>>>;

  beforeEach(() => {
    vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
    // Render the production component, not App: no worker, fixtures, server, or
    // clipboard is needed to control the async action-completion boundary.
    vi.stubGlobal("fetch", vi.fn(() => { throw new Error("component test must not fetch"); }));
    container = document.createElement("div");
    document.body.appendChild(container);
    root = createRoot(container);
    backup = deferred<string | null>();
    copy = deferred<string | null>();
    onAction = vi.fn((id: CloudUiActionId) => {
      if (id === "backup_setup_code") return backup.promise;
      if (id === "copy_setup_code") return copy.promise;
      throw new Error(`unexpected cloud action: ${id}`);
    });
  });

  afterEach(async () => {
    await act(async () => { root.unmount(); });
    container.remove();
    expect(fetch).not.toHaveBeenCalled();
    vi.unstubAllGlobals();
  });

  async function render(showCode: boolean) {
    await act(async () => {
      root.render(<NavigationPageOptionsContext.Provider value={{
        options: [
          { id: "home", label: "Home", launcherLabel: "HOME" },
          { id: "map", label: "Chart", launcherLabel: "CHART" },
        ],
        maxHistoryDepth: 2, chartOrPlateReturnPages: new Set(["map", "charts"]),
        defaultChartOrPlateReturnPage: "map",
      }}>
        <CloudPage page="cloud" state={pageState(showCode)} navElement={null}
          mostRecentChartOrPlatePage="map" onOpenPlan={() => {}} onOpenRecentChartOrPlate={() => {}}
          onSelectPage={() => {}} onAction={onAction} />
      </NavigationPageOptionsContext.Provider>);
    });
  }

  async function click(id: CloudUiActionId) {
    const button = container.querySelector<HTMLButtonElement>(`[data-testid="cloud-action-${id}"]`);
    expect(button, `${id} must be rendered`).not.toBeNull();
    await act(async () => { button!.click(); });
  }

  const status = () => container.querySelector('[data-testid="cloud-copy-status"]')?.textContent ?? "";
  const error = () => container.querySelector('[role="alert"]')?.textContent ?? "";

  async function startOverlappingActions() {
    await render(false);
    await click("backup_setup_code");
    // The core snapshot exposes the next panel before the older action's
    // provider work completes. Keep the real CloudPage mounted across this update.
    await render(true);
    await click("copy_setup_code");
    expect(onAction.mock.calls.map(([id]) => id)).toEqual(["backup_setup_code", "copy_setup_code"]);
  }

  it("preserves Copied when an older non-copy action completes afterward", async () => {
    await startOverlappingActions();
    await act(async () => { copy.resolve("Copied"); });
    expect(status()).toBe("Copied");

    await act(async () => { backup.resolve(null); });
    expect(status()).toBe("Copied");
    expect(error()).toBe("");
  });

  it("does not show an older action's late error over a newer success", async () => {
    await startOverlappingActions();
    await act(async () => { copy.resolve("Copied"); });
    expect(status()).toBe("Copied");

    await act(async () => { backup.reject(new Error("obsolete backup failure")); });
    expect(status()).toBe("Copied");
    expect(error()).toBe("");
  });

  it("also works when the older action completes before Copy", async () => {
    await startOverlappingActions();
    await act(async () => { backup.resolve(null); });
    expect(status()).toBe("");
    await act(async () => { copy.resolve("Copied"); });
    expect(status()).toBe("Copied");
    expect(error()).toBe("");
  });

  it("still shows a failure belonging to the latest action", async () => {
    await startOverlappingActions();
    await act(async () => { backup.resolve(null); });
    await act(async () => { copy.reject(new Error("clipboard denied")); });
    expect(status()).toBe("");
    expect(error()).toBe("clipboard denied");
  });

  it("does not resurrect a stale copy label while a newer action is pending", async () => {
    await render(true);
    await click("copy_setup_code");
    await click("backup_setup_code");
    await act(async () => { copy.resolve("Copied"); });
    expect(status()).toBe("");
    expect(error()).toBe("");
    await act(async () => { backup.resolve(null); });
    expect(status()).toBe("");
  });

  it("does not show an older success alongside the latest action's failure", async () => {
    await render(true);
    await click("copy_setup_code");
    await click("backup_setup_code");
    await act(async () => { backup.reject(new Error("backup denied")); });
    await act(async () => { copy.resolve("Copied"); });
    expect(status()).toBe("");
    expect(error()).toBe("backup denied");
  });
});
