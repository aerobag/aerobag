// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import {
  createJourneyResult, finishJourneyResult, recordJourneyCheck,
  recordJourneyStep, validateJourneyResult,
} from "./journey-result.mjs";
import {
  editSemanticText, inspectSemanticMapAt, navigateSemanticPage,
} from "./semantic-journey-driver.mjs";
import {
  E2E_TIMING, observeUntil, observeValueUntilStable, performTransition,
} from "./transition-contract.mjs";

export { E2E_TIMING } from "./transition-contract.mjs";

export function releaseJourneyFixtureUrl(platform, relativePath, fixtureOrigin = null) {
  if (/^https?:\/\//.test(relativePath)) return relativePath;
  const normalized = String(relativePath).replace(/^\/+/, "");
  if (platform === "android") {
    return `/releasejourney/${Buffer.from(normalized, "utf8").toString("hex")}`;
  }
  const path = `/release-journey/${normalized}`;
  return fixtureOrigin ? new URL(path, fixtureOrigin).href : path;
}

export function persistJourneyResult(result, artifactDir) {
  const resultPath = join(artifactDir, "result.json");
  writeFileSync(resultPath, `${JSON.stringify(result, null, 2)}\n`);
}

export function summarizeFixtureRequests(requests) {
  if (!Array.isArray(requests)) {
    return { count: 0, outcomes: {}, statuses: {}, anomalies: [], tail: [] };
  }
  const outcomes = {};
  const statuses = {};
  const anomalies = [];
  for (const request of requests) {
    const outcome = String(request?.outcome ?? "unknown");
    const status = request?.status == null ? "pending" : String(request.status);
    outcomes[outcome] = (outcomes[outcome] ?? 0) + 1;
    statuses[status] = (statuses[status] ?? 0) + 1;
    const isExpectedStream = request?.url === "/live-feeds/v3/events" ||
      String(request?.url ?? "").includes("/cloud/v1/watch");
    if ((!isExpectedStream && outcome !== "finished") ||
      (Number.isInteger(request?.status) && request.status >= 400)) {
      anomalies.push(request);
    }
  }
  return {
    count: requests.length,
    outcomes,
    statuses,
    anomalies: anomalies.slice(-50),
    tail: requests.slice(-100),
  };
}

export function semanticOptionSelected(option) {
  return option?.checked === true || option?.selected === true || option?.pressed === "true";
}

export function createJourneyRuntime({
  journey, platform, driver, fixture, fixtureOrigin = null, artifactDir, build = null,
}) {
  const result = createJourneyResult({
    id: journey.id,
    platform,
    fixture: fixture ? { id: fixture.fixture, schema_version: fixture.schema_version } : null,
    build,
  });
  const expectedAssertions = new Set(journey.assertions);
  const completedAssertions = new Set();
  const repeatableActionHandles = new WeakSet();
  mkdirSync(artifactDir, { recursive: true });

  const runRecordedPhase = async (phaseId, operation, detail = undefined) => {
    const startedAt = performance.now();
    console.log(`[${journey.id}:${platform}] phase start: ${phaseId}`);
    const value = await operation();
    const durationMs = Math.round(performance.now() - startedAt);
    console.log(`[${journey.id}:${platform}] phase pass: ${phaseId} (${durationMs}ms)`);
    recordJourneyStep(result, phaseId, detail, durationMs);
    return value;
  };

  const runtime = {
    journey,
    platform,
    driver,
    fixture,
    fixtureOrigin,
    result,
    artifactDir,

    capability(path) {
      const value = path.split(".").reduce((current, component) => current?.[component], fixture?.capabilities);
      if (value === undefined) throw new Error(`fixture capability ${path} is unavailable`);
      return value;
    },

    fixtureUrl(relativePath) {
      return releaseJourneyFixtureUrl(platform, relativePath, fixtureOrigin);
    },

    async reset(phaseId = "app.reset") {
      return runRecordedPhase(phaseId, () => driver.reset());
    },

    async resetApplicationData(phaseId = "app.reset-application-data") {
      return runRecordedPhase(phaseId, () => driver.resetApplicationData());
    },

    async reload(phaseId = "app.reload") {
      return runRecordedPhase(phaseId, () => driver.reload());
    },

    async revealElement(elementId, description = elementId) {
      return runRecordedPhase(`reveal ${description}`, () => driver.revealElement(elementId));
    },

    async revealProjectionMatching(probe, needle, description = `${probe} ${needle}`) {
      return runRecordedPhase(
        `reveal ${description}`,
        () => driver.revealProjectionMatching(probe, needle),
      );
    },

    check(assertionId, pass, detail = undefined) {
      if (!expectedAssertions.has(assertionId)) {
        throw new Error(`${journey.id} does not own assertion ${assertionId}`);
      }
      if (completedAssertions.has(assertionId)) {
        throw new Error(`${journey.id} repeated assertion ${assertionId}`);
      }
      completedAssertions.add(assertionId);
      return recordJourneyCheck(result, assertionId, pass, detail);
    },

    async eventually(
      description,
      operation,
      timeoutMs = E2E_TIMING.localReadyMs,
      intervalMs = E2E_TIMING.pollIntervalMs,
    ) {
      console.log(`[${journey.id}:${platform}] wait start: ${description} (limit ${timeoutMs}ms)`);
      const observed = await observeUntil(description, operation, {
        timeoutMs,
        intervalMs,
        waitForNextProbe: driver.waitForObservation?.bind(driver) ?? null,
      });
      console.log(`[${journey.id}:${platform}] wait pass: ${description} (${observed.durationMs}ms)`);
      return observed.value;
    },

    async stable(
      description,
      operation,
      timeoutMs = E2E_TIMING.localReadyMs,
      intervalMs = E2E_TIMING.pollIntervalMs,
    ) {
      console.log(`[${journey.id}:${platform}] stable start: ${description} (limit ${timeoutMs}ms)`);
      const observed = await observeValueUntilStable(description, operation, {
        timeoutMs,
        intervalMs,
        waitForNextProbe: driver.waitForObservation?.bind(driver) ?? null,
      });
      console.log(`[${journey.id}:${platform}] stable pass: ${description} (${observed.durationMs}ms)`);
      return observed.value;
    },

    async transition(description, contract) {
      const sessionRevisionBefore = await driver.readSessionRevision?.().catch(() => null) ?? null;
      let timingRecord = null;
      console.log(
        `[${journey.id}:${platform}] transition start: ${description} ` +
          `(budget ${contract.responseTimeoutMs ?? E2E_TIMING.userResponseMs}ms)`,
      );
      const completed = await performTransition(description, {
        ...contract,
        waitForObservation:
          contract.waitForObservation ?? driver.waitForObservation?.bind(driver) ?? null,
        onTiming(timing) {
          timing.session_revision_before = sessionRevisionBefore;
          timingRecord = timing;
          result.diagnostics.user_transitions ??= [];
          result.diagnostics.user_transitions.push(timing);
          contract.onTiming?.(timing);
        },
      }).catch(async (error) => {
        if (timingRecord) {
          timingRecord.session_revision_after =
            await driver.readSessionRevision?.().catch(() => null) ?? null;
        }
        throw error;
      });
      completed.timing.session_revision_after =
        await driver.readSessionRevision?.().catch(() => null) ?? null;
      console.log(
        `[${journey.id}:${platform}] transition pass: ${description} ` +
          `(${completed.timing.response_ms}ms response, ${completed.timing.total_ms}ms total)`,
      );
      return completed.value;
    },

    async openPage(pageId) {
      return navigateSemanticPage(driver, pageId, {
        observe: runtime.eventually,
        transition: runtime.transition,
      });
    },

    async editText(description, controlId, value, options = {}) {
      return editSemanticText(driver, description, controlId, value, options, {
        transition: runtime.transition,
      });
    },

    async inspectMapAt(point) {
      return inspectSemanticMapAt(driver, point, { transition: runtime.transition });
    },

    async action(description, actionId, contract, ...unexpectedArguments) {
      if (unexpectedArguments.length > 0) {
        throw new Error(`${description} action received unexpected positional arguments`);
      }
      if (contract?.ready) {
        throw new Error(
          `${description} supplies custom action readiness; ` +
            "action readiness must come from driver.readAction(actionId)",
        );
      }
      if (typeof contract?.complete !== "function") {
        throw new Error(`${description} must declare a semantic completion condition`);
      }
      return runtime.transition(description, {
        ...contract,
        ready: () => driver.readAction(actionId),
        act: (readyElement) => driver.performAction(actionId, readyElement),
        diagnose: contract.diagnose ?? (async () => ({
          action: await driver.readAction(actionId),
          session_revision: await driver.readSessionRevision?.().catch(() => null) ?? null,
        })),
      });
    },

    async repeatableAction(description, actionId, contract) {
      if (contract?.ready) {
        throw new Error(
          `${description} supplies custom action readiness; ` +
            "action readiness must come from driver.readAction(actionId)",
        );
      }
      if (typeof contract?.complete !== "function") {
        throw new Error(`${description} must declare a semantic completion condition`);
      }
      let retainedTarget = null;
      const value = await runtime.transition(description, {
        ...contract,
        ready: () => driver.readAction(actionId),
        act: (readyElement) => {
          retainedTarget = readyElement;
          return driver.performAction(actionId, readyElement);
        },
      });
      if (!retainedTarget) throw new Error(`${description} did not retain an action target`);
      const handle = Object.freeze({ actionId, retainedTarget, platform });
      repeatableActionHandles.add(handle);
      return { value, handle };
    },

    async repeatAction(description, handle, contract) {
      if (!repeatableActionHandles.has(handle) || handle?.platform !== platform) {
        throw new Error(`${description} received an unknown repeatable action handle`);
      }
      if (contract?.ready) {
        throw new Error(
          `${description} supplies custom repeated-action readiness; ` +
            "readiness must come from the retained rendered target",
        );
      }
      if (typeof contract?.complete !== "function") {
        throw new Error(`${description} must declare a semantic completion condition`);
      }
      return runtime.transition(description, {
        ...contract,
        ready: () => driver.readRepeatedAction(handle.actionId, handle.retainedTarget),
        act: (readyElement) => driver.performRepeatedAction(
          handle.actionId,
          handle.retainedTarget,
          readyElement,
        ),
      });
    },

    async openOption(description, launcherId, optionId) {
      const rendered = await driver.readOption(launcherId, optionId);
      if (rendered) return rendered;
      return runtime.transition(`open ${description} choices`, {
        ready: () => driver.readAction(launcherId),
        act: (readyElement) => driver.openChooser(launcherId, readyElement),
        complete: () => driver.readOption(launcherId, optionId),
      });
    },

    async selectOpenOption(description, launcherId, optionId, complete) {
      if (typeof complete !== "function") {
        throw new Error(`${description} must declare a semantic completion condition`);
      }
      return runtime.transition(description, {
        ready: () => driver.readOption(launcherId, optionId),
        act: (readyElement) => driver.selectOption(launcherId, optionId, readyElement),
        complete,
      });
    },

    async chooseOption(description, launcherId, optionId, contract) {
      if (typeof contract?.complete !== "function") {
        throw new Error(`${description} must declare a semantic completion condition`);
      }
      await runtime.openOption(description, launcherId, optionId);
      return runtime.selectOpenOption(description, launcherId, optionId, contract.complete);
    },

    async toggleOption(description, launcherId, optionId, selected) {
      await runtime.openOption(description, launcherId, optionId);
      return runtime.selectOpenOption(description, launcherId, optionId, async () => {
        const option = await driver.readOption(launcherId, optionId);
        return option && semanticOptionSelected(option) === selected ? option : null;
      });
    },

    async finish(error = null) {
      if (!error) {
        const missing = [...expectedAssertions].filter((id) => !completedAssertions.has(id));
        if (missing.length > 0) {
          error = new Error(`${journey.id} omitted assertions: ${missing.join(", ")}`);
        }
      }
      if (error) {
        try {
          const failureFrame = join(artifactDir, "failure.png");
          await driver.captureFrame(failureFrame);
          result.artifacts.push(failureFrame);
        } catch (captureError) {
          result.diagnostics.capture_error = captureError.message;
        }
      }
      finishJourneyResult(result, error);
      persistJourneyResult(result, artifactDir);
      validateJourneyResult(result);
      return result;
    },
  };
  return runtime;
}

export async function executeReleaseJourney(options, implementation) {
  const runtime = createJourneyRuntime(options);
  let failure = null;
  try {
    await implementation(runtime);
  } catch (error) {
    failure = error;
  }
  const result = await runtime.finish(failure);
  if (failure) throw Object.assign(failure, { journeyResult: result });
  return result;
}
