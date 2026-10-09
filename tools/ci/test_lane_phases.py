# SPDX-FileCopyrightText: 2026 Aerobag contributors
# SPDX-License-Identifier: AGPL-3.0-or-later

"""Controlled admission ordering, not host-speed assertions."""

import json
from pathlib import Path
import tempfile
import threading
from unittest import mock

import pytest
import local_candidate_qualification as q


def test_shared_build_resource_is_serial_but_prebuilt_tests_overlap():
    running_builds = set()
    finished = set()
    lock = threading.Lock()
    test_barrier = threading.Barrier(2)
    events = []

    def run(lane, logs):
        owner, phase = lane.name.split(".")
        with lock:
            events.append(lane.name)
            if phase == "build":
                assert not running_builds
                running_builds.add(owner)
            else:
                assert f"{owner}.build" in finished
        if phase == "test":
            # The first test cannot finish until the second build and test have
            # started. Holding the resource for a whole lane would deadlock.
            test_barrier.wait(timeout=30)
        with lock:
            if phase == "build":
                running_builds.remove(owner)
            finished.add(lane.name)
        log = logs / lane.name
        log.write_text(lane.name)
        return q.LaneResult(lane.name, 0, 0.1, log)

    lanes = [q.Lane(name, (), phases=(
        q.LanePhase("build", ("build",), ("cargo",)),
        q.LanePhase("test", ("test",)),
    )) for name in ("a", "b")]
    with tempfile.TemporaryDirectory() as directory, mock.patch.object(q, "run_lane", side_effect=run):
        results = q.run_lanes(lanes, Path(directory), 2)
        metrics = [json.loads(row) for row in (Path(directory) / "phase-timings.jsonl").read_text().splitlines()]
    assert all(result.passed for result in results)
    assert len(events) == 4
    assert len(metrics) == 4
    assert all(row["execution_seconds"] == 0.1 and row["queued_seconds"] >= 0 for row in metrics)


def test_waiting_for_build_does_not_start_its_execution_budget_or_block_unrelated_work():
    held = threading.Event()
    release = threading.Event()
    events = []

    def run(lane, logs):
        events.append(lane.name)
        if lane.name == "first.build":
            held.set()
            assert release.wait(30)
        elif lane.name == "unrelated":
            assert held.wait(30)
            assert "second.build" not in events
            release.set()
        elif lane.name == "second.build":
            assert release.is_set()
            assert lane.timeout_seconds == 1
        log = logs / lane.name
        log.write_text(lane.name)
        return q.LaneResult(lane.name, 0, 0, log)

    lanes = [q.Lane(name, (), timeout_seconds=1, phases=(
        q.LanePhase("build", ("build",), ("cargo",)),
    )) for name in ("first", "second")]
    lanes.append(q.Lane("unrelated", ("test",)))
    with tempfile.TemporaryDirectory() as directory, mock.patch.object(q, "run_lane", side_effect=run):
        q.run_lanes(lanes, Path(directory), 2)
    assert events.index("second.build") > events.index("unrelated")


@pytest.mark.parametrize("failure_code", [1, 124, 127])
def test_failed_build_blocks_only_its_dependents_and_releases_resources(failure_code):
    calls = []

    def run(lane, logs):
        calls.append(lane.name)
        log = logs / lane.name
        log.write_text("injected build failure" if lane.name == "bad.build" else "passed")
        return q.LaneResult(lane.name, failure_code if lane.name == "bad.build" else 0, 0, log)

    lanes = [q.Lane(name, (), phases=(
        q.LanePhase("build", ("build",), ("cargo",)),
        q.LanePhase("test", ("test",)),
    )) for name in ("bad", "good")]
    with tempfile.TemporaryDirectory() as directory, mock.patch.object(q, "run_lane", side_effect=run):
        with pytest.raises(q.QualificationError, match="bad"):
            q.run_lanes(lanes, Path(directory), 2)
        assert "BLOCKED test: prerequisite build failed" in (Path(directory) / "bad.log").read_text()
    assert calls == ["bad.build", "good.build", "good.test"]


def test_resource_admission_is_independent_of_worker_count():
    for workers in (1, 2, 8):
        with tempfile.TemporaryDirectory() as directory:
            lanes = [q.Lane(str(i), (), phases=(
                q.LanePhase("build", ("true",), ("cargo",)),
                q.LanePhase("test", ("true",)),
            )) for i in range(3)]
            assert all(result.passed for result in q.run_lanes(lanes, Path(directory), workers))
