#!/usr/bin/env python3
"""Small category board for repo-local task files.

This intentionally stays boring:
- one Python stdlib-only server
- reads/writes docs/backlog/tasks/*.md
- category columns are exactly-one cat:* labels
- task state is plain Markdown plus YAML-ish frontmatter
"""

from __future__ import annotations

import argparse
import html
import json
import os
import re
from datetime import datetime
from dataclasses import dataclass
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import parse_qs, urlparse


CATEGORY_COLUMNS = [
    ("preprocessor", "Preprocessor"),
    ("core", "Core"),
    ("web", "Web"),
    ("android", "Android"),
    ("productionization", "Productionization"),
    ("navigation", "Navigation"),
    ("weather", "Weather"),
    ("data", "Data"),
    ("performance", "Performance"),
    ("ui-affordances", "UI Affordances"),
    ("features", "Features"),
]

CATEGORY_KEYS = {key for key, _title in CATEGORY_COLUMNS}
CATEGORY_LABELS = {f"cat:{key}" for key in CATEGORY_KEYS}
TASK_STATES = ["high", "medium", "low", "someday", "done"]
TASK_STATE_RANK = {state: index for index, state in enumerate(TASK_STATES)}


@dataclass
class Task:
    path: Path
    id: str
    title: str
    labels: list[str]
    state: str
    ordinal: int
    description: str


def _parse_scalar(value: str) -> str:
    value = value.strip()
    if len(value) >= 2 and value[0] == value[-1] and value[0] in {"'", '"'}:
        return value[1:-1]
    return value


def parse_frontmatter(text: str) -> tuple[dict[str, object], str]:
    if not text.startswith("---\n"):
        return {}, text
    end = text.find("\n---\n", 4)
    if end < 0:
        return {}, text
    raw = text[4:end].splitlines()
    body = text[end + 5 :]
    data: dict[str, object] = {}
    i = 0
    while i < len(raw):
        line = raw[i]
        if not line.strip():
            i += 1
            continue
        if ":" not in line:
            i += 1
            continue
        key, value = line.split(":", 1)
        key = key.strip()
        value = value.strip()
        if value == "[]":
            data[key] = []
            i += 1
            continue
        if value:
            if value.startswith("[") and value.endswith("]"):
                inner = value[1:-1].strip()
                data[key] = [] if not inner else [_parse_scalar(v) for v in inner.split(",")]
            else:
                data[key] = _parse_scalar(value)
            i += 1
            continue
        items: list[str] = []
        i += 1
        while i < len(raw) and raw[i].startswith("  - "):
            items.append(_parse_scalar(raw[i][4:]))
            i += 1
        data[key] = items
    return data, body


def render_frontmatter(data: dict[str, object]) -> str:
    lines = ["---"]
    for key, value in data.items():
        if isinstance(value, list):
            if value:
                lines.append(f"{key}:")
                for item in value:
                    lines.append(f"  - {item}")
            else:
                lines.append(f"{key}: []")
        else:
            value_s = str(value)
            if key == "created_date" and value_s:
                lines.append(f"{key}: '{value_s}'")
            else:
                lines.append(f"{key}: {value_s}")
    lines.append("---")
    return "\n".join(lines) + "\n"


def extract_description(body: str) -> str:
    match = re.search(
        r"<!-- SECTION:DESCRIPTION:BEGIN -->(.*?)<!-- SECTION:DESCRIPTION:END -->",
        body,
        flags=re.DOTALL,
    )
    if not match:
        return ""
    return match.group(1).replace("\r\n", "\n").replace("\r", "\n").strip()


def replace_description(body: str, description: str) -> str:
    description = description.strip()
    replacement = (
        "<!-- SECTION:DESCRIPTION:BEGIN -->\n"
        f"{description}\n"
        "<!-- SECTION:DESCRIPTION:END -->"
    )
    next_body, count = re.subn(
        r"<!-- SECTION:DESCRIPTION:BEGIN -->.*?<!-- SECTION:DESCRIPTION:END -->",
        replacement,
        body,
        count=1,
        flags=re.DOTALL,
    )
    if count:
        return next_body
    return f"## Description\n\n{replacement}\n"


