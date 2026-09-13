// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

/** Owns async web-controller work, not core selection policy or geometry. */
export class MapSelectionRequests {
  private intent = 0;
  private viewport = 0;

  cancel() {
    this.intent += 1;
  }

  viewportChanged() {
    this.viewport += 1;
  }

  async run<T>(
    kind: "point" | "nav-ref",
    query: (isCurrent: () => boolean) => Promise<T>,
    apply: (result: T) => void,
    fail: (error: unknown) => void,
  ): Promise<void> {
    const intent = ++this.intent;
    const viewport = this.viewport;
    // A point hit test belongs to one map frame. A named search belongs to
    // the user's intent, even while automatic ownship-follow moves that frame.
    const isCurrent = () => intent === this.intent
      && (kind === "nav-ref" || viewport === this.viewport);
    try {
      const result = await query(isCurrent);
      if (isCurrent()) apply(result);
    } catch (error) {
      // Stale failures must not overwrite a newer selection's feedback either.
      if (isCurrent()) fail(error);
    }
  }
}
