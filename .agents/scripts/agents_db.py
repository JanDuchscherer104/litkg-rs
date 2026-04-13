from __future__ import annotations

import argparse
import json
import tomllib
from datetime import date
from pathlib import Path
from typing import Any, Literal

Kind = Literal["issue", "todo"]
Format = Literal["text", "json"]

REPO_ROOT = Path(__file__).resolve().parents[2]
AGENTS_DIR = REPO_ROOT / ".agents"
ISSUES_PATH = AGENTS_DIR / "issues.toml"
TODOS_PATH = AGENTS_DIR / "todos.toml"
RESOLVED_PATH = AGENTS_DIR / "resolved.toml"

PRIORITY_ORDER = {"critical": 4, "high": 3, "medium": 2, "low": 1}
ISSUE_STATUS_ORDER = {"open": 3, "in_progress": 2, "blocked": 1, "closed": 0}
TODO_STATUS_ORDER = {"pending": 3, "in_progress": 2, "blocked": 1, "done": 0}


def load_toml(path: Path) -> dict[str, Any]:
    with path.open("rb") as handle:
        return tomllib.load(handle)


def _priority_value(priority: str) -> int:
    return PRIORITY_ORDER.get(priority, 0)


def _issue_status_value(status: str) -> int:
    return ISSUE_STATUS_ORDER.get(status, 0)


def _todo_status_value(status: str) -> int:
    return TODO_STATUS_ORDER.get(status, 0)


def validate_todo_record(todo: dict[str, Any]) -> None:
    required = ("loc_min", "loc_expected", "loc_max")
    missing = [field for field in required if field not in todo]
    if missing:
        raise ValueError(f"Todo {todo.get('id', '<unknown>')} missing LOC fields: {', '.join(missing)}")
    loc_min = todo["loc_min"]
    loc_expected = todo["loc_expected"]
    loc_max = todo["loc_max"]
    if not all(isinstance(value, int) for value in (loc_min, loc_expected, loc_max)):
        raise ValueError(f"Todo {todo.get('id', '<unknown>')} must use integer LOC estimates.")
    if not (0 <= loc_min <= loc_expected <= loc_max):
        raise ValueError(f"Todo {todo.get('id', '<unknown>')} must satisfy 0 <= loc_min <= loc_expected <= loc_max.")


def rank_issues(issues: list[dict[str, Any]]) -> list[dict[str, Any]]:
    return sorted(
        issues,
        key=lambda issue: (
            -_priority_value(str(issue.get("priority", ""))),
            -_issue_status_value(str(issue.get("status", ""))),
            str(issue.get("id", "")),
        ),
    )


def rank_todos(todos: list[dict[str, Any]]) -> list[dict[str, Any]]:
    for todo in todos:
        validate_todo_record(todo)
    return sorted(
        todos,
        key=lambda todo: (
            -_priority_value(str(todo.get("priority", ""))),
            -_todo_status_value(str(todo.get("status", ""))),
            int(todo["loc_expected"]),
            int(todo["loc_min"]),
            str(todo.get("id", "")),
        ),
    )


def build_ranked_view() -> dict[str, list[dict[str, Any]]]:
    issues_data = load_toml(ISSUES_PATH)
    todos_data = load_toml(TODOS_PATH)
    resolved_data = load_toml(RESOLVED_PATH)
    resolved_issues = {str(item) for item in resolved_data.get("resolved_issues", [])}
    resolved_todos = {str(item) for item in resolved_data.get("resolved_todos", [])}
    active_issues = [
        issue
        for issue in issues_data.get("issues", [])
        if issue.get("status") != "closed" and issue.get("id") not in resolved_issues
    ]
    active_todos = [
        todo
        for todo in todos_data.get("todos", [])
        if todo.get("status") != "done" and todo.get("id") not in resolved_todos
    ]
    return {
        "issues": rank_issues(list(active_issues)),
        "todos": rank_todos(list(active_todos)),
    }


def render_ranked_text(kind: str, ranked: dict[str, list[dict[str, Any]]], limit: int | None) -> str:
    lines: list[str] = []
    if kind in {"issues", "all"}:
        lines.append("Issues")
        issue_items = ranked["issues"][:limit] if limit is not None else ranked["issues"]
        for index, issue in enumerate(issue_items, start=1):
            github_number = issue.get("github_issue_number")
            github_suffix = f" gh=#{github_number}" if github_number is not None else ""
            lines.append(
                f"{index}. {issue['id']} [{issue['priority']}/{issue['status']}] {issue['title']}{github_suffix}"
            )
            lines.append(f"   {issue['summary']}")
    if kind in {"todos", "all"}:
        if lines:
            lines.append("")
        lines.append("Todos")
        todo_items = ranked["todos"][:limit] if limit is not None else ranked["todos"]
        for index, todo in enumerate(todo_items, start=1):
            loc_triplet = f"{todo['loc_min']}/{todo['loc_expected']}/{todo['loc_max']}"
            lines.append(
                f"{index}. {todo['id']} [{todo['priority']}/{todo['status']}] loc={loc_triplet} {todo['title']}"
            )
            lines.append(f"   issues={', '.join(todo.get('issue_ids', []))}")
    return "\n".join(lines)


def _parse_args(argv: list[str] | None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Manage the local litkg-rs .agents backlog.")
    subparsers = parser.add_subparsers(dest="command", required=True)

    rank_parser = subparsers.add_parser("rank", help="Show ranked active issues and todos.")
    rank_parser.add_argument("--kind", choices=("issues", "todos", "all"), default="all")
    rank_parser.add_argument("--format", choices=("text", "json"), default="text")
    rank_parser.add_argument("--limit", type=int, default=None)
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = _parse_args(argv)
    if args.command == "rank":
        ranked = build_ranked_view()
        output_format: Format = args.format
        if output_format == "json":
            payload = ranked if args.kind == "all" else {args.kind: ranked[args.kind]}
            print(json.dumps(payload, indent=2))
        else:
            print(render_ranked_text(args.kind, ranked, args.limit))
        return 0
    raise ValueError(f"Unsupported command: {args.command}")


if __name__ == "__main__":
    raise SystemExit(main())
