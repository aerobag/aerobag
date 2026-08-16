// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import {
  UI_SESSION_UPDATE_GROUPS,
  type UiSessionUpdateGroup,
} from "../generated/sessionUpdateWire";
import type {
  UiSessionProjectionPublication,
  UiSessionSnapshot,
} from "./appCoreAdapter";

export const HIGH_RATE_SESSION_UPDATE_GROUPS = [
  "ownship",
  "situation",
  "flight_data",
] as const satisfies readonly UiSessionUpdateGroup[];

export const SHELL_SESSION_UPDATE_GROUPS = UI_SESSION_UPDATE_GROUPS.filter(
  (group) => !HIGH_RATE_SESSION_UPDATE_GROUPS.includes(group as typeof HIGH_RATE_SESSION_UPDATE_GROUPS[number]),
);

export const NO_SESSION_UPDATE_GROUPS: readonly UiSessionUpdateGroup[] = [];

export function publicationAffectsGroups(
  publication: UiSessionProjectionPublication,
  groups: readonly UiSessionUpdateGroup[],
): boolean {
  if (publication.fullSnapshot) return true;
  return publication.changedGroups.some((group) => groups.includes(group));
}

type ListenerRegistration = {
  groups: readonly UiSessionUpdateGroup[];
  listener: () => void;
};

export type SessionRenderStoreStats = {
  publications: number;
  fullSnapshots: number;
  shellPublications: number;
  highRatePublications: number;
  listenerNotifications: number;
};

export class SessionRenderStore {
  private currentSnapshot: UiSessionSnapshot;
  private readonly listeners = new Set<ListenerRegistration>();
  private readonly counters: SessionRenderStoreStats = {
    publications: 0,
    fullSnapshots: 0,
    shellPublications: 0,
    highRatePublications: 0,
    listenerNotifications: 0,
  };

  constructor(initialSnapshot: UiSessionSnapshot) {
    this.currentSnapshot = initialSnapshot;
  }

  get snapshot(): UiSessionSnapshot {
    return this.currentSnapshot;
  }

  get stats(): SessionRenderStoreStats {
    return { ...this.counters };
  }

  publish(publication: UiSessionProjectionPublication): boolean {
    if (publication.snapshot.session_revision < this.currentSnapshot.session_revision) {
      return false;
    }
    this.currentSnapshot = publication.snapshot;
    this.counters.publications += 1;
    if (publication.fullSnapshot) this.counters.fullSnapshots += 1;
    if (publicationAffectsGroups(publication, SHELL_SESSION_UPDATE_GROUPS)) {
      this.counters.shellPublications += 1;
    }
    if (publicationAffectsGroups(publication, HIGH_RATE_SESSION_UPDATE_GROUPS)) {
      this.counters.highRatePublications += 1;
    }
    for (const registration of this.listeners) {
      if (!publicationAffectsGroups(publication, registration.groups)) continue;
      this.counters.listenerNotifications += 1;
      registration.listener();
    }
    return true;
  }

  replaceUnannouncedSnapshot(snapshot: UiSessionSnapshot): boolean {
    if (snapshot.session_revision < this.currentSnapshot.session_revision) return false;
    if (snapshot.session_revision === this.currentSnapshot.session_revision && snapshot === this.currentSnapshot) {
      return true;
    }
    return this.publish({
      landing: { kind: "full_snapshot", value: snapshot },
      snapshot,
      changedGroups: UI_SESSION_UPDATE_GROUPS,
      fullSnapshot: true,
    });
  }

  subscribe(groups: readonly UiSessionUpdateGroup[], listener: () => void): () => void {
    const registration = { groups, listener };
    this.listeners.add(registration);
    return () => this.listeners.delete(registration);
  }
}

export class RenderValueStore<T> {
  private currentValue: T;
  private readonly listeners = new Set<() => void>();

  constructor(initialValue: T) {
    this.currentValue = initialValue;
  }

  get value(): T {
    return this.currentValue;
  }

  publish(value: T): void {
    if (Object.is(value, this.currentValue)) return;
    this.currentValue = value;
    for (const listener of this.listeners) listener();
  }

  subscribe(listener: () => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }
}
