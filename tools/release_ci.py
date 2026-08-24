#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

"""Inspect exact-commit GitHub qualification runs for a release tag."""

from __future__ import annotations

import json
import os
import urllib.error
import urllib.parse
import urllib.request
from dataclasses import dataclass
from typing import Any, Callable


DEFAULT_GITHUB_REPOSITORY = "aerobag/aerobag"


class ReleaseCiError(RuntimeError):
    pass


@dataclass(frozen=True)
class WorkflowQualification:
    label: str
    state: str
    detail: str
    url: str | None = None

    @property
    def passed(self) -> bool:
        return self.state == "passed"


@dataclass(frozen=True)
class ReleaseQualification:
    tag: str
    commit: str
    ordinary_ci: WorkflowQualification
    release_journeys: WorkflowQualification

    @property
    def passed(self) -> bool:
        return self.ordinary_ci.passed and self.release_journeys.passed

    def failure_summary(self) -> str:
        failures = [
            f"{check.label}: {check.detail}"
            for check in (self.ordinary_ci, self.release_journeys)
            if not check.passed
        ]
        return "; ".join(failures) or "passed"


def github_workflow_runs(
    repository: str,
    workflow: str,
    commit: str,
    *,
    opener: Callable[..., Any] = urllib.request.urlopen,
) -> list[dict[str, Any]]:
    query = urllib.parse.urlencode({"head_sha": commit, "per_page": 50})
    workflow_id = urllib.parse.quote(workflow, safe="")
    url = (
        f"https://api.github.com/repos/{repository}/actions/workflows/"
        f"{workflow_id}/runs?{query}"
    )
    headers = {
        "Accept": "application/vnd.github+json",
        "User-Agent": "aerobag-release-manager",
        "X-GitHub-Api-Version": "2022-11-28",
    }
    if token := os.environ.get("GITHUB_TOKEN"):
        headers["Authorization"] = f"Bearer {token}"
    request = urllib.request.Request(url, headers=headers)
    try:
        with opener(request, timeout=20) as response:
            document = json.load(response)
    except (OSError, urllib.error.URLError, json.JSONDecodeError) as error:
        raise ReleaseCiError(
            f"failed to read GitHub workflow {workflow} for {commit}: {error}"
        ) from error
    runs = document.get("workflow_runs") if isinstance(document, dict) else None
    if not isinstance(runs, list):
        raise ReleaseCiError(f"GitHub returned an invalid workflow-run list for {workflow}")
    return [run for run in runs if isinstance(run, dict)]


def _newest_matching_run(
    runs: list[dict[str, Any]],
    *,
    commit: str,
    predicate: Callable[[dict[str, Any]], bool],
) -> dict[str, Any] | None:
    matches = [run for run in runs if run.get("head_sha") == commit and predicate(run)]
    return max(matches, key=lambda run: (str(run.get("created_at", "")), int(run.get("id", 0))), default=None)


def _workflow_qualification(
    label: str,
    run: dict[str, Any] | None,
    *,
    missing_detail: str,
) -> WorkflowQualification:
    if run is None:
        return WorkflowQualification(label, "missing", missing_detail)
    url = run.get("html_url") if isinstance(run.get("html_url"), str) else None
    status = run.get("status")
    conclusion = run.get("conclusion")
    if status != "completed":
        return WorkflowQualification(label, "pending", str(status or "pending"), url)
    if conclusion == "success":
        return WorkflowQualification(label, "passed", "passed", url)
    return WorkflowQualification(label, "failed", str(conclusion or "failed"), url)


def evaluate_release_qualification(
    *,
    tag: str,
    commit: str,
    ci_runs: list[dict[str, Any]],
    e2e_runs: list[dict[str, Any]],
) -> ReleaseQualification:
    ordinary_run = _newest_matching_run(
        ci_runs,
        commit=commit,
        predicate=lambda run: run.get("head_branch") == "main",
    )
    release_title = f"Release qualification {tag}"
    journey_run = _newest_matching_run(
        e2e_runs,
        commit=commit,
        predicate=lambda run: run.get("display_title") == release_title,
    )
    return ReleaseQualification(
        tag=tag,
        commit=commit,
        ordinary_ci=_workflow_qualification(
            "ordinary CI",
            ordinary_run,
            missing_detail=f"no CI run found for main at {commit}",
        ),
        release_journeys=_workflow_qualification(
            "release journeys",
            journey_run,
            missing_detail=f"no full E2E run found for tag {tag} at {commit}",
        ),
    )


def release_qualification(
    repository: str,
    tag: str,
    commit: str,
    *,
    runs_loader: Callable[[str, str, str], list[dict[str, Any]]] | None = None,
) -> ReleaseQualification:
    loader = runs_loader or github_workflow_runs
    return evaluate_release_qualification(
        tag=tag,
        commit=commit,
        ci_runs=loader(repository, "ci.yml", commit),
        e2e_runs=loader(repository, "e2e-ci.yml", commit),
    )
