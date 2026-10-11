// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import { E2E_TIMING, observeUntil } from "./transition-contract.mjs";

export const LOCAL_PERSISTENCE_PROJECTION = "org.aerobag.app:id/e2e_local_persistence_projection";

export function persistenceReceiptFromNode(node) {
  if (!node) return null;
  const process = /^projection-provider:([^:]+):\d+$/.exec(node["semantic-path"] ?? "")?.[1];
  if (!process) throw new Error("local persistence projection omitted process identity");
  return { ...JSON.parse(node["state-description"]), process };
}

// No flush, save action, retry or disk inspection here. Observe the same
// completion receipts that the real document writer supplies to core.
export async function restartAndroidAfterPersistence({
  readStartup, readPersistence, restart, waitForInteractive, waitForObservation,
  scheduler,
}) {
  let target = null;
  let owner = null;
  const saved = await observeUntil("Android local documents durably saved before restart", async () => {
    const startup = await readStartup();
    const persistence = await readPersistence();
    if (startup?.ready === "true") {
      target = Number(startup.session_revision);
      if (!Number.isSafeInteger(target)) throw new Error("startup omitted session revision");
    }
    if (persistence?.observation_error) throw new Error(persistence.observation_error);
    if (persistence) {
      if (persistence.schema_version !== 1 || !Number.isSafeInteger(persistence.session_revision) ||
          !Number.isSafeInteger(persistence.owner) || !persistence.process || !persistence.available || !persistence.documents) {
        throw new Error(`invalid/unavailable local persistence receipt: ${JSON.stringify(persistence)}`);
      }
      const identity = `${persistence.process}/${persistence.owner}`;
      if (owner !== null && owner !== identity) throw new Error("persistence owner changed before restart");
      owner = identity;
      if (persistence.blocked_reason) throw new Error(`local persistence blocked: ${persistence.blocked_reason}`);
      for (const [key, state] of Object.entries(persistence.documents)) {
        if (state.error) throw new Error(`local persistence failed for ${key}: ${state.error}`);
        if (!Number.isSafeInteger(state.revision) || !Number.isSafeInteger(state.saved_revision) ||
            typeof state.pending !== "boolean") throw new Error(`invalid persistence receipt for ${key}`);
      }
    }
    return { target_revision: target, startup, persistence };
  }, {
    timeoutMs: E2E_TIMING.localResourceMs,
    waitForNextProbe: waitForObservation,
    scheduler,
    accept: ({ target_revision, startup, persistence }) => target_revision !== null &&
      startup?.ready === "true" && startup.disclaimer_required === "false" &&
      persistence?.session_revision >= target_revision &&
      // A startup session must actually have read its settings, not merely
      // expose an empty, apparently idle writer.
      Boolean(persistence.documents["aerobag.core.settings.v1"]) &&
      Object.values(persistence.documents).every(state =>
        !state.pending && state.saved_revision === state.revision),
  });
  await restart();
  await waitForInteractive({ requirePreservedDisclaimer: true });
  return saved.value;
}
