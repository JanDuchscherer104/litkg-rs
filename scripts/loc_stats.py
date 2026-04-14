#!/usr/bin/env python3
"""Compute simple LOC statistics for litkg-rs sources."""

from __future__ import annotations

import argparse
from dataclasses import dataclass
from pathlib import Path

COMMENT_PREFIXES = {
    ".py": ("#",),
    ".sh": ("#",),
    ".toml": ("#",),
    ".yml": ("#",),
    ".yaml": ("#",),
    ".rs": ("//",),
}

DEFAULT_ROOTS = [
    "crates",
    "scripts",
    "docs",
    "examples",
    ".agents",
    "README.md",
    "AGENTS.md",
    "CODEOWNER.md",
    "Cargo.toml",
    "Makefile",
]

DEFAULT_EXTENSIONS = [".rs", ".py", ".sh", ".md", ".toml", ".yml", ".yaml", ".json", ".jsonl"]


@dataclass(frozen=True)
class MarkerEntry:
    kind: str
    path: Path
    line_number: int
    text: str


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--roots",
        nargs="*",
        default=DEFAULT_ROOTS,
        help="Directories or files to scan.",
    )
    parser.add_argument(
        "--extensions",
        nargs="*",
        default=DEFAULT_EXTENSIONS,
        help="File extensions to include (example: .rs .py).",
    )
    parser.add_argument(
        "--todo",
        action="store_true",
        help="Print TODO markers after the summary.",
    )
    parser.add_argument(
        "--fixme",
        action="store_true",
        help="Print FIXME markers after the summary.",
    )
    return parser.parse_args()


def iter_files(roots: list[str], extensions: set[str]) -> list[Path]:
    files: list[Path] = []
    for raw_root in roots:
        root = Path(raw_root)
        if not root.exists():
            continue
        if root.is_file():
            if root.suffix in extensions or root.name == "Makefile":
                files.append(root)
            continue
        for path in sorted(root.rglob("*")):
            if not path.is_file():
                continue
            if any(part in {".git", "target", ".cache", ".data", "__pycache__"} for part in path.parts):
                continue
            if path.suffix in extensions:
                files.append(path)
    # stable de-duplication
    seen: set[Path] = set()
    unique_files: list[Path] = []
    for path in files:
        if path in seen:
            continue
        seen.add(path)
        unique_files.append(path)
    return unique_files


def extract_markers(path: Path, lines: list[str]) -> list[MarkerEntry]:
    markers: list[MarkerEntry] = []
    for line_number, line in enumerate(lines, start=1):
        upper = line.upper()
        for kind in ("TODO", "FIXME"):
            if kind not in upper:
                continue
            markers.append(
                MarkerEntry(
                    kind=kind.lower(),
                    path=path,
                    line_number=line_number,
                    text=line.strip() or "-",
                )
            )
    return markers


def count_stats(files: list[Path]) -> tuple[dict[str, int], list[MarkerEntry]]:
    stats = {
        "files": 0,
        "total": 0,
        "non_empty": 0,
        "comments": 0,
        "todo": 0,
        "fixme": 0,
        "code": 0,
    }
    markers: list[MarkerEntry] = []
    for path in files:
        lines = path.read_text(encoding="utf-8", errors="replace").splitlines()
        stats["files"] += 1
        stats["total"] += len(lines)
        stats["non_empty"] += sum(1 for line in lines if line.strip())
        prefixes = COMMENT_PREFIXES.get(path.suffix, ())
        stats["comments"] += sum(
            1 for line in lines if line.strip() and any(line.lstrip().startswith(prefix) for prefix in prefixes)
        )
        file_markers = extract_markers(path, lines)
        markers.extend(file_markers)
    stats["todo"] = sum(1 for marker in markers if marker.kind == "todo")
    stats["fixme"] = sum(1 for marker in markers if marker.kind == "fixme")
    stats["code"] = max(0, stats["non_empty"] - stats["comments"])
    return stats, markers


def print_summary(label: str, stats: dict[str, int]) -> None:
    print(f"{label}:")
    print(f"  files      {stats['files']}")
    print(f"  total      {stats['total']}")
    print(f"  non-empty  {stats['non_empty']}")
    print(f"  comments   {stats['comments']}")
    print(f"  todo       {stats['todo']}")
    print(f"  fixme      {stats['fixme']}")
    print(f"  code       {stats['code']}")


def print_markers(title: str, markers: list[MarkerEntry]) -> None:
    print(f"\n{title}:")
    if not markers:
        print("  none")
        return
    for marker in markers:
        print(f"  {marker.path}:{marker.line_number}  {marker.text}")


def main() -> None:
    args = parse_args()
    files = iter_files(args.roots, set(args.extensions))
    stats, markers = count_stats(files)
    print_summary("LOC Summary", stats)
    if args.todo:
        print_markers("TODO markers", [marker for marker in markers if marker.kind == "todo"])
    if args.fixme:
        print_markers("FIXME markers", [marker for marker in markers if marker.kind == "fixme"])


if __name__ == "__main__":
    main()