def normalize_task_state(state: object) -> str:
    value = str(state or "medium").lower()
    return value if value in TASK_STATE_RANK else "medium"


def task_state_from_frontmatter(data: dict[str, object]) -> str:
    return normalize_task_state(data.get("state", ""))


def load_tasks(backlog_dir: Path) -> list[Task]:
    tasks_dir = backlog_dir / "tasks"
    tasks: list[Task] = []
    for path in sorted(tasks_dir.glob("task-*.md")):
        data, body = parse_frontmatter(path.read_text(encoding="utf-8"))
        if not data:
            continue
        labels = data.get("labels", [])
        if not isinstance(labels, list):
            labels = []
        ordinal_raw = str(data.get("ordinal", "999999"))
        try:
            ordinal = int(ordinal_raw)
        except ValueError:
            ordinal = 999999
        tasks.append(
            Task(
                path=path,
                id=str(data.get("id", path.stem)),
                title=str(data.get("title", path.stem)),
                labels=[str(label) for label in labels],
                state=task_state_from_frontmatter(data),
                ordinal=ordinal,
                description=extract_description(body),
            )
        )
    return sorted(tasks, key=lambda t: (TASK_STATE_RANK.get(t.state, 99), t.ordinal, t.id))


def task_column(task: Task) -> str:
    category_error = validate_category_labels(task)
    if category_error:
        return "needs-category"
    return next(label[4:] for label in task.labels if label.startswith("cat:"))


def validate_category_labels(task: Task) -> str | None:
    cat_labels = [label for label in task.labels if label.startswith("cat:")]
    if len(cat_labels) == 0:
        return "missing cat:* label"
    if len(cat_labels) > 1:
        return "multiple cat:* labels: " + ", ".join(cat_labels)
    if cat_labels[0][4:] not in CATEGORY_KEYS:
        return f"unknown category label: {cat_labels[0]}"
    return None


def update_task_category(backlog_dir: Path, task_id: str, category: str) -> None:
    if category not in CATEGORY_KEYS:
        raise ValueError(f"unknown category: {category}")
    matching = [task for task in load_tasks(backlog_dir) if task.id == task_id]
    if not matching:
        raise ValueError(f"unknown task: {task_id}")
    task = matching[0]
    text = task.path.read_text(encoding="utf-8")
    data, body = parse_frontmatter(text)
    labels = data.get("labels", [])
    if not isinstance(labels, list):
        labels = []
    data["labels"] = [str(label) for label in labels if not str(label).startswith("cat:")]
    data["labels"].append(f"cat:{category}")
    data.pop("category", None)
    task.path.write_text(render_frontmatter(data) + body, encoding="utf-8")


def update_task_state(backlog_dir: Path, task_id: str, state: str) -> None:
    state = normalize_task_state(state)
    matching = [task for task in load_tasks(backlog_dir) if task.id == task_id]
    if not matching:
        raise ValueError(f"unknown task: {task_id}")
    task = matching[0]
    text = task.path.read_text(encoding="utf-8")
    data, body = parse_frontmatter(text)
    data["state"] = state
    task.path.write_text(render_frontmatter(data) + body, encoding="utf-8")


def get_task(backlog_dir: Path, task_id: str) -> Task:
    matching = [task for task in load_tasks(backlog_dir) if task.id == task_id]
    if not matching:
        raise ValueError(f"unknown task: {task_id}")
    return matching[0]


def update_task_text(backlog_dir: Path, task_id: str, title: str, description: str) -> None:
    task = get_task(backlog_dir, task_id)
    text = task.path.read_text(encoding="utf-8")
    data, body = parse_frontmatter(text)
    title = title.strip()
    if not title:
        raise ValueError("title cannot be empty")
    data["title"] = title
    body = replace_description(body, description)
    task.path.write_text(render_frontmatter(data) + body, encoding="utf-8")


def slugify(value: str) -> str:
    slug = re.sub(r"[^A-Za-z0-9]+", "-", value.strip()).strip("-")
    return slug or "Untitled"


