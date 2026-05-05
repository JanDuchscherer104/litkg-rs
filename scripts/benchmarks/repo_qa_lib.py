#!/usr/bin/env python3
"""Shared helpers for deterministic local repo-QA benchmark assets."""

from __future__ import annotations

import json
import random
import re
import subprocess
import time
import urllib.error
import urllib.request
from pathlib import Path
from typing import Any


STOPWORDS = {
    "a",
    "an",
    "and",
    "as",
    "at",
    "be",
    "by",
    "for",
    "from",
    "in",
    "is",
    "it",
    "of",
    "on",
    "or",
    "reply",
    "shown",
    "that",
    "the",
    "to",
    "using",
    "what",
    "which",
    "with",
}


def load_jsonl(path: Path) -> list[dict[str, Any]]:
    rows: list[dict[str, Any]] = []
    if not path.exists():
        return rows
    with path.open(encoding="utf-8") as handle:
        for line in handle:
            line = line.strip()
            if line:
                rows.append(json.loads(line))
    return rows


def write_jsonl(path: Path, rows: list[dict[str, Any]]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8") as handle:
        for row in rows:
            handle.write(json.dumps(row, ensure_ascii=False) + "\n")


def repo_source_prefixes(repo_id: str, repo_root: Path) -> list[str]:
    if repo_id == "nbv":
        return [
            "aria_nbv/aria_nbv/",
            "aria_nbv/main.py",
            "scripts/",
        ]
    if repo_id == "prml-vslam":
        return [
            "src/prml_vslam/",
            "streamlit_app.py",
            "scripts/",
        ]

    prefixes: list[str] = []
    if (repo_root / "src").exists():
        prefixes.append("src/")
    if (repo_root / "scripts").exists():
        prefixes.append("scripts/")
    return prefixes


def _path_allowed(rel_path: str, prefixes: list[str]) -> bool:
    if any(
        part in rel_path
        for part in (
            "/tests/",
            "/test_",
            "/external/",
            "/.agents/",
            "/docs/",
            "__pycache__",
        )
    ):
        return False
    if rel_path.startswith("tests/") or rel_path.startswith("external/") or rel_path.startswith(".agents/"):
        return False
    return any(rel_path.startswith(prefix) for prefix in prefixes)


def parse_python_symbols(repo_id: str, repo_root: Path) -> list[dict[str, Any]]:
    prefixes = repo_source_prefixes(repo_id, repo_root)
    by_symbol: dict[tuple[str, str], list[dict[str, Any]]] = {}

    for path in repo_root.rglob("*.py"):
        rel_path = path.relative_to(repo_root).as_posix()
        if not _path_allowed(rel_path, prefixes):
            continue
        try:
            text = path.read_text(encoding="utf-8", errors="ignore")
        except OSError:
            continue
        for lineno, line in enumerate(text.splitlines(), start=1):
            match = re.match(r"^(def|class)\s+([A-Za-z_][A-Za-z0-9_]*)\b", line)
            if not match:
                continue
            kind, name = match.groups()
            if name.startswith("_") or len(name) < 4:
                continue
            by_symbol.setdefault((kind, name), []).append(
                {
                    "fact_type": "symbol_definition",
                    "question_type": "symbol_definition",
                    "answer_kind": "path",
                    "symbol_kind": kind,
                    "symbol_name": name,
                    "expected_answer": rel_path,
                    "acceptable_answers": [rel_path],
                    "source_file": rel_path,
                    "source_line": lineno,
                    "source_excerpt": line.strip(),
                    "retrieval_query": name,
                }
            )

    unique_facts: list[dict[str, Any]] = []
    for entries in by_symbol.values():
        if len(entries) == 1:
            unique_facts.extend(entries)

    unique_facts.sort(key=lambda item: (item["symbol_name"], item["source_file"]))
    return unique_facts


def parse_make_targets(repo_root: Path) -> list[dict[str, Any]]:
    path = repo_root / "Makefile"
    if not path.exists():
        return []
    text = path.read_text(encoding="utf-8", errors="ignore")
    facts: list[dict[str, Any]] = []
    for lineno, line in enumerate(text.splitlines(), start=1):
        match = re.match(r"^([A-Za-z0-9_.-]+):.*?##\s+(.*)$", line)
        if not match:
            continue
        target, description = match.groups()
        if target.startswith("_"):
            continue
        facts.append(
            {
                "fact_type": "make_target",
                "question_type": "make_target",
                "answer_kind": "target",
                "target": target,
                "description": description.strip(),
                "expected_answer": target,
                "acceptable_answers": [target],
                "source_file": "Makefile",
                "source_line": lineno,
                "source_excerpt": line.strip(),
                "retrieval_query": description.strip(),
            }
        )
    return facts


def _fenced_code_blocks(markdown: str) -> list[tuple[str, str]]:
    blocks: list[tuple[str, str]] = []
    pattern = re.compile(r"```([A-Za-z0-9_-]*)\n(.*?)```", re.DOTALL)
    for match in pattern.finditer(markdown):
        blocks.append((match.group(1).strip().lower(), match.group(2)))
    return blocks


def parse_readme_commands(repo_root: Path) -> list[dict[str, Any]]:
    path = repo_root / "README.md"
    if not path.exists():
        return []
    text = path.read_text(encoding="utf-8", errors="ignore")
    facts: list[dict[str, Any]] = []
    for language, block in _fenced_code_blocks(text):
        if language not in {"bash", "sh", ""}:
            continue
        lines = block.splitlines()
        idx = 0
        while idx < len(lines):
            line = lines[idx].strip()
            if not line.startswith("#"):
                idx += 1
                continue
            comment_lines = [line[1:].strip()]
            idx += 1
            while idx < len(lines) and lines[idx].strip().startswith("#"):
                comment_lines.append(lines[idx].strip()[1:].strip())
                idx += 1
            while idx < len(lines) and not lines[idx].strip():
                idx += 1
            if idx >= len(lines):
                break
            command = lines[idx].strip()
            if not command or command.startswith("#"):
                continue
            description = " ".join(part for part in comment_lines if part)
            facts.append(
                {
                    "fact_type": "readme_command",
                    "question_type": "readme_command",
                    "answer_kind": "command",
                    "description": description,
                    "expected_answer": command,
                    "acceptable_answers": [command],
                    "source_file": "README.md",
                    "source_line": None,
                    "source_excerpt": f"# {description}\n{command}",
                    "retrieval_query": description,
                }
            )
            idx += 1
    return facts


def build_question(fact: dict[str, Any]) -> str:
    if fact["question_type"] == "symbol_definition":
        noun = "function" if fact["symbol_kind"] == "def" else "class"
        return (
            f"Which file defines the {noun} `{fact['symbol_name']}`? "
            "Reply with only the repo-relative path."
        )
    if fact["question_type"] == "make_target":
        return (
            f"Which `make` target is described as: \"{fact['description']}\"? "
            "Reply with only the target name."
        )
    if fact["question_type"] == "readme_command":
        return (
            f"What command is shown for: \"{fact['description']}\"? "
            "Reply with only the command line."
        )
    if fact["question_type"] == "file_contains":
        return (
            f"Which file contains the project fact: \"{fact['description']}\"? "
            "Reply with only the repo-relative path."
        )
    raise ValueError(f"Unsupported question type {fact['question_type']}")


def generate_questions(
    repo_id: str,
    repo_root: Path,
    trials: int = 25,
    seed: int = 0,
) -> list[dict[str, Any]]:
    rng = random.Random(f"{repo_id}:{seed}")

    symbol_facts = parse_python_symbols(repo_id, repo_root)
    make_facts = parse_make_targets(repo_root)
    command_facts = parse_readme_commands(repo_root)

    rng.shuffle(symbol_facts)
    rng.shuffle(make_facts)
    rng.shuffle(command_facts)

    desired = [
        ("symbol_definition", 10, symbol_facts),
        ("make_target", 10, make_facts),
        ("readme_command", 5, command_facts),
    ]

    selected: list[dict[str, Any]] = []
    used_keys: set[tuple[str, str]] = set()
    remaining: list[dict[str, Any]] = []

    for question_type, count, pool in desired:
        taken = 0
        for fact in pool:
            dedupe_key = (question_type, fact["expected_answer"])
            if dedupe_key in used_keys:
                continue
            selected.append(fact)
            used_keys.add(dedupe_key)
            taken += 1
            if taken >= count:
                break
        for fact in pool[taken:]:
            remaining.append(fact)

    if len(selected) < trials:
        rng.shuffle(remaining)
        for fact in remaining:
            dedupe_key = (fact["question_type"], fact["expected_answer"])
            if dedupe_key in used_keys:
                continue
            selected.append(fact)
            used_keys.add(dedupe_key)
            if len(selected) >= trials:
                break

    if len(selected) < trials:
        raise RuntimeError(
            f"Only generated {len(selected)} questions for {repo_id}; need {trials}."
        )

    rows: list[dict[str, Any]] = []
    for index, fact in enumerate(selected[:trials], start=1):
        row = dict(fact)
        row["repo_id"] = repo_id
        row["repo_root"] = repo_root.as_posix()
        row["question_id"] = f"{repo_id}-{index:03d}"
        row["question"] = build_question(fact)
        rows.append(row)
    return rows


def tokenize(text: str) -> list[str]:
    return [
        token
        for token in re.findall(r"[A-Za-z0-9_.-]+", text.lower())
        if token not in STOPWORDS and len(token) >= 3
    ]


def normalize_prediction(text: str) -> str:
    text = re.sub(r"<think>.*?</think>", "", text, flags=re.DOTALL).strip()
    text = re.sub(r"^```[A-Za-z0-9_-]*\n", "", text)
    text = re.sub(r"\n```$", "", text)
    text = text.strip().strip("`").strip()
    if "\n" in text:
        text = text.splitlines()[0].strip()
    return text


def normalize_for_match(text: str, answer_kind: str) -> str:
    text = normalize_prediction(text)
    if answer_kind == "command":
        return " ".join(text.split())
    return text


def is_correct(prediction: str, acceptable_answers: list[str], answer_kind: str) -> bool:
    normalized = normalize_for_match(prediction, answer_kind)
    if not normalized:
        return False
    acceptable = [normalize_for_match(value, answer_kind) for value in acceptable_answers]
    if normalized in acceptable:
        return True
    return any(value and value in normalized for value in acceptable)


def run_rg(repo_root: Path, pattern: str, glob: str | None = None, max_count: int = 8) -> list[str]:
    command = ["rg", "-n", "-S", "--hidden", "--max-count", str(max_count)]
    if glob:
        command.extend(["-g", glob])
    command.extend([pattern, repo_root.as_posix()])
    try:
        result = subprocess.run(
            command,
            check=False,
            capture_output=True,
            text=True,
            encoding="utf-8",
        )
    except FileNotFoundError:
        return []
    if result.returncode not in {0, 1}:
        return []
    return [line.strip() for line in result.stdout.splitlines() if line.strip()]


def makefile_context(repo_root: Path, query: str) -> list[str]:
    candidates = parse_make_targets(repo_root)
    query_tokens = set(tokenize(query))
    ranked: list[tuple[int, dict[str, Any]]] = []
    for candidate in candidates:
        description_tokens = set(tokenize(candidate["description"]))
        ranked.append((len(query_tokens & description_tokens), candidate))
    ranked.sort(key=lambda item: (-item[0], item[1]["target"]))
    return [
        f"{item['target']}: {item['description']}"
        for score, item in ranked[:10]
        if score > 0 or len(ranked) <= 10
    ]


def readme_command_context(repo_root: Path, query: str) -> list[str]:
    candidates = parse_readme_commands(repo_root)
    query_tokens = set(tokenize(query))
    ranked: list[tuple[int, dict[str, Any]]] = []
    for candidate in candidates:
        desc_tokens = set(tokenize(candidate["description"]))
        ranked.append((len(query_tokens & desc_tokens), candidate))
    ranked.sort(key=lambda item: (-item[0], item[1]["expected_answer"]))
    return [
        f"# {item['description']}\n{item['expected_answer']}"
        for score, item in ranked[:8]
        if score > 0 or len(ranked) <= 8
    ]


def symbol_context(repo_root: Path, symbol_name: str) -> list[str]:
    exact = run_rg(repo_root, rf"^(def|class)\s+{re.escape(symbol_name)}\b", glob="*.py")
    if exact:
        return exact[:6]
    return run_rg(repo_root, symbol_name, glob="*.py")[:6]


def build_context(question: dict[str, Any], repo_root: Path) -> list[str]:
    question_type = question["question_type"]
    if question_type == "symbol_definition":
        return symbol_context(repo_root, question["retrieval_query"])
    if question_type == "make_target":
        return makefile_context(repo_root, question["retrieval_query"])
    if question_type == "readme_command":
        return readme_command_context(repo_root, question["retrieval_query"])
    if question_type == "file_contains":
        return run_rg(
            repo_root,
            question["retrieval_query"],
            glob=question.get("source_glob"),
        )
    return []


def direct_answer(question: dict[str, Any], repo_root: Path) -> tuple[str, list[str]]:
    context = build_context(question, repo_root)
    if not context:
        return "", context

    if question["question_type"] == "symbol_definition":
        for line in context:
            match = re.match(rf"{re.escape(repo_root.as_posix())}/(.+?):\d+:", line)
            if match:
                return match.group(1), context
        if ":" in context[0]:
            return context[0].split(":", 1)[0], context

    if question["question_type"] == "make_target":
        best = context[0]
        if ":" in best:
            return best.split(":", 1)[0].strip(), context

    if question["question_type"] == "readme_command":
        best = context[0]
        lines = [line for line in best.splitlines() if line and not line.lstrip().startswith("#")]
        if lines:
            return lines[0].strip(), context

    if question["question_type"] == "file_contains":
        for line in context:
            match = re.match(rf"{re.escape(repo_root.as_posix())}/(.+?):\d+:", line)
            if match:
                return match.group(1), context
        if ":" in context[0]:
            return context[0].split(":", 1)[0], context

    return "", context


def ollama_chat_completion(
    model: str,
    system_prompt: str,
    user_prompt: str,
    api_base_url: str = "http://localhost:11434/v1/chat/completions",
    temperature: float = 0.0,
    timeout_seconds: float = 60.0,
) -> str:
    payload = {
        "model": model,
        "messages": [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": user_prompt},
        ],
        "temperature": temperature,
        "stream": False,
    }
    body = json.dumps(payload).encode("utf-8")
    request = urllib.request.Request(
        api_base_url,
        data=body,
        headers={
            "Content-Type": "application/json",
            "Authorization": "Bearer local-ollama",
        },
        method="POST",
    )
    with urllib.request.urlopen(request, timeout=timeout_seconds) as response:
        raw = response.read().decode("utf-8")
    parsed = json.loads(raw)
    return parsed["choices"][0]["message"]["content"]


def answer_question(
    question: dict[str, Any],
    repo_root: Path,
    model: str,
    api_base_url: str,
) -> tuple[str, list[str], float]:
    context = build_context(question, repo_root)
    system_prompt = (
        "You answer repository benchmark questions using only the provided snippets. "
        "Return exactly one answer, with no explanation, no markdown fence, and no extra words."
    )
    kind_hint = {
        "path": "Return only the repo-relative path.",
        "target": "Return only the make target name.",
        "command": "Return only the command line.",
    }[question["answer_kind"]]
    context_block = "\n".join(f"- {line}" for line in context) if context else "- <no matches>"
    user_prompt = (
        f"Repository root: {repo_root.as_posix()}\n"
        f"Question: {question['question']}\n"
        f"{kind_hint}\n"
        "Relevant snippets:\n"
        f"{context_block}\n"
    )
    started = time.perf_counter()
    prediction = ollama_chat_completion(
        model=model,
        system_prompt=system_prompt,
        user_prompt=user_prompt,
        api_base_url=api_base_url,
    )
    return prediction, context, time.perf_counter() - started
