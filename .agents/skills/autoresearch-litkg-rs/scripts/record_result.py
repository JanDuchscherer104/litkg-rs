"""Append an autoresearch result row and update resumable run state."""

from __future__ import annotations

import argparse
import json
import re
from datetime import datetime, timezone
from pathlib import Path

RESULTS_HEADER = (
    "experiment_id\tcommit\tstatus\tprimary_metric\tguardrail_status\tdescription\n"
)
TAG_PATTERN = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]*$")
VALID_STATUSES = {"baseline", "keep", "discard", "crash"}


def parse_args() -> argparse.Namespace:
    """Return validated CLI arguments."""
    parser = argparse.ArgumentParser(
        description=(
            "Append a result row to .logs/autoresearch/<tag>/results.tsv and "
            "update state.json."
        )
    )
    parser.add_argument("--tag", required=True, help="Stable run tag.")
    parser.add_argument(
        "--experiment-id",
        required=True,
        help="Experiment identifier, for example 00-baseline or 03-ordering-guard.",
    )
    parser.add_argument(
        "--commit",
        required=True,
        help="Short git commit for the trial result, or a stable marker like '-'.",
    )
    parser.add_argument(
        "--status",
        choices=sorted(VALID_STATUSES),
        required=True,
        help="One of baseline, keep, discard, or crash.",
    )
    parser.add_argument(
        "--primary-metric",
        required=True,
        help="Primary metric outcome for this experiment.",
    )
    parser.add_argument(
        "--guardrail-status",
        default="n/a",
        help="Guard status summary, for example pass or fail:cargo test -p litkg-core.",
    )
    parser.add_argument(
        "--description",
        required=True,
        help="Exact experiment hypothesis or result description.",
    )
    parser.add_argument(
        "--set-best",
        action="store_true",
        help="Mark this result as the current best known run state.",
    )
    parser.add_argument(
        "--repo-root",
        type=Path,
        default=Path("."),
        help="Repository root where the .logs directory lives.",
    )
    return parser.parse_args()


def utc_now() -> str:
    """Return a stable UTC timestamp."""
    return datetime.now(timezone.utc).isoformat(timespec="seconds").replace(
        "+00:00", "Z"
    )


def load_state(state_path: Path) -> dict:
    """Return the existing state payload."""
    if not state_path.exists():
        raise SystemExit(f"Missing state file: {state_path}")
    return json.loads(state_path.read_text(encoding="utf-8"))


def sanitize_field(value: str) -> str:
    """Keep TSV fields single-line and tab-safe."""
    return value.replace("\t", " ").replace("\n", " ").strip()


def validate_tag(tag: str) -> str:
    """Reject tags that could escape the intended log directory."""
    if not TAG_PATTERN.fullmatch(tag):
        raise SystemExit(
            "Invalid --tag. Use letters, digits, '.', '_' or '-' only, starting "
            "with a letter or digit."
        )
    return tag


def load_results(results_path: Path) -> tuple[list[str], set[str]]:
    """Return existing TSV lines and recorded experiment ids."""
    contents = results_path.read_text(encoding="utf-8").splitlines()
    if not contents or contents[0] != RESULTS_HEADER.rstrip("\n"):
        raise SystemExit(f"Unexpected results.tsv header in {results_path}")

    experiment_ids: set[str] = set()
    for raw_line in contents[1:]:
        if not raw_line.strip():
            continue
        parts = raw_line.split("\t")
        if len(parts) != 6:
            raise SystemExit(f"Malformed results.tsv row in {results_path}: {raw_line}")
        experiment_id = sanitize_field(parts[0])
        if experiment_id in experiment_ids:
            raise SystemExit(
                f"Duplicate experiment_id already present in {results_path}: {experiment_id}"
            )
        experiment_ids.add(experiment_id)
    return contents, experiment_ids


def atomic_write(path: Path, contents: str) -> None:
    """Replace a file atomically with new contents."""
    tmp_path = path.with_name(f"{path.name}.tmp")
    tmp_path.write_text(contents, encoding="utf-8")
    tmp_path.replace(path)


def replace_with_rollback(
    first_path: Path,
    first_contents: str,
    second_path: Path,
    second_contents: str,
) -> None:
    """Best-effort two-file update with rollback if the second replace fails."""
    old_first = first_path.read_text(encoding="utf-8")
    atomic_write(first_path, first_contents)
    try:
        atomic_write(second_path, second_contents)
    except Exception:
        atomic_write(first_path, old_first)
        raise


def main() -> None:
    """Append the result row and update the resumable state file."""
    args = parse_args()
    if args.set_best and args.status not in {"baseline", "keep"}:
        raise SystemExit("--set-best is only valid for baseline or keep results.")

    tag = validate_tag(args.tag)
    repo_root = args.repo_root.expanduser().resolve()
    run_dir = repo_root / ".logs" / "autoresearch" / tag
    results_path = run_dir / "results.tsv"
    state_path = run_dir / "state.json"
    if not results_path.exists():
        raise SystemExit(f"Missing results file: {results_path}")

    existing_lines, experiment_ids = load_results(results_path)
    experiment_id = sanitize_field(args.experiment_id)
    if not experiment_id:
        raise SystemExit("--experiment-id must not be empty after normalization.")
    commit = sanitize_field(args.commit)
    primary_metric = sanitize_field(args.primary_metric)
    guardrail_status = sanitize_field(args.guardrail_status)
    description = sanitize_field(args.description)

    if experiment_id in experiment_ids:
        raise SystemExit(
            f"Experiment id already recorded in {results_path}: {experiment_id}"
        )
    if args.status == "keep" and not guardrail_status.startswith("pass"):
        raise SystemExit("A keep result must have guardrail_status starting with 'pass'.")
    state = load_state(state_path)

    row = [
        experiment_id,
        commit,
        args.status,
        primary_metric,
        guardrail_status,
        description,
    ]
    counts = state.setdefault("counts", {})
    for status in VALID_STATUSES:
        counts.setdefault(status, 0)
    counts[args.status] += 1

    if args.status in {"baseline", "keep"}:
        consecutive_non_keep = 0
    else:
        consecutive_non_keep = int(state.get("consecutive_non_keep", 0)) + 1

    state["updated_at_utc"] = utc_now()
    state["last_result"] = {
        "experiment_id": experiment_id,
        "commit": commit,
        "status": args.status,
        "primary_metric": primary_metric,
        "guardrail_status": guardrail_status,
        "description": description,
    }
    state["consecutive_non_keep"] = consecutive_non_keep
    state["needs_pivot"] = consecutive_non_keep >= 3
    state["status"] = "needs-pivot" if state["needs_pivot"] else "active"

    should_update_best = False
    if args.status == "keep":
        should_update_best = True
    elif args.status == "baseline" and (
        args.set_best or state.get("best_experiment_id") is None
    ):
        should_update_best = True

    if should_update_best:
        state["best_experiment_id"] = experiment_id
        state["best_commit"] = commit
        state["best_primary_metric"] = primary_metric

    new_results_lines = existing_lines + ["\t".join(row)]
    replace_with_rollback(
        results_path,
        "\n".join(new_results_lines) + "\n",
        state_path,
        json.dumps(state, indent=2, sort_keys=True) + "\n",
    )

    print(results_path)
    print(state_path)


if __name__ == "__main__":
    main()
