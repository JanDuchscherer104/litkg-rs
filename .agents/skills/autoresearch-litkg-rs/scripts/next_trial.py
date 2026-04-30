#!/usr/bin/env python3
"""Allocate the next autoresearch experiment id and branch name."""

from __future__ import annotations

import argparse
import json
import re
from pathlib import Path

TAG_PATTERN = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]*$")
EXPERIMENT_PREFIX = re.compile(r"^(\d+)-")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Read .logs/autoresearch/<tag>/ state and propose the next experiment id "
            "plus a suggested trial branch name."
        )
    )
    parser.add_argument("--tag", required=True, help="Stable run tag.")
    parser.add_argument(
        "--slug",
        default="candidate",
        help="Short slug to append to the next experiment id.",
    )
    parser.add_argument(
        "--repo-root",
        type=Path,
        default=Path("."),
        help="Repository root where the .logs directory lives.",
    )
    parser.add_argument(
        "--json",
        action="store_true",
        help="Emit the allocation as JSON instead of human-readable text.",
    )
    return parser.parse_args()


def validate_tag(tag: str) -> str:
    if not TAG_PATTERN.fullmatch(tag):
        raise SystemExit(
            "Invalid --tag. Use letters, digits, '.', '_' or '-' only, starting "
            "with a letter or digit."
        )
    return tag


def sanitize_slug(slug: str) -> str:
    cleaned = re.sub(r"[^A-Za-z0-9._-]+", "-", slug).strip("-._")
    return cleaned or "candidate"


def load_json(path: Path) -> dict:
    if not path.exists():
        raise SystemExit(f"Missing file: {path}")
    return json.loads(path.read_text(encoding="utf-8"))


def load_results(results_path: Path) -> list[dict[str, str]]:
    if not results_path.exists():
        raise SystemExit(f"Missing file: {results_path}")
    lines = results_path.read_text(encoding="utf-8").splitlines()
    rows: list[dict[str, str]] = []
    for raw in lines[1:]:
        if not raw.strip():
            continue
        parts = raw.split("\t")
        if len(parts) != 6:
            raise SystemExit(f"Malformed results.tsv row in {results_path}: {raw}")
        rows.append(
            {
                "experiment_id": parts[0],
                "commit": parts[1],
                "status": parts[2],
                "primary_metric": parts[3],
                "guardrail_status": parts[4],
                "description": parts[5],
            }
        )
    return rows


def next_experiment_number(rows: list[dict[str, str]]) -> int:
    highest = 0
    for row in rows:
        match = EXPERIMENT_PREFIX.match(row["experiment_id"])
        if match:
            highest = max(highest, int(match.group(1)))
    return highest + 1


def allocation(tag: str, repo_root: Path, slug: str) -> dict:
    run_dir = repo_root / ".logs" / "autoresearch" / tag
    state = load_json(run_dir / "state.json")
    rows = load_results(run_dir / "results.tsv")
    number = next_experiment_number(rows)
    experiment_id = f"{number:02d}-{sanitize_slug(slug)}"
    payload = {
        "tag": tag,
        "experiment_id": experiment_id,
        "suggested_branch": f"codex/autoresearch-{tag}-trial-{number:02d}",
        "needs_pivot": bool(state.get("needs_pivot")),
        "recommended_action": (
            "pivot before starting this trial"
            if state.get("needs_pivot")
            else "continue with the next trial"
        ),
        "best_experiment_id": state.get("best_experiment_id"),
        "best_commit": state.get("best_commit"),
    }
    return payload


def render_text(payload: dict) -> str:
    lines = [
        f"Run tag: {payload['tag']}",
        f"Next experiment id: {payload['experiment_id']}",
        f"Suggested branch: {payload['suggested_branch']}",
        f"Needs pivot: {'yes' if payload['needs_pivot'] else 'no'}",
        f"Recommended action: {payload['recommended_action']}",
    ]
    if payload.get("best_experiment_id"):
        lines.append(
            f"Current best: {payload['best_experiment_id']} @ {payload['best_commit']}"
        )
    return "\n".join(lines)


def main() -> None:
    args = parse_args()
    tag = validate_tag(args.tag)
    repo_root = args.repo_root.expanduser().resolve()
    payload = allocation(tag, repo_root, args.slug)
    if args.json:
        print(json.dumps(payload, indent=2, sort_keys=True))
    else:
        print(render_text(payload))


if __name__ == "__main__":
    main()
