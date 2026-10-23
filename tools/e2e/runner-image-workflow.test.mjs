// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";
import test from "node:test";
import { requireWebDependency } from "../../ui/web-app/scripts/web-workspace-require.mjs";

const { parse } = requireWebDependency("yaml");
// Upgrade this baseline deliberately, together with the workflows and evidence
// described in docs/testing/hosted-ci.md. A floating label is not a pinned OS.
const runnerImage = "ubuntu-24.04";
const workflows = new URL("../../.github/workflows/", import.meta.url);
let runnerJobs = 0;
for (const name of readdirSync(workflows).filter((name) => /\.ya?ml$/.test(name)).sort()) {
  const workflow = parse(readFileSync(new URL(name, workflows), "utf8"));
  for (const [jobId, job] of Object.entries(workflow.jobs ?? {})) {
    if (job.uses) continue; // Reusable workflows choose their own job runners.
    runnerJobs += 1;
    test(`${name}: ${jobId} pins the reviewed runner OS`, () => {
      assert.equal(job["runs-on"], runnerImage,
        "Runner OS upgrades require an explicit workflow policy review; do not use floating labels");
    });
  }
}
test("Runner OS audit finds execution jobs", () => {
  assert.ok(runnerJobs > 0, "No runner jobs audited; update the audit if workflows moved");
});
