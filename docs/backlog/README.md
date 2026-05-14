# Aerobag Backlog

This directory is the repo-local task database. It is intentionally file-backed:

- one task per Markdown file in `docs/backlog/tasks/`
- YAML-ish frontmatter for machine-readable fields
- Markdown body for human-readable detail
- no dependency on Backlog.md runtime semantics

## State

Every task has one state:

- `high`
- `medium`
- `low`
- `someday`
- `done`

The category board sorts each category by this state order. The state is edited
directly from the card.

## Category Invariant

Every task must have exactly one category label:

```yaml
labels:
  - bug
  - android
  - cat:android
```

The `cat:*` label is the visual board partition. Other labels are descriptive
tags for filtering/searching. The category board displays tasks with missing,
multiple, or unknown `cat:*` labels in `Needs Category`.

Current category labels:

- `cat:preprocessor`
- `cat:core`
- `cat:web`
- `cat:android`
- `cat:productionization`
- `cat:navigation`
- `cat:weather`
- `cat:data`
- `cat:performance`
- `cat:ui-affordances`
- `cat:features`

## Board

Run the category board from the repo root:

```bash
python3 tools/backlog_category_board.py --port 6422
```

The board reads and writes the task files directly. Dragging a card between
columns rewrites only the `cat:*` label.