def create_task(backlog_dir: Path, category: str) -> Task:
    if category not in CATEGORY_KEYS:
        raise ValueError(f"unknown category: {category}")
    tasks_dir = backlog_dir / "tasks"
    max_id = 0
    max_ordinal = 0
    for task in load_tasks(backlog_dir):
        match = re.fullmatch(r"TASK-(\d+)", task.id)
        if match:
            max_id = max(max_id, int(match.group(1)))
        max_ordinal = max(max_ordinal, task.ordinal)
    task_num = max_id + 1
    task_id = f"TASK-{task_num}"
    title = "Untitled"
    data: dict[str, object] = {
        "id": task_id,
        "title": title,
        "assignee": [],
        "created_date": datetime.utcnow().strftime("%Y-%m-%d %H:%M"),
        "labels": [f"cat:{category}"],
        "dependencies": [],
        "state": "medium",
        "ordinal": max_ordinal + 1000,
    }
    body = replace_description("", "")
    path = tasks_dir / f"task-{task_num} - {slugify(title)}.md"
    path.write_text(render_frontmatter(data) + body, encoding="utf-8")
    return get_task(backlog_dir, task_id)


def render_page(tasks: list[Task]) -> bytes:
    columns = [(key, title) for key, title in CATEGORY_COLUMNS]
    columns.append(("needs-category", "Needs Category"))
    grouped: dict[str, list[Task]] = {key: [] for key, _title in columns}
    for task in tasks:
        grouped.setdefault(task_column(task), []).append(task)
    state_counts = {state: 0 for state in TASK_STATES}
    for task in tasks:
        if task.state in state_counts:
            state_counts[task.state] += 1
    state_count_pills = " ".join(
        f'<span class="stateCount state-{html.escape(state)}">{state_counts[state]} {html.escape(state)}</span>'
        for state in TASK_STATES
    )
    body = "\n".join(render_column(key, title, grouped.get(key, [])) for key, title in columns)
    page = f"""<!doctype html>
<html>
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Aerobag Backlog Categories</title>
  <style>
    :root {{
      --bg: #d9edf8;
      --ink: #081923;
      --muted: #5c7280;
      --card: #fffdf6;
      --line: rgba(8, 25, 35, 0.18);
      --blue: #1f70a8;
      --green: #176f4c;
      --red: #a82335;
    }}
    * {{ box-sizing: border-box; }}
    body {{
      margin: 0;
      min-height: 100vh;
      color: var(--ink);
      background: linear-gradient(180deg, #eef8fd 0%, var(--bg) 58%, #b8d6e8 100%);
      font: 14px/1.35 ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
    }}
    header {{
      position: sticky;
      top: 0;
      z-index: 5;
      padding: 14px 18px;
      background: rgba(238, 248, 253, 0.94);
      border-bottom: 1px solid var(--line);
      backdrop-filter: blur(8px);
      display: flex;
      align-items: center;
      gap: 16px;
    }}
    h1 {{
      margin: 0;
      font-size: 18px;
      letter-spacing: 0.02em;
    }}
    .titleEnd {{
      display: flex;
      align-items: center;
      gap: 7px;
      flex-wrap: wrap;
    }}
    .stateCount {{
      border-radius: 999px;
      padding: 3px 8px;
      font-size: 12px;
      font-weight: 850;
      white-space: nowrap;
    }}
    .board {{
      display: grid;
      grid-auto-flow: column;
      grid-auto-columns: minmax(280px, 330px);
      gap: 14px;
      overflow-x: auto;
      align-items: start;
      padding: 14px;
      height: calc(100vh - 58px);
    }}
    .column {{
      border: 1px solid var(--line);
      border-radius: 16px;
      background: rgba(255, 255, 255, 0.36);
      min-height: calc(100vh - 92px);
      overflow: clip;
    }}
    .columnHeader {{
      position: sticky;
      top: 0;
      z-index: 2;
      background: #f7fbfe;
      border-bottom: 1px solid var(--line);
      padding: 10px 12px;
      display: flex;
      justify-content: space-between;
      align-items: center;
      gap: 8px;
      font-weight: 800;
    }}
    .columnTitle {{
      display: flex;
      align-items: baseline;
      gap: 7px;
      min-width: 0;
    }}
    .count {{
      font-weight: 700;
      color: var(--muted);
      font-size: 12px;
    }}
    .newButton {{
      padding: 4px 8px;
      border-radius: 999px;
      font-size: 11px;
      text-transform: uppercase;
      letter-spacing: 0.06em;
      white-space: nowrap;
    }}
    .cards {{
      min-height: calc(100vh - 136px);
      padding: 10px;
    }}
    .column.dropTarget {{
      outline: 4px solid rgba(31, 112, 168, 0.35);
      outline-offset: -4px;
    }}
    .stateDivider {{
      margin: 20px 6px 12px;
      display: flex;
      align-items: center;
      gap: 8px;
      color: var(--muted);
      font-size: 11px;
      font-weight: 900;
      letter-spacing: 0.11em;
      text-transform: uppercase;
    }}
    .stateDivider::before,
    .stateDivider::after {{
      content: "";
      height: 1px;
      flex: 1;
      background: rgba(8, 25, 35, 0.18);
    }}
    .stateDivider.someday {{
      margin-top: 28px;
    }}
    .stateDivider.done {{
      margin-top: 24px;
      opacity: 0.78;
    }}
    .card {{
      background: var(--card);
      border: 1px solid rgba(8, 25, 35, 0.16);
      border-radius: 12px;
      padding: 10px;
      margin-bottom: 10px;
      box-shadow: 0 3px 9px rgba(8, 25, 35, 0.10);
      cursor: pointer;
    }}
    .card.doneCard {{
      background: #cccccc;
      color: rgba(8, 25, 35, 0.76);
      box-shadow: none;
    }}
    .card:active {{ cursor: grabbing; }}
    .taskId {{
      font-weight: 900;
      color: var(--blue);
      margin-right: 6px;
    }}
    .title {{
      font-weight: 750;
      font-size: 14px;
    }}
    .meta {{
      display: flex;
      gap: 6px;
      flex-wrap: wrap;
      margin-top: 8px;
    }}
    .pill {{
      border-radius: 999px;
      padding: 2px 7px;
      background: rgba(31, 112, 168, 0.10);
      color: #174766;
      font-size: 12px;
      font-weight: 700;
    }}
    .state-high {{ background: rgba(168, 35, 53, 0.13); color: var(--red); }}
    .state-medium {{ background: rgba(31, 112, 168, 0.12); color: var(--blue); }}
    .state-low {{ background: rgba(23, 111, 76, 0.12); color: var(--green); }}
    .state-someday {{ background: rgba(92, 114, 128, 0.14); color: var(--muted); }}
    .state-done {{ background: rgba(8, 25, 35, 0.14); color: #26343c; }}
    .stateButton {{
      border-radius: 999px;
      padding: 4px 9px;
      font-size: 12px;
    }}
    .stateMenu {{
      position: fixed;
      z-index: 20;
      display: none;
      grid-template-columns: 1fr;
      gap: 5px;
      padding: 8px;
      border: 1px solid var(--line);
      border-radius: 12px;
      background: #fffdf6;
      box-shadow: 0 10px 28px rgba(8, 25, 35, 0.24);
    }}
    .stateMenu.open {{ display: grid; }}
    .desc {{
      margin-top: 8px;
      color: #233c49;
      font-size: 12px;
      max-height: 4.1em;
      overflow: hidden;
      white-space: pre-wrap;
    }}
    .warning {{
      margin-top: 8px;
      padding: 6px 8px;
      border-radius: 8px;
      background: rgba(168, 35, 53, 0.12);
      color: var(--red);
      font-size: 12px;
      font-weight: 850;
    }}
    button {{
      border: 1px solid var(--line);
      border-radius: 8px;
      background: #f4fbff;
      color: var(--ink);
      font-weight: 800;
      padding: 5px 8px;
      cursor: pointer;
    }}
    button:hover {{ background: #ffffff; }}
    .modalScrim {{
      position: fixed;
      inset: 0;
      z-index: 30;
      display: none;
      align-items: center;
      justify-content: center;
      padding: 24px;
      background: rgba(8, 25, 35, 0.35);
    }}
    .modalScrim.open {{ display: flex; }}
    .editor {{
      width: min(760px, 94vw);
      max-height: min(760px, 88vh);
      border: 1px solid var(--line);
      border-radius: 18px;
      background: #fffdf6;
      box-shadow: 0 16px 50px rgba(8, 25, 35, 0.30);
      padding: 14px;
      display: grid;
      gap: 10px;
    }}
    .editorTitle {{
      width: 100%;
      border: 1px solid var(--line);
      border-radius: 10px;
      padding: 9px 10px;
      font: inherit;
      font-weight: 850;
      background: #fff;
    }}
    .editorTaskId {{
      color: var(--blue);
      font-size: 12px;
      font-weight: 900;
      letter-spacing: 0.06em;
      text-transform: uppercase;
    }}
    .editorBody {{
      width: 100%;
      min-height: 340px;
      resize: vertical;
      border: 1px solid var(--line);
      border-radius: 10px;
      padding: 10px;
      font: 14px/1.4 ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
      background: #fff;
    }}
    .editorActions {{
      display: flex;
      justify-content: flex-end;
      gap: 8px;
    }}
    @media (max-width: 700px) {{
      .board {{ grid-auto-columns: minmax(260px, 86vw); }}
      header {{ align-items: flex-start; flex-direction: column; gap: 4px; }}
    }}
  </style>
</head>
<body>
  <header>
    <h1>Aerobag Bugs</h1>
    <div class="titleEnd">{state_count_pills}</div>
  </header>
  <main class="board">{body}</main>
  <div id="stateMenu" class="stateMenu">
    <button class="stateButton state-high" onclick="setState('high')">high</button>
    <button class="stateButton state-medium" onclick="setState('medium')">medium</button>
    <button class="stateButton state-low" onclick="setState('low')">low</button>
    <button class="stateButton state-someday" onclick="setState('someday')">someday</button>
    <button class="stateButton state-done" onclick="setState('done')">done</button>
  </div>
  <div id="editorScrim" class="modalScrim">
    <div class="editor">
      <div id="editorTaskId" class="editorTaskId"></div>
      <input id="editorTitle" class="editorTitle" type="text" aria-label="Task title">
      <textarea id="editorBody" class="editorBody" aria-label="Task body"></textarea>
      <div class="editorActions">
        <button onclick="closeEditor()">Close</button>
      </div>
    </div>
  </div>
  <script>
    let draggedTaskId = null;
    let stateTaskId = null;
    let editorTaskId = null;
    document.addEventListener('dragstart', (event) => {{
      const card = event.target.closest('.card');
      if (!card) return;
      draggedTaskId = card.dataset.taskId;
      card.dataset.dragging = 'true';
      event.dataTransfer.setData('text/plain', draggedTaskId);
      event.dataTransfer.effectAllowed = 'move';
    }});
    document.addEventListener('dragend', (event) => {{
      const card = event.target.closest('.card');
      if (!card) return;
      setTimeout(() => {{
        delete card.dataset.dragging;
      }}, 0);
    }});
    document.addEventListener('click', (event) => {{
      const card = event.target.closest('.card');
      if (!card || card.dataset.dragging === 'true') return;
      if (event.target.closest('.stateButton')) return;
      openEditor(card.dataset.taskId);
    }});
    document.addEventListener('dragover', (event) => {{
      const column = event.target.closest('.column');
      if (!column) return;
      event.preventDefault();
      column.classList.add('dropTarget');
    }});
    document.addEventListener('dragleave', (event) => {{
      const column = event.target.closest('.column');
      if (column) column.classList.remove('dropTarget');
    }});
    document.addEventListener('drop', async (event) => {{
      const column = event.target.closest('.column');
      if (!column || !draggedTaskId) return;
      event.preventDefault();
      column.classList.remove('dropTarget');
      if (column.dataset.category === 'needs-category') return;
      await post('/api/category', {{taskId: draggedTaskId, category: column.dataset.category}});
      location.reload();
    }});
    document.addEventListener('click', (event) => {{
      const menu = document.getElementById('stateMenu');
      if (!event.target.closest('.stateMenu') && !event.target.closest('.stateButton')) {{
        menu.classList.remove('open');
      }}
    }});
    document.addEventListener('keydown', (event) => {{
      const editorIsOpen = document.getElementById('editorScrim').classList.contains('open');
      if (!editorIsOpen) return;
      if (event.key === 'Escape' || (event.key === 'Enter' && event.ctrlKey)) {{
        event.preventDefault();
        closeEditor();
      }}
    }});
    function openStateMenu(event, taskId) {{
      event.stopPropagation();
      stateTaskId = taskId;
      const menu = document.getElementById('stateMenu');
      menu.style.left = `${{event.clientX}}px`;
      menu.style.top = `${{event.clientY}}px`;
      menu.classList.add('open');
    }}
    async function setState(state) {{
      if (!stateTaskId) return;
      await post('/api/state', {{taskId: stateTaskId, state}});
      location.reload();
    }}
    async function createTask(category) {{
      const response = await fetch('/api/create', {{
        method: 'POST',
        headers: {{'Content-Type': 'application/json'}},
        body: JSON.stringify({{category}}),
      }});
      if (!response.ok) {{
        alert(await response.text());
        return;
      }}
      const task = await response.json();
      await openEditor(task.id);
    }}
    async function openEditor(taskId) {{
      editorTaskId = taskId;
      const response = await fetch(`/api/task?taskId=${{encodeURIComponent(taskId)}}`);
      if (!response.ok) {{
        alert(await response.text());
        return;
      }}
      const task = await response.json();
      document.getElementById('editorTitle').value = task.title;
      document.getElementById('editorTaskId').textContent = task.id;
      document.getElementById('editorBody').value = task.description;
      document.getElementById('editorScrim').classList.add('open');
      document.getElementById('editorTitle').focus();
    }}
    async function closeEditor() {{
      if (editorTaskId) {{
        await post('/api/task', {{
          taskId: editorTaskId,
          title: document.getElementById('editorTitle').value,
          description: document.getElementById('editorBody').value,
        }});
        location.reload();
        return;
      }}
      document.getElementById('editorScrim').classList.remove('open');
    }}
    async function post(url, data) {{
      const response = await fetch(url, {{
        method: 'POST',
        headers: {{'Content-Type': 'application/json'}},
        body: JSON.stringify(data),
      }});
      if (!response.ok) {{
        alert(await response.text());
      }}
    }}
  </script>
</body>
</html>"""
    return page.encode("utf-8")


