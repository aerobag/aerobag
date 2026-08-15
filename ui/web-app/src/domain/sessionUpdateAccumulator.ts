// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import {
  UI_SESSION_UPDATE_GROUPS,
  type UiSessionProjectionPatch,
  type UiSessionProjectionAssignment,
  type UiSessionUpdate,
  type UiSessionUpdateGroup,
} from "../generated/sessionUpdateWire";

type JsonObject = Record<string, unknown>;

export type SessionUpdateDisposition = "applied" | "stale" | "resync_required";
export type SessionProjectionDisposition = SessionUpdateDisposition | "full_snapshot";

export class SessionUpdateContractError extends Error {}

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
  rejectUnknownKeys(patch, new Set(["version", "assignments"]), `session update ${group} patch`);
  requireWireInteger(patch.version, `session update ${group} version`);
  if (!Array.isArray(patch.assignments)) {
    throw new SessionUpdateContractError(`session update ${group} assignments must be an array`);
  }
  for (const [index, value] of patch.assignments.entries()) {
    parseAssignment(value, `session update ${group} assignment ${index}`);
  }
  return patch as unknown as UiSessionProjectionPatch;
}

function parseAssignment(value: unknown, label: string): UiSessionProjectionAssignment {
  const assignment = requireJsonObject(value, label);
  rejectUnknownKeys(assignment, new Set(["path", "value"]), label);
  if (!Array.isArray(assignment.path) || assignment.path.length === 0) {
    throw new SessionUpdateContractError(`${label} path must be a nonempty array`);
  }
  for (const segment of assignment.path) {
    if (typeof segment !== "string" || segment.length === 0) {
      throw new SessionUpdateContractError(`${label} path segments must be nonempty strings`);
    }
  }
  if (!Object.hasOwn(assignment, "value")) {
    throw new SessionUpdateContractError(`${label} must contain value`);
  }
  return assignment as unknown as UiSessionProjectionAssignment;
}

function pathsOverlap(left: readonly string[], right: readonly string[]): boolean {
  const commonLength = Math.min(left.length, right.length);
  for (let index = 0; index < commonLength; index += 1) {
    if (left[index] !== right[index]) return false;
  }
  return true;
}

function replaceAtPath(
  current: unknown,
  path: readonly string[],
  offset: number,
  value: unknown,
  label: string,
): unknown {
  if (offset === path.length) return value;
  const segment = path[offset];
  if (Array.isArray(current)) {
    if (!/^(0|[1-9][0-9]*)$/.test(segment)) {
      throw new SessionUpdateContractError(`${label} path segment ${segment} is not an array index`);
    }
    const index = Number(segment);
    if (index >= current.length) {
      throw new SessionUpdateContractError(`${label} array index ${index} is out of range`);
    }
    const next = [...current];
    next[index] = replaceAtPath(current[index], path, offset + 1, value, label);
    return next;
  }
  const object = requireJsonObject(current, `${label} path parent`);
  if (!Object.hasOwn(object, segment)) {
    throw new SessionUpdateContractError(`${label} path does not exist at ${segment}`);
  }
  return {
    ...object,
    [segment]: replaceAtPath(object[segment], path, offset + 1, value, label),
  };
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
  if (Object.hasOwn(raw, "session_update")) {
    throw new SessionUpdateContractError("full session snapshot must not contain session_update");
  }
  const snapshot = { ...raw };
  const contractVersion = requireWireInteger(snapshot.ui_contract_version, "snapshot contract version");
  requireWireInteger(snapshot.session_revision, "snapshot revision");
  if (contractVersion !== expectedContractVersion) {
    throw new SessionUpdateContractError(
      `UI wire contract ${contractVersion} is unsupported; client requires ${expectedContractVersion}`,
    );
  }
  return snapshot;
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

    let nextSnapshot: JsonObject = this.rawSnapshot;
    const assignedPaths: Array<readonly string[]> = [];
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
      for (const [index, assignmentValue] of patch.assignments.entries()) {
        const assignment = parseAssignment(
          assignmentValue,
          `session update ${group} assignment ${index}`,
        );
        const envelopeField = assignment.path[0];
        if (envelopeField === "ui_contract_version" || envelopeField === "session_revision" || envelopeField === "session_update") {
          throw new SessionUpdateContractError(`session update ${group} cannot replace envelope field ${envelopeField}`);
        }
        const overlap = assignedPaths.find((path) => pathsOverlap(path, assignment.path));
        if (overlap) {
          throw new SessionUpdateContractError(
            `session update path ${assignment.path.join("/")} overlaps ${overlap.join("/")}`,
          );
        }
        nextSnapshot = requireJsonObject(
          replaceAtPath(
            nextSnapshot,
            assignment.path,
            0,
            assignment.value,
            `session update ${group}`,
          ),
          `session update ${group} result`,
        );
        assignedPaths.push(assignment.path);
      }
      nextVersions.push([group, patch.version]);
    }

    this.rawSnapshot = {
      ...nextSnapshot,
      ui_contract_version: update.ui_contract_version,
      session_revision: update.session_revision,
    };
    for (const [group, version] of nextVersions) this.groupVersions.set(group, version);
    return "applied";
  }

  async applyOrResync(
    value: unknown,
    loadFullSnapshot: () => Promise<unknown>,
  ): Promise<SessionUpdateDisposition> {
    const disposition = this.apply(value);
    if (disposition === "resync_required") {
      this.replaceFullSnapshot(await loadFullSnapshot());
    }
    return disposition;
  }

  async applyProjectionResult(
    value: unknown,
    loadFullSnapshot: () => Promise<unknown>,
  ): Promise<SessionProjectionDisposition> {
    if (isJsonObject(value) && Object.hasOwn(value, "app_ui_state")) {
      this.replaceFullSnapshot(value);
      return "full_snapshot";
    }
    return this.applyOrResync(value, loadFullSnapshot);
  }

}
