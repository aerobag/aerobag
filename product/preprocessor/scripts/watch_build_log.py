#!/usr/bin/env python3

import argparse
import curses
from datetime import datetime, timezone
import os
import re
import time
from dataclasses import dataclass
from pathlib import Path


LAUNCH_RE = re.compile(
    r"^(?:(?P<wall>\S+)\s+)?(?P<ts>\+\d+:\d+(?::\d+)?)\s+launch\s+(?P<task>\S+)\s+"
    r"launched=(?P<launched>\d+)/(?P<total>\d+)\s+"
    r"completed=(?P<completed>\d+)/(?P=total)\s+"
    r"weight=(?P<weight>\d+)\s+running_units=(?P<running>\d+)/(?P<budget>\d+)"
)

COMPLETE_RE = re.compile(
    r"^(?:(?P<wall>\S+)\s+)?(?P<ts>\+\d+:\d+(?::\d+)?)\s+complete\s+(?P<task>\S+)\s+"
    r"completed=(?P<completed>\d+)/(?P<total>\d+)\s+"
    r"running_units=(?P<running>\d+)/(?P<budget>\d+)(?P<rest>.*)$"
)

FINAL_RE = re.compile(
    r"^(?:(?P<wall>\S+)\s+)?(?P<ts>\+\d+:\d+(?::\d+)?)\s+complete\s+(?P<result>PASS|FAIL)(?P<rest>.*)$"
)

READY_RE = re.compile(
    r"^.*scheduler-ready\s+tasks=(?P<total>\d+)\s+work_unit_budget=(?P<budget>\d+).*$"
)

BEGIN_RE = re.compile(
    r"^(?:(?P<wall>\S+)\s+)?(?P<ts>\+\d+:\d+(?::\d+)?)\s+begin\s+(?P<rest>.*)$"
)

CYCLE_RE = re.compile(
    r"^(?:(?P<wall>\S+)\s+)?(?P<ts>\+\d+:\d+(?::\d+)?)\s+cycle\s+bundle=(?P<bundle>\S+)\s+(?P<rest>.*)$"
)


@dataclass
class TaskState:
    task: str
    launched_at: str
    launched_wall: str
    weight: int
    status: str
    details: str = ""
    completed_at: str | None = None
    completed_wall: str | None = None


class BuildState:
    def __init__(self) -> None:
        self.total_tasks = 0
        self.work_unit_budget = 0
        self.launched = 0
        self.completed = 0
        self.running_units = 0
        self.header = ""
        self.cycle_summary = ""
        self.bundle_cycle = "?"
        self.pid: int | None = None
        self.final_result: str | None = None
        self.final_details = ""
        self.final_at: str | None = None
        self.tasks: dict[str, TaskState] = {}
        self.completion_order: list[str] = []
        self.last_line = ""
        self.last_timestamp = ""

    def apply_line(self, line: str) -> None:
        self.last_line = line

        match = BEGIN_RE.match(line)
        if match:
            self._reset_for_new_run()
            self.header = f"{match.group('ts')} {match.group('rest')}"
            self.pid = parse_pid(match.group("rest"))
            self.last_timestamp = match.group("ts")
            return

        match = CYCLE_RE.match(line)
        if match:
            self.last_timestamp = match.group("ts")
            self.bundle_cycle = match.group("bundle")
            self.cycle_summary = match.group("rest")
            return

        match = READY_RE.match(line)
        if match:
            self.total_tasks = int(match.group("total"))
            self.work_unit_budget = int(match.group("budget"))
            return

        match = LAUNCH_RE.match(line)
        if match:
            self.last_timestamp = match.group("ts")
            task = match.group("task")
            self.total_tasks = int(match.group("total"))
            self.launched = int(match.group("launched"))
            self.completed = int(match.group("completed"))
            self.running_units = int(match.group("running"))
            self.work_unit_budget = int(match.group("budget"))
            self.tasks[task] = TaskState(
                task=task,
                launched_at=match.group("ts"),
                launched_wall=match.group("wall"),
                weight=int(match.group("weight")),
                status="active",
            )
            return

        match = COMPLETE_RE.match(line)
        if match:
            self.last_timestamp = match.group("ts")
            task = match.group("task")
            self.total_tasks = int(match.group("total"))
            self.completed = int(match.group("completed"))
            self.running_units = int(match.group("running"))
            self.work_unit_budget = int(match.group("budget"))
            task_state = self.tasks.get(task)
            if task_state is None:
                task_state = TaskState(
                    task=task,
                    launched_at="?",
                    launched_wall=match.group("wall"),
                    weight=0,
                    status="done",
                )
                self.tasks[task] = task_state
            task_state.status = "done"
            task_state.completed_at = match.group("ts")
            task_state.completed_wall = match.group("wall")
            task_state.details = match.group("rest").strip()
            if task not in self.completion_order:
                self.completion_order.append(task)
            return

        match = FINAL_RE.match(line)
        if match:
            self.last_timestamp = match.group("ts")
            self.final_result = match.group("result")
            self.final_details = match.group("rest").strip()
            self.final_at = match.group("ts")
            return

    def _reset_for_new_run(self) -> None:
        self.total_tasks = 0
        self.work_unit_budget = 0
        self.launched = 0
        self.completed = 0
        self.running_units = 0
        self.header = ""
        self.cycle_summary = ""
        self.bundle_cycle = "?"
        self.pid = None
        self.final_result = None
        self.final_details = ""
        self.final_at = None
        self.tasks = {}
        self.completion_order = []
        self.last_timestamp = ""

    def active_tasks(self) -> list[TaskState]:
        return sorted(
            (task for task in self.tasks.values() if task.status == "active"),
            key=lambda task: (task.launched_at, task.task),
        )

    def recent_completed(self, limit: int) -> list[TaskState]:
        keys = self.completion_order[-limit:]
        return [self.tasks[key] for key in reversed(keys)]

    def pending_count(self) -> int:
        return max(self.total_tasks - self.launched, 0)