def render_column(key: str, title: str, tasks: list[Task]) -> str:
    tasks = sorted(tasks, key=lambda task: (TASK_STATE_RANK.get(task.state, 99), task.ordinal, task.id))
    cards = render_state_grouped_cards(tasks)
    new_button = "" if key == "needs-category" else f'<button class="newButton" onclick="createTask(\'{html.escape(key)}\')">New</button>'
    return f"""<section class="column" data-category="{html.escape(key)}">
  <div class="columnHeader"><span class="columnTitle"><span>{html.escape(title)}</span><span class="count">{len(tasks)}</span></span>{new_button}</div>
  <div class="cards">{cards}</div>
</section>"""


def render_state_grouped_cards(tasks: list[Task]) -> str:
    parts: list[str] = []
    seen_someday = False
    seen_done = False
    for task in tasks:
        if task.state == "someday" and not seen_someday:
            parts.append('<div class="stateDivider someday">Someday</div>')
            seen_someday = True
        if task.state == "done" and not seen_done:
            parts.append('<div class="stateDivider done">Done</div>')
            seen_done = True
        parts.append(render_card(task))
    return "\n".join(parts)


def render_card(task: Task) -> str:
    state_class = f"state-{html.escape(task.state)}" if task.state else ""
    card_class = "card doneCard" if task.state == "done" else "card"
    state = (
        f'<button class="stateButton {state_class}" '
        f'onclick="openStateMenu(event, \'{html.escape(task.id)}\')">{html.escape(task.state)}</button>'
    )
    category_error = validate_category_labels(task)
    warning = f'<div class="warning">{html.escape(category_error)}</div>' if category_error else ""
    desc = f'<div class="desc">{html.escape(task.description)}</div>' if task.description else ""
    return f"""<article class="{card_class}" draggable="true" data-task-id="{html.escape(task.id)}">
  <div class="title"><span class="taskId">{html.escape(task.id)}</span>{html.escape(task.title)}</div>
  <div class="meta">{state}</div>
  {warning}
  {desc}
</article>"""


