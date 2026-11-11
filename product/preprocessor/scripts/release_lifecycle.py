# SPDX-FileCopyrightText: 2026 Aerobag contributors
# SPDX-License-Identifier: AGPL-3.0-or-later

"""Controller-owned operational lifecycle, independent of retained artifacts."""

from dataclasses import dataclass
from enum import Enum
import re


class Phase(str, Enum):
    SERVING = "serving"
    PREPARING = "preparing"
    RETIRED = "retired"


class Role(str, Enum):
    PRODUCTION = "production"
    STAGING = "staging"
    SUNSET = "sunset"


@dataclass(frozen=True)
class Release:
    commit: str
    phase: Phase
    role: Role | None

    def to_dict(self) -> dict:
        return {"commit": self.commit, "phase": self.phase.value,
                "role": self.role.value if self.role else None}


def roles(production: str | None, staging: str | None, sunset: list[str]) -> dict[str, Role]:
    if not isinstance(sunset, list):
        raise ValueError("release lifecycle sunset assignments must be a list")
    assignments = [(production, Role.PRODUCTION), (staging, Role.STAGING),
                   *((tag, Role.SUNSET) for tag in sunset)]
    result = {}
    for tag, role in assignments:
        if tag is None:
            continue
        if not isinstance(tag, str) or not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._-]{0,79}", tag):
            raise ValueError("invalid release lifecycle tag")
        if tag in result:
            raise ValueError(f"ambiguous release lifecycle role for {tag}")
        result[tag] = role
    return result


def project(commits: dict[str, str], serving: dict[str, Role], requested: dict[str, Role]) -> dict:
    """The controller calls this when atomically saving its observed state."""
    if (serving.keys() | requested.keys()) - commits.keys():
        raise ValueError("release lifecycle assignment has no release identity")
    result = {}
    for tag, commit in sorted(commits.items()):
        if tag in serving:
            release = Release(commit, Phase.SERVING, serving[tag])
        elif tag in requested:
            release = Release(commit, Phase.PREPARING, requested[tag])
        else:
            release = Release(commit, Phase.RETIRED, None)
        result[tag] = release.to_dict()
    return result


def read(observed: dict) -> dict[str, Release]:
    """Reject incomplete or contradictory evidence; missing never means retired."""
    if observed.get("schema_version") != 3:
        raise ValueError("release lifecycle requires observed state schema_version 3")
    document = observed.get("release_lifecycle")
    records = observed.get("releases")
    if not isinstance(document, dict) or not isinstance(records, dict) or document.keys() != records.keys():
        raise ValueError("release lifecycle must identify every known release")
    serving = roles(observed.get("production"), observed.get("staging"), observed.get("sunset", []))
    result = {}
    for tag, value in document.items():
        roles(tag, None, [])  # Validate even historical identifiers.
        if not isinstance(value, dict) or set(value) != {"commit", "phase", "role"}:
            raise ValueError(f"invalid release lifecycle entry for {tag}")
        commit = value["commit"]
        if (not isinstance(commit, str) or not re.fullmatch(r"[0-9a-f]{40}", commit)
                or not isinstance(records[tag], dict) or records[tag].get("commit") != commit):
            raise ValueError(f"release lifecycle identity mismatch for {tag}")
        phase = Phase(value["phase"])
        role = None if value["role"] is None else Role(value["role"])
        if (phase == Phase.RETIRED) != (role is None):
            raise ValueError(f"invalid release lifecycle role for {tag}")
        if (tag in serving) != (phase == Phase.SERVING) or (tag in serving and serving[tag] != role):
            raise ValueError(f"release lifecycle disagrees with serving assignment for {tag}")
        result[tag] = Release(commit, phase, role)
    if serving.keys() - result.keys():
        raise ValueError("serving release absent from lifecycle")
    return result
