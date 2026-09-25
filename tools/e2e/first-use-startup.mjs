// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { E2E_TIMING, TerminalObservationError } from "./transition-contract.mjs";
import { semanticProjectionFields } from "./android-harness.mjs";

// Main journeys and independent browser peers must settle the same first-use
// lifecycle. A map behind the introduction is not an interactive landing page.
export async function readStartupState(runtime) {
  const fatal = runtime.platform === "web"
    ? await runtime.driver.readElement("startup-fatal-error")
    : null;
  if (fatal) {
    throw new TerminalObservationError("application startup failed", fatal.text || "unknown failure");
  }
  const entries = await runtime.driver.readProjection("parity:startup-state:");
  const id = entries[0]?.id ?? "";
  const prefix = "parity:startup-state:";
  const fields = id.startsWith(prefix) ? semanticProjectionFields(id.slice(prefix.length)) : null;
  return fields?.ready === "true" ? fields : null;
}

export async function startupState(runtime, timeoutMs = E2E_TIMING.startupMs) {
  return runtime.eventually("operational startup state", () => readStartupState(runtime), timeoutMs);
}

export function readGuidedTourPanel(runtime) {
  // Both platforms index the rendered panel. Absence is an ordinary result,
  // not a reason to walk Android's entire accessibility hierarchy.
  return runtime.driver.readElement("guided-tour-panel");
}

export async function acceptDisclaimer(runtime, { required = false, keepIntroduction = false } = {}) {
  const initial = await startupState(runtime);
  const accepted = initial.disclaimer_required === "true";
  if (!accepted && required) {
    throw new Error("fresh profile reached the map without presenting the disclaimer");
  }
  if (accepted) {
    await runtime.action("accept mandatory disclaimer", "disclaimer-accept-button", {
      complete: async () => {
        const state = await readStartupState(runtime);
        return state?.disclaimer_required === "false" ? state : null;
      },
    });
    const completed = await startupState(runtime);
    if (completed.disclaimer_required !== "false") {
      throw new Error("application startup retained mandatory disclaimer after acceptance");
    }
  }
  if (!keepIntroduction) {
    await runtime.eventually("first-use introduction settled", async () => {
      const state = await readStartupState(runtime);
      return state && state.tour_pending !== "true" ? state : null;
    });
    // A successful read of null proves absence; an unavailable provider does
    // not. Keep transport recovery inside the bounded observation contract.
    const { panel } = await runtime.eventually("first-use introduction visibility", async () => ({
      panel: await readGuidedTourPanel(runtime),
    }));
    if (panel) {
      await runtime.action("close first-use tour", "guided-tour-close", {
        complete: async () => !(await readGuidedTourPanel(runtime)),
      });
      // Closing the introduction lands on Home. Existing journeys start on Map.
      await runtime.openPage("map");
    }
  }
  return accepted;
}
