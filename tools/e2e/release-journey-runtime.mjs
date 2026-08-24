// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import {
  createJourneyResult, finishJourneyResult, recordJourneyCheck,
  recordJourneyStep, validateJourneyResult,
} from "./journey-result.mjs";

export function releaseJourneyFixtureUrl(platform, relativePath, fixtureOrigin = null) {
  if (/^https?:\/\//.test(relativePath)) return relativePath;
  const normalized = String(relativePath).replace(/^\/+/, "");
  if (platform === "android") {
    return `/releasejourney/${Buffer.from(normalized, "utf8").toString("hex")}`;
  }
  const path = `/release-journey/${normalized}`;
  return fixtureOrigin ? new URL(path, fixtureOrigin).href : path;
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
  mkdirSync(artifactDir, { recursive: true });

  return {
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

    async step(actionId, operation, detail = undefined) {
      const startedAt = performance.now();
      const value = await operation();
      recordJourneyStep(result, actionId, detail, Math.round(performance.now() - startedAt));
      return value;
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

    async eventually(description, operation, timeoutMs = 15_000, intervalMs = 100) {
      const deadline = Date.now() + timeoutMs;
      let lastError = null;
      while (Date.now() < deadline) {
        try {
          const value = await operation();
          if (value) return value;
        } catch (error) {
          lastError = error;
        }
        await new Promise((resolve) => setTimeout(resolve, intervalMs));
      }
      throw new Error(`${description} timed out${lastError ? `: ${lastError.message}` : ""}`);
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
      const resultPath = join(artifactDir, "result.json");
      writeFileSync(resultPath, `${JSON.stringify(result, null, 2)}\n`);
      validateJourneyResult(result);
      return result;
    },
  };
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