def read_state(log_path: Path) -> BuildState:
    state = BuildState()
    if not log_path.exists():
        return state
    with log_path.open("r", encoding="utf-8", errors="replace") as handle:
        for raw_line in handle:
            state.apply_line(raw_line.rstrip("\n"))
    return state


def parse_pid(header_rest: str) -> int | None:
    for token in header_rest.split():
        if token.startswith("pid="):
            try:
                return int(token.split("=", 1)[1])
            except ValueError:
                return None
    return None


def pid_is_alive(pid: int | None) -> bool | None:
    if pid is None:
        return None
    return Path(f"/proc/{pid}").exists()


def format_runtime(now_wall: datetime, launched_wall: str) -> str:
    if not launched_wall:
        return "?"
    try:
        start = datetime.fromisoformat(launched_wall)
        delta = (now_wall - start).total_seconds()
    except ValueError:
        return "?"
    total = max(int(delta), 0)
    hours, rem = divmod(total, 3600)
    minutes, seconds = divmod(rem, 60)
    if hours:
        return f"{hours}:{minutes:02d}:{seconds:02d}"
    return f"{minutes}:{seconds:02d}"


def draw_line(stdscr, row: int, col: int, text: str, attr: int, max_x: int) -> None:
    if row < 0:
        return
    clipped = text[: max(0, max_x - col - 1)]
    stdscr.addstr(row, col, clipped, attr)


