#!/usr/bin/env python3
from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]


def run_check(args: list[str]) -> list[str]:
    result = subprocess.run(args, cwd=REPO_ROOT, text=True, capture_output=True)
    if result.returncode == 0:
        return []
    output = "\n".join(part for part in (result.stdout, result.stderr) if part)
    return [f"{' '.join(args)} failed:\n{output.strip()}"]


def check_agents_md() -> list[str]:
    path = REPO_ROOT / "AGENTS.md"
    if not path.exists():
        return ["AGENTS.md is missing"]
    text = path.read_text()
    required_sections = ["Sources Of Truth", "Repo Map", "Repo-Wide Rules"]
    return [f"AGENTS.md missing section `{section}`" for section in required_sections if section not in text]


def check_nested_agents() -> list[str]:
    errors: list[str] = []
    root_text = (REPO_ROOT / "AGENTS.md").read_text() if (REPO_ROOT / "AGENTS.md").exists() else ""
    for path in sorted(REPO_ROOT.glob("**/AGENTS.md")):
        if path == REPO_ROOT / "AGENTS.md":
            continue
        if ".git" in path.parts or "target" in path.parts:
            continue
        text = path.read_text()
        if "git reset --hard" in text and "git reset --hard" not in root_text:
            errors.append(f"{path}: nested AGENTS.md introduces destructive git guidance not present in root")
    return errors


def check_mcp_profiles() -> list[str]:
    errors: list[str] = []
    config_paths = sorted(
        path
        for pattern in ("**/*mcp*.json", "**/*mcp*.yaml", "**/*mcp*.yml", "**/*mcp*.toml")
        for path in REPO_ROOT.glob(pattern)
        if ".git" not in path.parts and "target" not in path.parts
    )
    server_names: dict[str, Path] = {}
    broad_write_re = re.compile(r"(?i)(gmail|send|delete|write|mutation|commit|push)")
    for path in config_paths:
        text = path.read_text(errors="ignore")
        for name in re.findall(r"(?m)^\s*(?:name|server|id)\s*[:=]\s*['\"]?([A-Za-z0-9_.-]+)", text):
            if name in server_names:
                errors.append(f"duplicate MCP server `{name}` in {server_names[name]} and {path}")
            server_names[name] = path
        if "default" in text.lower() and broad_write_re.search(text):
            errors.append(f"{path}: default MCP profile appears to expose broad write tools")
    return errors


def check_secret_patterns() -> list[str]:
    errors: list[str] = []
    secret_patterns = [
        re.compile(r"sk-[A-Za-z0-9_-]{20,}"),
        re.compile(r"-----BEGIN [A-Z ]*PRIVATE KEY-----"),
        re.compile(r"(?i)(api[_-]?key|token|secret)\s*=\s*['\"][^'\"]{12,}['\"]"),
    ]
    for path in sorted((REPO_ROOT / ".agents").glob("**/*")):
        if not path.is_file() or path.suffix not in {".md", ".toml", ".yaml", ".yml", ".json", ".py"}:
            continue
        text = path.read_text(errors="ignore")
        for pattern in secret_patterns:
            if pattern.search(text):
                errors.append(f"{path}: matched secret pattern `{pattern.pattern}`")
    return errors


def main() -> int:
    errors: list[str] = []
    errors.extend(run_check(["python3", ".agents/scripts/check_backlog.py"]))
    errors.extend(run_check(["python3", ".agents/scripts/check_skills.py"]))
    errors.extend(check_agents_md())
    errors.extend(check_nested_agents())
    errors.extend(check_mcp_profiles())
    errors.extend(check_secret_patterns())
    if errors:
        print("Scaffold validation errors:")
        for error in errors:
            print(f"  - {error}")
        return 1
    print("Scaffold is valid.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
