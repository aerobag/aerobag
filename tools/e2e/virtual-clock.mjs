// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

export function advancingVirtualClockScript(referenceEpochMs) {
  if (!Number.isFinite(referenceEpochMs)) throw new Error("virtual clock requires a finite epoch");
  return `(() => {
    const RealDate = globalThis.Date;
    const referenceEpochMs = ${JSON.stringify(referenceEpochMs)};
    const startedAt = performance.now();
    const virtualNow = () => Math.trunc(referenceEpochMs + (performance.now() - startedAt));
    class AerobagE2EDate extends RealDate {
      constructor(...args) {
        super(...(args.length ? args : [virtualNow()]));
      }
      static now() { return virtualNow(); }
    }
    globalThis.Date = AerobagE2EDate;
  })()`;
}