def run_ui(stdscr, log_path: Path, refresh_seconds: float) -> None:
    curses.curs_set(0)
    curses.use_default_colors()
    curses.init_pair(1, curses.COLOR_GREEN, -1)
    curses.init_pair(2, curses.COLOR_CYAN, -1)
    curses.init_pair(3, curses.COLOR_WHITE, -1)
    curses.init_pair(4, curses.COLOR_YELLOW, -1)
    curses.init_pair(5, curses.COLOR_RED, -1)

    while True:
        state = read_state(log_path)
        max_y, max_x = stdscr.getmaxyx()
        stdscr.erase()

        draw_line(
            stdscr,
            0,
            0,
            f"log: {log_path}",
            curses.color_pair(2) | curses.A_BOLD,
            max_x,
        )
        draw_line(
            stdscr,
            1,
            0,
            state.header or "waiting for build header...",
            curses.A_DIM,
            max_x,
        )

        summary = (
            f"bundle_cycle={state.bundle_cycle} "
            f"active={len(state.active_tasks())} "
            f"completed={state.completed}/{state.total_tasks or '?'} "
            f"scheduled={state.launched}/{state.total_tasks or '?'} "
            f"running_units={state.running_units}/{state.work_unit_budget or '?'} "
            f"pending={state.pending_count()}"
        )
        draw_line(stdscr, 3, 0, summary, curses.A_BOLD, max_x)
        if state.cycle_summary:
            draw_line(stdscr, 4, 0, state.cycle_summary, curses.A_DIM, max_x)
            status_row = 5
            liveness_row = 6
            row = 8
        else:
            status_row = 4
            liveness_row = 5
            row = 7
        if state.final_result == "PASS":
            status_text = f"result=PASS {state.final_at or ''} {state.final_details}".strip()
            status_attr = curses.color_pair(1) | curses.A_BOLD
        elif state.final_result == "FAIL":
            status_text = f"result=FAIL {state.final_at or ''} {state.final_details}".strip()
            status_attr = curses.color_pair(5) | curses.A_BOLD
        else:
            status_text = "result=in_progress"
            status_attr = curses.color_pair(4) | curses.A_BOLD
        draw_line(stdscr, status_row, 0, status_text, status_attr, max_x)
        pid_alive = pid_is_alive(state.pid)
        if pid_alive is None:
            liveness = "pid=unknown"
            liveness_attr = curses.A_DIM
        elif pid_alive:
            liveness = f"pid={state.pid} alive"
            liveness_attr = curses.color_pair(1) | curses.A_BOLD
        else:
            liveness = f"pid={state.pid} dead"
            liveness_attr = curses.color_pair(4) | curses.A_BOLD
        draw_line(stdscr, liveness_row, 0, liveness, liveness_attr, max_x)

        draw_line(
            stdscr,
            row,
            0,
            "Active Tasks",
            curses.color_pair(1) | curses.A_BOLD,
            max_x,
        )
        row += 1

        active = state.active_tasks()
        now_wall = datetime.now(timezone.utc)
        if not active:
            draw_line(stdscr, row, 2, "(none)", curses.A_DIM, max_x)
            row += 1
        else:
            for task in active[: max(0, (max_y - row - 8) // 2)]:
                draw_line(
                    stdscr,
                    row,
                    2,
                    f"{task.launched_at}  {task.task}  weight={task.weight}  runtime={format_runtime(now_wall, task.launched_wall)}",
                    curses.color_pair(1),
                    max_x,
                )
                row += 1

        row += 1
        draw_line(
            stdscr,
            row,
            0,
            "Recent Completed",
            curses.color_pair(3) | curses.A_BOLD,
            max_x,
        )
        row += 1

        remaining_rows = max_y - row - 3
        for task in state.recent_completed(max(0, remaining_rows)):
            details = f" {task.details}" if task.details else ""
            draw_line(
                stdscr,
                row,
                2,
                f"{task.completed_at or '?'}  {task.task}{details}",
                curses.A_DIM,
                max_x,
            )
            row += 1

        footer = (
            f"last: {state.last_line[: max(0, max_x - 25)]}"
            if state.last_line
            else "last: (none yet)"
        )
        draw_line(
            stdscr,
            max_y - 2,
            0,
            footer,
            curses.color_pair(4),
            max_x,
        )
        draw_line(
            stdscr,
            max_y - 1,
            0,
            "q to quit",
            curses.A_DIM,
            max_x,
        )

        stdscr.timeout(int(refresh_seconds * 1000))
        stdscr.refresh()
        key = stdscr.getch()
        if key in (ord("q"), ord("Q")):
            break


def main() -> int:
    parser = argparse.ArgumentParser(description="Watch an aerobag orchestrator log in curses.")
    parser.add_argument(
        "log_path",
        nargs="?",
        default="/root/aerobag-artifacts/private-work/orchestrator-logs/published-packaged/master.log",
        help="Path to master.log",
    )
    parser.add_argument(
        "--refresh-seconds",
        type=float,
        default=2.0,
        help="Refresh interval in seconds",
    )
    args = parser.parse_args()

    log_path = Path(os.path.expanduser(args.log_path))
    curses.wrapper(run_ui, log_path, args.refresh_seconds)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
