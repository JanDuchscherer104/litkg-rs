"""Summarize a resumable autoresearch run and recommend the next move."""

from __future__ import annotations

import argparse
import json
import re
from pathlib import Path

TAG_PATTERN = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]*$")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Read .logs/autoresearch/<tag>/ state and print a resume summary."
    )
    parser.add_argument("--tag", required=True, help="Stable run tag.")
    parser.add_argument(
        "--repo-root",
        type=Path,
        default=Path("."),
        help="Repository root where the .logs directory lives.",
    )
    parser.add_argument(
        "--json",
        action="store_true",
        help="Emit the summary as JSON instead of human-readable text.",
    )
    return parser.parse_args()


def validate_tag(tag: str) -> str:
    if not TAG_PATTERN.fullmatch(tag):
        raise SystemExit(
            "Invalid --tag. Use letters, digits, '.', '_' or '-' only, starting "
            "with a letter or digit."
        )
    return tag


def load_json(path: Path) -> dict:
    if not path.exists():
        raise SystemExit(f"Missing file: {path}")
    return json.loads(path.read_text(encoding="utf-8"))


def load_recent_results(results_path: Path, limit: int = 5) -> list[dict[str, str]]:
    if not results_path.exists():
        raise SystemExit(f"Missing file: {results_path}")
    lines = results_path.read_text(encoding="utf-8").splitlines()
    if not lines:
        return []
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
    return rows[-limit:]


def summarize_run(tag: str, repo_root: Path) -> dict:
    run_dir = repo_root / ".logs" / "autoresearch" / tag
    state = load_json(run_dir / "state.json")
    recent_results = load_recent_results(run_dir / "results.tsv")

    recommended_action = (
        "pivot before the next trial"
        if state.get("needs_pivot")
        else "continue from the winner branch"
    )
    summary = {
        "tag": tag,
        "branch": state.get("branch"),
        "status": state.get("status"),
        "best_experiment_id": state.get("best_experiment_id"),
        "best_commit": state.get("best_commit"),
        "best_primary_metric": state.get("best_primary_metric"),
        "consecutive_non_keep": state.get("consecutive_non_keep", 0),
        "needs_pivot": bool(state.get("needs_pivot")),
        "recommended_action": recommended_action,
        "last_result": state.get("last_result"),
        "recent_results": recent_results,
    }
    return summary


def render_text(summary: dict) -> str:
    lines = [
        f"Run tag: {summary['tag']}",
        f"Branch: {summary.get('branch') or 'unknown'}",
        f"Status: {summary.get('status') or 'unknown'}",
        (
            f"Best: {summary['best_experiment_id']} @ {summary['best_commit']} "
            f"(metric={summary['best_primary_metric']})"
            if summary.get("best_experiment_id")
            else "Best: none recorded"
        ),
        f"Consecutive non-keep: {summary.get('consecutive_non_keep', 0)}",
        f"Recommended action: {summary['recommended_action']}",
    ]
    last_result = summary.get("last_result")
    if last_result:
        lines.extend(
            [
                "Last result:",
                f"- {last_result['experiment_id']} [{last_result['status']}]",
                f"- metric={last_result['primary_metric']}",
                f"- guard={last_result['guardrail_status']}",
                f"- {last_result['description']}",
            ]
        )
    recent_results = summary.get("recent_results") or []
    if recent_results:
        lines.append("Recent results:")
        for row in recent_results:
            lines.append(
                f"- {row['experiment_id']} [{row['status']}] metric={row['primary_metric']} guard={row['guardrail_status']}"
            )
    return "\n".join(lines)


def main() -> None:
    args = parse_args()
    tag = validate_tag(args.tag)
    repo_root = args.repo_root.expanduser().resolve()
    summary = summarize_run(tag, repo_root)
    if args.json:
        print(json.dumps(summary, indent=2, sort_keys=True))
    else:
        print(render_text(summary))


if __name__ == "__main__":
    main()