class CategoryBoardHandler(BaseHTTPRequestHandler):
    backlog_dir: Path

    def _send(self, code: int, content_type: str, body: bytes) -> None:
        self.send_response(code)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self) -> None:
        parsed = urlparse(self.path)
        if parsed.path == "/api/task":
            try:
                query = parse_qs(parsed.query)
                task_id = query.get("taskId", [""])[0]
                task = get_task(self.backlog_dir, task_id)
                body = json.dumps(
                    {
                        "id": task.id,
                        "title": task.title,
                        "description": task.description,
                        "state": task.state,
                        "labels": task.labels,
                    }
                ).encode("utf-8")
                self._send(200, "application/json", body)
            except Exception as exc:  # noqa: BLE001 - UI should show the concrete failure.
                self._send(400, "text/plain; charset=utf-8", str(exc).encode("utf-8"))
            return
        if parsed.path not in {"/", "/index.html"}:
            self._send(404, "text/plain; charset=utf-8", b"not found")
            return
        query = parse_qs(parsed.query)
        tasks = load_tasks(self.backlog_dir)
        if "q" in query:
            needle = query["q"][0].lower()
            tasks = [
                task
                for task in tasks
                if needle in task.title.lower()
                or needle in task.id.lower()
                or needle in task.description.lower()
                or any(needle in label.lower() for label in task.labels)
            ]
        self._send(200, "text/html; charset=utf-8", render_page(tasks))

    def do_HEAD(self) -> None:
        parsed = urlparse(self.path)
        if parsed.path not in {"/", "/index.html"}:
            self.send_response(404)
            self.end_headers()
            return
        self.send_response(200)
        self.send_header("Content-Type", "text/html; charset=utf-8")
        self.end_headers()

    def do_POST(self) -> None:
        try:
            length = int(self.headers.get("Content-Length", "0"))
            payload = json.loads(self.rfile.read(length).decode("utf-8"))
            if self.path == "/api/category":
                update_task_category(self.backlog_dir, str(payload["taskId"]), str(payload["category"]))
                self._send(200, "application/json", b'{"ok":true}')
                return
            if self.path == "/api/state":
                update_task_state(self.backlog_dir, str(payload["taskId"]), str(payload["state"]))
                self._send(200, "application/json", b'{"ok":true}')
                return
            if self.path == "/api/task":
                update_task_text(
                    self.backlog_dir,
                    str(payload["taskId"]),
                    str(payload["title"]),
                    str(payload["description"]),
                )
                self._send(200, "application/json", b'{"ok":true}')
                return
            if self.path == "/api/create":
                task = create_task(self.backlog_dir, str(payload["category"]))
                body = json.dumps({"id": task.id}).encode("utf-8")
                self._send(200, "application/json", body)
                return
            self._send(404, "text/plain; charset=utf-8", b"not found")
        except Exception as exc:  # noqa: BLE001 - UI should show the concrete failure.
            self._send(400, "text/plain; charset=utf-8", str(exc).encode("utf-8"))

    def log_message(self, fmt: str, *args: object) -> None:
        print(f"{self.address_string()} - {fmt % args}")


def main() -> None:
    parser = argparse.ArgumentParser(description="Serve a category board for repo-local task files.")
    parser.add_argument("--host", default="0.0.0.0")
    parser.add_argument("--port", type=int, default=6422)
    parser.add_argument("--backlog-dir", default="docs/backlog")
    args = parser.parse_args()

    backlog_dir = Path(args.backlog_dir).resolve()
    if not (backlog_dir / "tasks").is_dir():
        raise SystemExit(f"missing backlog tasks directory: {backlog_dir / 'tasks'}")

    handler = type("Handler", (CategoryBoardHandler,), {"backlog_dir": backlog_dir})
    server = ThreadingHTTPServer((args.host, args.port), handler)
    print(f"Aerobag backlog category board running at http://{args.host}:{args.port}")
    print(f"Reading/writing tasks in {backlog_dir / 'tasks'}")
    server.serve_forever()


if __name__ == "__main__":
    os.chdir(Path(__file__).resolve().parents[1])
    main()
