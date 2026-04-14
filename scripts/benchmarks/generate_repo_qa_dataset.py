#!/usr/bin/env python3
"""Generate deterministic local repo-QA benchmark questions."""

from __future__ import annotations

import argparse
from pathlib import Path

from repo_qa_lib import generate_questions, write_jsonl


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo-id", required=True, help="Stable benchmark repo id, e.g. nbv.")
    parser.add_argument("--repo-root", required=True, help="Absolute path to the target repository.")
    parser.add_argument("--output", required=True, help="JSONL output path.")
    parser.add_argument("--trials", type=int, default=25, help="Number of questions to generate.")
    parser.add_argument("--seed", type=int, default=0, help="Deterministic selection seed.")
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    repo_root = Path(args.repo_root).resolve()
    rows = generate_questions(
        repo_id=args.repo_id,
        repo_root=repo_root,
        trials=args.trials,
        seed=args.seed,
    )
    write_jsonl(Path(args.output), rows)
    print(f"Wrote {len(rows)} questions to {args.output}")


if __name__ == "__main__":
    main()
