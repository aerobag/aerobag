// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

export const JOURNEY_RESULT_SCHEMA_VERSION = 1;

export function createJourneyResult({ id, platform, fixture = null, build = null }) {
  if (!id || !platform) throw new Error("journey result requires id and platform");
  return {
    schema_version: JOURNEY_RESULT_SCHEMA_VERSION,
    id,
    platform,
    status: "running",
    started_at: new Date().toISOString(),
    finished_at: null,
    fixture,
    build,
    steps: [],
    checks: [],
    artifacts: [],
    diagnostics: {},
    error: null,
  };
}

export function recordJourneyStep(result, actionId, detail = undefined, durationMs = undefined) {
  const step = {
    action_id: actionId,
    status: "pass",
    ...(detail === undefined ? {} : { detail }),
    ...(durationMs === undefined ? {} : { duration_ms: durationMs }),
  };
  result.steps.push(step);
  return step;
}

export function recordJourneyCheck(result, assertionId, pass, detail = undefined) {
  const check = {
    assertion_id: assertionId,
    pass: Boolean(pass),
    ...(detail === undefined ? {} : { detail }),
  };
  result.checks.push(check);
  if (!check.pass) {
    throw new Error(`${assertionId} failed${detail === undefined ? "" : `: ${detail}`}`);
  }
  return check;
}

export function finishJourneyResult(result, error = null) {
  result.finished_at = new Date().toISOString();
  result.status = error ? "fail" : "pass";
  result.error = error ? (error.message ?? String(error)) : null;
  return result;
}

export function validateJourneyResult(result) {
  if (result?.schema_version !== JOURNEY_RESULT_SCHEMA_VERSION) {
    throw new Error(`journey result schema must be ${JOURNEY_RESULT_SCHEMA_VERSION}`);
  }
  if (!result.id || !result.platform || !["running", "pass", "fail"].includes(result.status)) {
    throw new Error("journey result identity or status is invalid");
  }
  const assertionIds = new Set();
  for (const check of result.checks ?? []) {
    if (!check.assertion_id || typeof check.pass !== "boolean") {
      throw new Error("journey checks require assertion_id and boolean pass");
    }
    if (assertionIds.has(check.assertion_id)) {
      throw new Error(`duplicate journey assertion ${check.assertion_id}`);
    }
    assertionIds.add(check.assertion_id);
  }
  if (result.status === "pass" && (result.error || [...result.checks].some((check) => !check.pass))) {
    throw new Error("passing journey result contains a failure");
  }
  if (result.status === "fail" && !result.error) {
    throw new Error("failed journey result requires an error");
  }
  return result;
}
