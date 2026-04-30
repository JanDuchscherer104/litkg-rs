#!/usr/bin/env python3
from __future__ import annotations

import os
import re
import stat
import sys
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
SKILLS_ROOT = REPO_ROOT / ".agents" / "skills"
SECRET_PATTERNS = [
    re.compile(r"sk-[A-Za-z0-9_-]{20,}"),
    re.compile(r"-----BEGIN [A-Z ]*PRIVATE KEY-----"),
    re.compile(r"(?i)(api[_-]?key|token|secret)\s*=\s*['\"][^'\"]{12,}['\"]"),
]
PERSONAL_PATH_PATTERNS = [
    re.compile(r"/home/[A-Za-z0-9._-]+/"),
    re.compile(r"/Users/[A-Za-z0-9._-]+/"),
    re.compile(r"[A-Za-z]:\\\\Users\\\\"),
]
WRONG_REPO_PATTERNS = [
    re.compile(r"PRML VSLAM"),
    re.compile(r"prml_vslam"),
    re.compile(r"JanDuchscherer104/NBV"),
]


def parse_frontmatter(path: Path) -> tuple[dict[str, str], list[str]]:
    text = path.read_text()
    lines = text.splitlines()
    errors: list[str] = []
    if not lines or lines[0].strip() != "---":
        return {}, [f"{path}: missing YAML frontmatter"]
    try:
        end = lines[1:].index("---") + 1
    except ValueError:
        return {}, [f"{path}: unterminated YAML frontmatter"]
    fields: dict[str, str] = {}
    for line in lines[1:end]:
        if not line.strip():
            continue
        if ":" not in line:
            errors.append(f"{path}: invalid frontmatter line `{line}`")
            continue
        key, value = line.split(":", 1)
        fields[key.strip()] = value.strip().strip('"')
    return fields, errors


def check_skill(skill_dir: Path) -> list[str]:
    errors: list[str] = []
    skill_md = skill_dir / "SKILL.md"
    if not skill_md.exists():
        return [f"{skill_dir}: missing SKILL.md"]

    fields, fm_errors = parse_frontmatter(skill_md)
    errors.extend(fm_errors)
    allowed = {"name", "description"}
    extra = sorted(set(fields) - allowed)
    if extra:
        errors.append(f"{skill_md}: unsupported frontmatter fields: {', '.join(extra)}")
    for required in ("name", "description"):
        if not fields.get(required):
            errors.append(f"{skill_md}: missing frontmatter field `{required}`")
    if fields.get("name") and fields["name"] != skill_dir.name:
        errors.append(f"{skill_md}: skill name `{fields['name']}` does not match folder `{skill_dir.name}`")
    description = fields.get("description", "")
    if "when" not in description.lower() or len(description.split()) < 8:
        errors.append(f"{skill_md}: description must include what the skill does and when to use it")

    text = skill_md.read_text()
    for pattern in SECRET_PATTERNS + PERSONAL_PATH_PATTERNS + WRONG_REPO_PATTERNS:
        if pattern.search(text):
            errors.append(f"{skill_md}: matched disallowed pattern `{pattern.pattern}`")

    scripts_dir = skill_dir / "scripts"
    if scripts_dir.exists():
        for script in sorted(path for path in scripts_dir.iterdir() if path.is_file()):
            mode = script.stat().st_mode
            if not (mode & stat.S_IXUSR):
                errors.append(f"{script}: script must be executable")
            first_line = script.read_text(errors="ignore").splitlines()[:1]
            if script.suffix == ".py" and first_line != ["#!/usr/bin/env python3"]:
                errors.append(f"{script}: Python script must start with #!/usr/bin/env python3")
            if script.name not in text:
                errors.append(f"{script}: script must be documented in SKILL.md")
    return errors


def main() -> int:
    if not SKILLS_ROOT.exists():
        print(f"Missing skills root: {SKILLS_ROOT}")
        return 1
    errors: list[str] = []
    for skill_dir in sorted(path for path in SKILLS_ROOT.iterdir() if path.is_dir()):
        errors.extend(check_skill(skill_dir))
    if errors:
        print("Skill validation errors:")
        for error in errors:
            print(f"  - {error}")
        return 1
    print("Skills are valid.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
