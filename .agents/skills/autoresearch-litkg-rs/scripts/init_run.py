"""Initialize a bounded autoresearch run log for litkg-rs."""

from __future__ import annotations

import argparse
import json
import platform
import re
import shutil
import subprocess
from datetime import datetime, timezone
from pathlib import Path

RESULTS_HEADER = (
    "experiment_id\tcommit\tstatus\tprimary_metric\tguardrail_status\tdescription\n"
)
TAG_PATTERN = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]*$")


def parse_args() -> argparse.Namespace:
    """Return validated CLI arguments."""
    parser = argparse.ArgumentParser(
        description=(
            "Create .logs/autoresearch/<tag>/brief.md, results.tsv, and "
            "state.json for a litkg-rs autoresearch loop."
        )
    )
    parser.add_argument(
        "--tag",
        required=True,
        help="Stable run tag, for example 2026-04-13-kg-navigation.",
    )
    parser.add_argument(
        "--question",
        required=True,
        help="Research question for this run.",
    )
    parser.add_argument(
        "--primary-metric",
        required=True,
        help="Primary winner metric for keep/discard decisions.",
    )
    parser.add_argument(
        "--direction",
        default="n/a",
        help="How to read the primary metric, e.g. lower, higher, pass/fail, rubric.",
    )
    parser.add_argument(
        "--secondary-metric",
        default="none",
        help="Optional secondary metric or guardrail summary.",
    )
    parser.add_argument(
        "--verify-cmd",
        "--evaluation-cmd",
        dest="verify_cmd",
        action="append",
        default=[],
        help="Repeatable frozen verify command.",
    )
    parser.add_argument(
        "--guard-cmd",
        action="append",
        default=[],
        help="Repeatable frozen guard command.",
    )
    parser.add_argument(
        "--mutable",
        action="append",
        default=[],
        help="Repeatable mutable file or glob surface.",
    )
    parser.add_argument(
        "--immutable",
        action="append",
        default=[],
        help="Repeatable immutable file or glob surface.",
    )
    parser.add_argument(
        "--catalog",
        default="examples/benchmarks/kg.toml",
        help="Benchmark catalog path to freeze in the brief.",
    )
    parser.add_argument(
        "--results-bundle",
        default="examples/benchmarks/sample-results.toml",
        help="Benchmark results bundle path to freeze in the brief.",
    )
    parser.add_argument(
        "--target-id",
        default="",
        help="Optional render-autoresearch-target id for benchmark-driven runs.",
    )
    parser.add_argument(
        "--target-format",
        default="markdown",
        help="Render format to freeze for benchmark-driven target runs.",
    )
    parser.add_argument(
        "--benchmark-id",
        action="append",
        default=[],
        help="Repeatable benchmark id relevant to this run.",
    )
    parser.add_argument(
        "--component-id",
        action="append",
        default=[],
        help="Repeatable autoresearch component id relevant to this run.",
    )
    parser.add_argument(
        "--branch",
        default="",
        help="Optional dedicated winner-branch name. Defaults to codex/autoresearch-<tag>.",
    )
    parser.add_argument(
        "--max-experiments",
        type=int,
        default=0,
        help="Optional upper bound on experiments. Zero means unspecified.",
    )
    parser.add_argument(
        "--time-budget",
        default="",
        help="Optional run time budget, for example 4h.",
    )
    parser.add_argument(
        "--stop-when",
        default="budget exhausted or the run no longer produces credible keeps",
        help="Explicit stop condition for the run.",
    )
    parser.add_argument(
        "--repo-root",
        type=Path,
        default=Path("."),
        help="Repository root where the .logs directory should be created.",
    )
    return parser.parse_args()


def render_bullets(items: list[str], *, empty_text: str) -> str:
    """Render one markdown bullet list."""
    if not items:
        return f"- {empty_text}\n"
    return "".join(f"- {item}\n" for item in items)


def utc_now() -> str:
    """Return a stable UTC timestamp."""
    return datetime.now(timezone.utc).isoformat(timespec="seconds").replace(
        "+00:00", "Z"
    )


def git_output(repo_root: Path, *args: str) -> str:
    """Return git output for repo_root, or an empty string when unavailable."""
    try:
        result = subprocess.run(
            ["git", *args],
            cwd=repo_root,
            check=True,
            capture_output=True,
            text=True,
        )
    except (FileNotFoundError, subprocess.CalledProcessError):
        return ""
    return result.stdout.strip()


def validate_tag(tag: str) -> str:
    """Reject tags that could escape the intended log directory."""
    if not TAG_PATTERN.fullmatch(tag):
        raise SystemExit(
            "Invalid --tag. Use letters, digits, '.', '_' or '-' only, starting "
            "with a letter or digit."
        )
    return tag


