// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { describe, expect, it } from "vitest";
import {
  classifyCreateOnceExperiment,
  classifyRace,
  redactObservationUrl,
  type WriteOutcome,
} from "./driveCasApi";

function outcome(writer: string, status: number | null): WriteOutcome {
  return {
    writer,
    success: status !== null && status >= 200 && status < 300,
    conflict: status === 409 || status === 412,
    terminal_status: status,
    observations: [],
  };
}

describe("Drive CAS race classification", () => {
  it("accepts exactly one committed writer and one rejected stale writer", () => {
    expect(classifyRace([outcome("A", 200), outcome("B", 412)], "A")).toBe("single-winner");
  });

  it("rejects two successful writers even when one wins the final content", () => {
    expect(classifyRace([outcome("A", 200), outcome("B", 200)], "B")).toBe("multiple-winners");
  });

  it("does not call transport failures successful exclusion", () => {
    expect(classifyRace([outcome("A", 200), outcome("B", null)], "A")).toBe("inconclusive");
  });

  it("does not accept a final payload that disagrees with the successful writer", () => {
    expect(classifyRace([outcome("A", 200), outcome("B", 412)], "B")).toBe("inconclusive");
  });
});

describe("Drive CAS evidence redaction", () => {
  it("redacts resumable upload capabilities without hiding the request shape", () => {
    expect(redactObservationUrl(
      "https://www.googleapis.com/upload/drive/v3/files/file-id?uploadType=resumable&upload_id=secret&session_crd=also-secret",
    )).toBe(
      "https://www.googleapis.com/upload/drive/v3/files/file-id?uploadType=resumable&upload_id=%3Credacted%3E&session_crd=%3Credacted%3E",
    );
  });

  it("leaves ordinary Drive API URLs unchanged", () => {
    const url = "https://www.googleapis.com/drive/v3/files/file-id?alt=media";
    expect(redactObservationUrl(url)).toBe(url);
  });
});

describe("Drive create-once classification", () => {
  it("accepts repeated single-winner races and a conflicting retry", () => {
    expect(classifyCreateOnceExperiment(
      ["single-winner", "single-winner"],
      outcome("first", 200),
      outcome("retry", 409),
    )).toBe("create-once-observed");
  });

  it("rejects a retry that creates the same generated ID twice", () => {
    expect(classifyCreateOnceExperiment(
      ["single-winner"],
      outcome("first", 200),
      outcome("retry", 200),
    )).toBe("unsafe");
  });

  it("rejects a race with multiple successful creators", () => {
    expect(classifyCreateOnceExperiment(
      ["multiple-winners"],
      outcome("first", 200),
      outcome("retry", 409),
    )).toBe("unsafe");
  });

  it("keeps transport failures inconclusive", () => {
    expect(classifyCreateOnceExperiment(
      ["inconclusive"],
      outcome("first", 200),
      outcome("retry", null),
    )).toBe("inconclusive");
  });
});
