#!/usr/bin/env python3
"""Run a local repo-QA benchmark dataset and emit normalized benchmark JSON."""

from __future__ import annotations

import argparse
import json
from pathlib import Path

from repo_qa_lib import (
    answer_question,
    direct_answer,
    is_correct,
    load_jsonl,
    normalize_prediction,
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dataset", required=True, help="Path to the JSONL question dataset.")
    parser.add_argument("--repo-root", required=True, help="Absolute path to the repo being queried.")
    parser.add_argument("--model", default="qwen3-coder:30b", help="Local Ollama model name.")
    parser.add_argument(
        "--answerer",
        choices=["direct", "llm"],
        default="direct",
        help="Answering backend. `direct` uses deterministic local lookup; `llm` uses the local Ollama chat endpoint.",
    )
    parser.add_argument(
        "--api-base-url",
        default="http://localhost:11434/v1/chat/completions",
        help="OpenAI-compatible Ollama endpoint.",
    )
    parser.add_argument(
        "--output-path",
        default=None,
        help="Optional explicit normalized benchmark JSON output path. "
        "Defaults to $LITKG_BENCHMARK_OUTPUT_PATH.",
    )
    parser.add_argument(
        "--artifact-dir",
        default=None,
        help="Optional explicit artifact dir. Defaults to $LITKG_BENCHMARK_ARTIFACT_DIR.",
    )
    parser.add_argument(
        "--persist-artifact-dir",
        default=None,
        help="Optional stable artifact directory. When set, final answer logs are written here instead of the runner tempdir.",
    )
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    dataset_path = Path(args.dataset).resolve()
    repo_root = Path(args.repo_root).resolve()

    output_path = Path(
        args.output_path
        or Path(
            __import__("os").environ["LITKG_BENCHMARK_OUTPUT_PATH"]
        )
    )
    artifact_dir = Path(
        args.artifact_dir
        or Path(
            __import__("os").environ["LITKG_BENCHMARK_ARTIFACT_DIR"]
        )
    )
    artifact_dir.mkdir(parents=True, exist_ok=True)
    persist_artifact_dir = Path(args.persist_artifact_dir) if args.persist_artifact_dir else artifact_dir
    persist_artifact_dir.mkdir(parents=True, exist_ok=True)

    questions = load_jsonl(dataset_path)
    answer_rows: list[dict[str, object]] = []
    correct_count = 0
    nonempty_count = 0
    total_latency = 0.0
    failures = 0

    for question in questions:
        try:
            if args.answerer == "direct":
                raw_prediction, context = direct_answer(question, repo_root)
                latency_s = 0.0
                prediction = normalize_prediction(raw_prediction)
            else:
                raw_prediction, context, latency_s = answer_question(
                    question=question,
                    repo_root=repo_root,
                    model=args.model,
                    api_base_url=args.api_base_url,
                )
                prediction = normalize_prediction(raw_prediction)
            correct = is_correct(
                prediction,
                question["acceptable_answers"],
                question["answer_kind"],
            )
            diagnostics: list[str] = []
        except Exception as exc:  # noqa: BLE001
            raw_prediction = ""
            prediction = ""
            context = []
            latency_s = 0.0
            correct = False
            failures += 1
            diagnostics = [f"question failed: {type(exc).__name__}: {exc}"]

        if prediction:
            nonempty_count += 1
        if correct:
            correct_count += 1
        total_latency += latency_s

        answer_rows.append(
            {
                "question_id": question["question_id"],
                "question_type": question["question_type"],
                "question": question["question"],
                "expected_answer": question["expected_answer"],
                "acceptable_answers": question["acceptable_answers"],
                "prediction": prediction,
                "raw_prediction": raw_prediction,
                "correct": correct,
                "latency_s": round(latency_s, 4),
                "source_file": question["source_file"],
                "source_line": question["source_line"],
                "context": context,
                "diagnostics": diagnostics,
            }
        )

    total_questions = len(answer_rows)
    exact_match = (correct_count / total_questions) if total_questions else 0.0
    answered_rate = (nonempty_count / total_questions) if total_questions else 0.0
    avg_latency = (total_latency / total_questions) if total_questions else 0.0

    answers_path = persist_artifact_dir / "answers.jsonl"
    summary_path = persist_artifact_dir / "summary.json"
    summary_md_path = persist_artifact_dir / "summary.md"

    with answers_path.open("w", encoding="utf-8") as handle:
        for row in answer_rows:
            handle.write(json.dumps(row, ensure_ascii=False) + "\n")

    summary = {
        "dataset": dataset_path.as_posix(),
        "repo_root": repo_root.as_posix(),
        "model": args.model,
        "question_count": total_questions,
        "exact_match": exact_match,
        "answered_rate": answered_rate,
        "average_latency_s": avg_latency,
        "failure_count": failures,
        "incorrect_questions": [
            row["question_id"] for row in answer_rows if not row["correct"]
        ],
    }
    summary_path.write_text(json.dumps(summary, indent=2), encoding="utf-8")

    incorrect_rows = [row for row in answer_rows if not row["correct"]][:10]
    summary_md_lines = [
        f"# Repo QA Summary: {dataset_path.stem}",
        "",
        f"- Model: `{args.model}`",
        f"- Repo root: `{repo_root}`",
        f"- Questions: `{total_questions}`",
        f"- Exact match: `{exact_match:.3f}`",
        f"- Answered rate: `{answered_rate:.3f}`",
        f"- Average latency (s): `{avg_latency:.3f}`",
        "",
        "## First Incorrect Questions",
        "",
    ]
    if incorrect_rows:
        for row in incorrect_rows:
            summary_md_lines.extend(
                [
                    f"### {row['question_id']}",
                    f"- Question: {row['question']}",
                    f"- Expected: `{row['expected_answer']}`",
                    f"- Predicted: `{row['prediction']}`",
                    "",
                ]
            )
    else:
        summary_md_lines.append("- None")
        summary_md_lines.append("")
    summary_md_path.write_text("\n".join(summary_md_lines), encoding="utf-8")

    payload = {
        "status": "completed",
        "summary": (
            f"Answered {total_questions} local repo-QA trials for {dataset_path.stem} "
            f"with exact_match={exact_match:.3f} and answered_rate={answered_rate:.3f} "
            f"using {args.answerer}."
        ),
        "scores": [
            {"metric_id": "exact_match", "value": exact_match, "unit": "rate"},
            {"metric_id": "answered_rate", "value": answered_rate, "unit": "rate"},
            {
                "metric_id": "average_latency_s",
                "value": avg_latency,
                "unit": "seconds",
            },
        ],
        "diagnostics": [
            f"dataset={dataset_path}",
            f"repo_root={repo_root}",
            f"model={args.model}",
            f"answerer={args.answerer}",
            f"question_count={total_questions}",
            f"failure_count={failures}",
        ],
        "artifacts": [
            {"label": "answers", "kind": "jsonl", "location": answers_path.as_posix()},
            {"label": "summary", "kind": "json", "location": summary_path.as_posix()},
            {
                "label": "summary_markdown",
                "kind": "md",
                "location": summary_md_path.as_posix(),
            },
        ],
    }
    output_path.write_text(json.dumps(payload, indent=2), encoding="utf-8")
    print(json.dumps(payload, indent=2))


if __name__ == "__main__":
    main()