def main() -> None:
    """Create the research brief and TSV header."""
    args = parse_args()
    tag = validate_tag(args.tag)
    repo_root = args.repo_root.expanduser().resolve()
    run_dir = repo_root / ".logs" / "autoresearch" / tag
    if run_dir.exists():
        raise SystemExit(f"Refusing to overwrite existing run directory: {run_dir}")

    run_dir.mkdir(parents=True, exist_ok=False)
    branch = args.branch or f"codex/autoresearch-{tag}"
    created_at = utc_now()
    starting_commit = git_output(repo_root, "rev-parse", "--short", "HEAD")
    starting_branch = git_output(repo_root, "branch", "--show-current")
    platform_summary = {
        "system": platform.system(),
        "machine": platform.machine(),
        "platform": platform.platform(),
    }

    brief_path = run_dir / "brief.md"
    results_path = run_dir / "results.tsv"
    state_path = run_dir / "state.json"

    try:
        brief_path.write_text(
            "\n".join(
                [
                    f"# Autoresearch Run: {tag}",
                    "",
                    f"- Created at (UTC): `{created_at}`",
                    f"- Platform: `{platform_summary['platform']}`",
                    (
                        f"- Starting branch: `{starting_branch}`"
                        if starting_branch
                        else "- Starting branch: unavailable"
                    ),
                    (
                        f"- Starting commit: `{starting_commit}`"
                        if starting_commit
                        else "- Starting commit: unavailable"
                    ),
                    f"- Branch: `{branch}`",
                    f"- Question: {args.question}",
                    f"- Primary metric: `{args.primary_metric}`",
                    f"- Direction: `{args.direction}`",
                    f"- Secondary metric: `{args.secondary_metric}`",
                    "",
                    "## Verify Commands",
                    "",
                    render_bullets(
                        args.verify_cmd,
                        empty_text="Fill in the frozen verify commands.",
                    ),
                    "## Guard Commands",
                    "",
                    render_bullets(
                        args.guard_cmd,
                        empty_text="Fill in the frozen guard commands.",
                    ),
                    "## Mutable Surface",
                    "",
                    render_bullets(
                        args.mutable,
                        empty_text="Fill in the editable file/module surface.",
                    ),
                    "## Immutable Surface",
                    "",
                    render_bullets(
                        args.immutable,
                        empty_text="Fill in the frozen docs/contracts/evaluator surface.",
                    ),
                    "## Budget And Stop Conditions",
                    "",
                    (
                        f"- Max experiments: `{args.max_experiments}`"
                        if args.max_experiments > 0
                        else "- Max experiments: unspecified"
                    ),
                    (
                        f"- Time budget: `{args.time_budget}`"
                        if args.time_budget
                        else "- Time budget: unspecified"
                    ),
                    f"- Stop when: {args.stop_when}",
                    "",
                    "## Benchmark Context",
                    "",
                    f"- Catalog: `{args.catalog}`",
                    f"- Results bundle: `{args.results_bundle}`",
                    (
                        f"- Target id: `{args.target_id}`"
                        if args.target_id
                        else "- Target id: none"
                    ),
                    f"- Target format: `{args.target_format}`",
                    "",
                    "### Benchmark Ids",
                    "",
                    render_bullets(
                        args.benchmark_id,
                        empty_text="Fill in the benchmark ids for this run.",
                    ),
                    "### Component Ids",
                    "",
                    render_bullets(
                        args.component_id,
                        empty_text="Fill in the autoresearch component ids for this run.",
                    ),
                    "## Baseline Protocol",
                    "",
                    "- Run the frozen verify and guard commands on the untouched winner branch.",
                    "- Record the baseline in `results.tsv` before the first experimental edit.",
                    "- Do not continue if the baseline is already failing.",
                ]
            ),
            encoding="utf-8",
        )
        results_path.write_text(RESULTS_HEADER, encoding="utf-8")
        state_path.write_text(
            json.dumps(
                {
                    "tag": tag,
                    "created_at_utc": created_at,
                    "updated_at_utc": created_at,
                    "status": "initialized",
                    "question": args.question,
                    "branch": branch,
                    "starting_branch": starting_branch or None,
                    "starting_commit": starting_commit or None,
                    "primary_metric": args.primary_metric,
                    "direction": args.direction,
                    "secondary_metric": args.secondary_metric,
                    "verify_commands": args.verify_cmd,
                    "guard_commands": args.guard_cmd,
                    "mutable": args.mutable,
                    "immutable": args.immutable,
                    "budget": {
                        "max_experiments": args.max_experiments or None,
                        "time_budget": args.time_budget or None,
                        "stop_when": args.stop_when,
                    },
                    "benchmark_context": {
                        "catalog": args.catalog,
                        "results_bundle": args.results_bundle,
                        "target_id": args.target_id or None,
                        "target_format": args.target_format,
                        "benchmark_ids": args.benchmark_id,
                        "component_ids": args.component_id,
                    },
                    "platform": platform_summary,
                    "best_experiment_id": None,
                    "best_commit": None,
                    "best_primary_metric": None,
                    "counts": {
                        "baseline": 0,
                        "keep": 0,
                        "discard": 0,
                        "crash": 0,
                    },
                    "consecutive_non_keep": 0,
                    "needs_pivot": False,
                    "last_result": None,
                },
                indent=2,
                sort_keys=True,
            )
            + "\n",
            encoding="utf-8",
        )
    except Exception:
        shutil.rmtree(run_dir, ignore_errors=True)
        raise

    print(brief_path)
    print(results_path)
    print(state_path)


if __name__ == "__main__":
    main()
