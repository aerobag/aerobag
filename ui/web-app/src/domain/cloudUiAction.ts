// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import type { CloudPlatformEffect } from "./appCoreAdapter";

interface CloudUiActionDependencies<Snapshot> {
  platformEffect: CloudPlatformEffect | null;
  performCoreAction: () => Promise<Snapshot>;
  applySnapshot: (snapshot: Snapshot) => void;
  pumpCloudProvider: () => Promise<void>;
  writeClipboard: (text: string) => Promise<void>;
  writeClipboardFallback: (text: string) => boolean;
}

type PlatformEffectOutcome =
  | { completionLabel: string | null; error: null }
  | { completionLabel: null; error: unknown };

function startPlatformEffect(
  effect: CloudPlatformEffect | null,
  writeClipboard: (text: string) => Promise<void>,
  writeClipboardFallback: (text: string) => boolean,
): Promise<PlatformEffectOutcome> | null {
  if (effect?.kind !== "copy_text") return null;

  // Clipboard access must begin in the click's user-activation task. Waiting
  // for the core worker first makes permission behavior browser-dependent.
  const modernWrite = writeClipboard(effect.text).then<PlatformEffectOutcome, PlatformEffectOutcome>(
    () => ({ completionLabel: effect.completion_label, error: null }),
    (error: unknown) => ({ completionLabel: null, error }),
  );
  // Chromium can leave Clipboard.writeText pending indefinitely even after
  // granting permission. A synchronous copy still has user-activation here;
  // once the click task ends, that recovery path is no longer available.
  if (writeClipboardFallback(effect.text)) {
    return Promise.resolve({ completionLabel: effect.completion_label, error: null });
  }
  return modernWrite;
}

export async function performCloudUiActionWithPlatformEffect<Snapshot>({
  platformEffect,
  performCoreAction,
  applySnapshot,
  pumpCloudProvider,
  writeClipboard,
  writeClipboardFallback,
}: CloudUiActionDependencies<Snapshot>): Promise<string | null> {
  const platformOutcome = startPlatformEffect(
    platformEffect,
    writeClipboard,
    writeClipboardFallback,
  );
  const snapshot = await performCoreAction();
  applySnapshot(snapshot);

  if (platformOutcome) {
    const outcome = await platformOutcome;
    if (outcome.error !== null) throw outcome.error;
    return outcome.completionLabel;
  }

  await pumpCloudProvider();
  return null;
}
