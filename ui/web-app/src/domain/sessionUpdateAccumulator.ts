// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import {
  UI_SESSION_UPDATE_GROUPS,
  type UiSessionProjectionPatch,
  type UiSessionUpdate,
  type UiSessionUpdateGroup,
} from "../generated/sessionUpdateWire";

type JsonObject = Record<string, unknown>;

export type SessionUpdateDisposition = "applied" | "stale" | "resync_required";

export class SessionUpdateContractError extends Error {}
export class SessionUpdateProjectionMismatchError extends Error {}

function isJsonObject(value: unknown): value is JsonObject {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function requireJsonObject(value: unknown, label: string): JsonObject {
  if (!isJsonObject(value)) {
    throw new SessionUpdateContractError(`${label} must be a JSON object`);
  }
  return value;
}

function requireWireInteger(value: unknown, label: string): number {
  if (!Number.isSafeInteger(value) || (value as number) < 0) {
    throw new SessionUpdateContractError(`${label} must be a non-negative safe integer`);
  }
  return value as number;
}

function rejectUnknownKeys(value: JsonObject, allowed: ReadonlySet<string>, label: string): void {
  const unknown = Object.keys(value).filter((key) => !allowed.has(key));
  if (unknown.length > 0) {
    throw new SessionUpdateContractError(`${label} has unknown fields: ${unknown.join(", ")}`);
  }
}

function parsePatch(value: unknown, group: UiSessionUpdateGroup): UiSessionProjectionPatch | null {
  if (value === undefined || value === null) return null;
  const patch = requireJsonObject(value, `session update ${group} patch`);
  rejectUnknownKeys(patch, new Set(["version", "fields"]), `session update ${group} patch`);
  requireWireInteger(patch.version, `session update ${group} version`);
  requireJsonObject(patch.fields, `session update ${group} fields`);
  return patch as unknown as UiSessionProjectionPatch;
}

export function parseSessionUpdate(value: unknown): UiSessionUpdate {
  const update = requireJsonObject(value, "session update");
  rejectUnknownKeys(
    update,
    new Set(["ui_contract_version", "session_revision", ...UI_SESSION_UPDATE_GROUPS]),
    "session update",
  );
  requireWireInteger(update.ui_contract_version, "session update contract version");
  requireWireInteger(update.session_revision, "session update revision");
  for (const group of UI_SESSION_UPDATE_GROUPS) {
    parsePatch(update[group], group);
  }
  return update as unknown as UiSessionUpdate;
}

function sanitizedFullSnapshot(value: unknown, expectedContractVersion: number): JsonObject {
  const raw = requireJsonObject(value, "session snapshot");
  const snapshot = { ...raw };
  delete snapshot.session_update;
  const contractVersion = requireWireInteger(snapshot.ui_contract_version, "snapshot contract version");
  requireWireInteger(snapshot.session_revision, "snapshot revision");
  if (contractVersion !== expectedContractVersion) {
    throw new SessionUpdateContractError(
      `UI wire contract ${contractVersion} is unsupported; client requires ${expectedContractVersion}`,
    );
  }
  return snapshot;
}

function jsonEqual(left: unknown, right: unknown): boolean {
  if (Object.is(left, right)) return true;
  if (Array.isArray(left) || Array.isArray(right)) {
    return Array.isArray(left)
      && Array.isArray(right)
      && left.length === right.length
      && left.every((value, index) => jsonEqual(value, right[index]));
  }
  if (!isJsonObject(left) || !isJsonObject(right)) return false;
  const leftKeys = Object.keys(left);
  const rightKeys = Object.keys(right);
  return leftKeys.length === rightKeys.length
    && leftKeys.every((key) => Object.hasOwn(right, key) && jsonEqual(left[key], right[key]));
}

export class SessionUpdateAccumulator {
  private rawSnapshot: JsonObject;
  private readonly groupVersions = new Map<UiSessionUpdateGroup, number>();

  constructor(initialSnapshot: unknown, private readonly expectedContractVersion: number) {
    this.rawSnapshot = sanitizedFullSnapshot(initialSnapshot, expectedContractVersion);
  }

  get snapshot(): JsonObject {
    return this.rawSnapshot;
  }

  replaceFullSnapshot(value: unknown): JsonObject {
    const nextSnapshot = sanitizedFullSnapshot(value, this.expectedContractVersion);
    if ((nextSnapshot.session_revision as number) < (this.rawSnapshot.session_revision as number)) {
      return this.rawSnapshot;
    }
    this.rawSnapshot = nextSnapshot;
    this.groupVersions.clear();
    return this.rawSnapshot;
  }

  apply(value: unknown): SessionUpdateDisposition {
    const update = parseSessionUpdate(value);
    if (update.ui_contract_version !== this.expectedContractVersion) {
      throw new SessionUpdateContractError(
        `UI wire contract ${update.ui_contract_version} is unsupported; client requires ${this.expectedContractVersion}`,
      );
    }
    const currentRevision = this.rawSnapshot.session_revision as number;
    if (update.session_revision <= currentRevision) return "stale";
    if (update.session_revision !== currentRevision + 1) return "resync_required";

    const nextFields: JsonObject = {};
    const nextVersions: Array<[UiSessionUpdateGroup, number]> = [];
    for (const group of UI_SESSION_UPDATE_GROUPS) {
      const patch = parsePatch(update[group], group);
      if (!patch) continue;
      const previousVersion = this.groupVersions.get(group);
      if (previousVersion !== undefined && patch.version <= previousVersion) {
        throw new SessionUpdateContractError(
          `session update ${group} version ${patch.version} does not advance ${previousVersion}`,
        );
      }
      for (const [field, fieldValue] of Object.entries(patch.fields)) {
        if (field === "ui_contract_version" || field === "session_revision" || field === "session_update") {
          throw new SessionUpdateContractError(`session update ${group} cannot replace envelope field ${field}`);
        }
        if (Object.hasOwn(nextFields, field)) {
          throw new SessionUpdateContractError(`session update field ${field} appears in multiple groups`);
        }
        nextFields[field] = fieldValue;
      }
      nextVersions.push([group, patch.version]);
    }

    this.rawSnapshot = {
      ...this.rawSnapshot,
      ...nextFields,
      ui_contract_version: update.ui_contract_version,
      session_revision: update.session_revision,
    };
    for (const [group, version] of nextVersions) this.groupVersions.set(group, version);
    return "applied";
  }

  applyTransitionalMutationSnapshot(value: unknown): SessionUpdateDisposition {
    const fullSnapshot = sanitizedFullSnapshot(value, this.expectedContractVersion);
    const raw = requireJsonObject(value, "session mutation result");
    if (!Object.hasOwn(raw, "session_update")) {
      throw new SessionUpdateContractError("session mutation result is missing session_update");
    }
    const update = parseSessionUpdate(raw.session_update);
    if (fullSnapshot.session_revision !== update.session_revision) {
      throw new SessionUpdateContractError("session mutation snapshot and update revisions differ");
    }
    const disposition = this.apply(update);
    if (disposition === "resync_required") {
      this.replaceFullSnapshot(fullSnapshot);
      return disposition;
    }
    if (disposition === "applied" && !jsonEqual(this.rawSnapshot, fullSnapshot)) {
      throw new SessionUpdateProjectionMismatchError(
        `session update revision ${update.session_revision} does not reproduce core's full snapshot`,
      );
    }
    return disposition;
  }
}
