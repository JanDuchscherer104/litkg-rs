#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

load_env() {
  local env_file
  for env_file in "${REPO_ROOT}/.env" "${REPO_ROOT}/.env.example"; do
    if [[ -f "${env_file}" ]]; then
      set -a
      # shellcheck disable=SC1090
      source "${env_file}"
      set +a
      return
    fi
  done
}

resolve_repo_path() {
  local raw_path="$1"
  if [[ "${raw_path}" = /* ]]; then
    printf '%s\n' "${raw_path}"
  else
    printf '%s\n' "${REPO_ROOT}/${raw_path#./}"
  fi
}

ensure_graphiti_checkout() {
  local graphiti_repo="$1"
  mkdir -p "$(dirname "${graphiti_repo}")"
  if [[ ! -d "${graphiti_repo}/.git" ]]; then
    git clone --depth 1 https://github.com/getzep/graphiti.git "${graphiti_repo}"
  fi
}

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "Missing required command: $1" >&2
    exit 1
  fi
}

pick_gemma_model() {
  local installed_model
  if [[ -n "${GRAPHITI_LLM_MODEL:-}" ]] && ollama list | awk 'NR > 1 {print $1}' | grep -qx "${GRAPHITI_LLM_MODEL}"; then
    printf '%s\n' "${GRAPHITI_LLM_MODEL}"
    return
  fi

  installed_model="$(ollama list | awk 'NR > 1 && $1 ~ /^gemma/ {print $1; exit}')"
  if [[ -n "${installed_model}" ]]; then
    printf '%s\n' "${installed_model}"
    return
  fi

  ollama pull gemma3:27b
  printf '%s\n' "gemma3:27b"
}

ensure_embedding_model() {
  local model_name="$1"
  if ! ollama list | awk 'NR > 1 {print $1}' | grep -qx "${model_name}"; then
    ollama pull "${model_name}"
  fi
}

load_env

require_cmd git
require_cmd ollama
require_cmd uv

GRAPHITI_REPO_ABS="$(resolve_repo_path "${GRAPHITI_REPO_DIR:-.cache/kg/graphiti}")"
ensure_graphiti_checkout "${GRAPHITI_REPO_ABS}"
cd "${GRAPHITI_REPO_ABS}"
uv sync

export GRAPHITI_LLM_MODEL="$(pick_gemma_model)"
export EMBEDDING_MODEL="${EMBEDDING_MODEL:-qwen3-embedding:4b}"
ensure_embedding_model "${EMBEDDING_MODEL}"

cd "${REPO_ROOT}"
uv run --directory "${GRAPHITI_REPO_ABS}" --project . python - "${REPO_ROOT}" "$@" <<'PY'
import asyncio
import json
import os
import re
import sys
from datetime import datetime, timezone
from pathlib import Path

repo_root = Path(sys.argv[1]).resolve()
cli_paths = [Path(arg) for arg in sys.argv[2:]]

os.environ.setdefault("EMBEDDING_DIM", os.environ.get("EMBEDDING_DIM", "1024"))

from graphiti_core import Graphiti
from graphiti_core.cross_encoder.openai_reranker_client import OpenAIRerankerClient
from graphiti_core.embedder.openai import OpenAIEmbedder, OpenAIEmbedderConfig
from graphiti_core.llm_client.config import LLMConfig
from graphiti_core.llm_client.openai_generic_client import OpenAIGenericClient
from graphiti_core.nodes import EpisodeType
from openai.types.chat import ChatCompletionMessageParam
from pydantic import BaseModel


class SanitizingOpenAIGenericClient(OpenAIGenericClient):
    @staticmethod
    def _extract_json_payload(text: str) -> dict[str, object]:
        cleaned = text.strip()
        cleaned = re.sub(r"<\|channel\|>thought\s*.*?<\|channel\|>", "", cleaned, flags=re.DOTALL)
        cleaned = re.sub(r"^```(?:json)?\s*", "", cleaned, flags=re.IGNORECASE)
        cleaned = re.sub(r"\s*```$", "", cleaned)
        decoder = json.JSONDecoder()
        for index, character in enumerate(cleaned):
            if character not in "{[":
                continue
            try:
                parsed, _ = decoder.raw_decode(cleaned[index:])
            except json.JSONDecodeError:
                continue
            if isinstance(parsed, dict):
                return parsed
            if isinstance(parsed, list) and parsed and isinstance(parsed[0], dict):
                return parsed[0]
        return json.loads(cleaned)

    async def _generate_response(
        self,
        messages: list,
        response_model: type[BaseModel] | None = None,
        max_tokens: int = 16384,
        model_size=None,
    ) -> dict[str, object]:
        openai_messages: list[ChatCompletionMessageParam] = []
        for message in messages:
            message.content = self._clean_input(message.content)
            if message.role == "user":
                openai_messages.append({"role": "user", "content": message.content})
            elif message.role == "system":
                openai_messages.append({"role": "system", "content": message.content})

        response_format: dict[str, object] = {"type": "json_object"}
        if response_model is not None:
            response_format = {
                "type": "json_schema",
                "json_schema": {
                    "name": getattr(response_model, "__name__", "structured_response"),
                    "schema": response_model.model_json_schema(),
                },
            }

        response = await self.client.chat.completions.create(
            model=self.model,
            messages=openai_messages,
            temperature=self.temperature,
            max_tokens=self.max_tokens,
            response_format=response_format,  # type: ignore[arg-type]
        )
        result = response.choices[0].message.content or ""
        return self._extract_json_payload(result)


def iter_doc_paths() -> list[Path]:
    if cli_paths:
        resolved = []
        for raw_path in cli_paths:
            path = raw_path if raw_path.is_absolute() else (repo_root / raw_path)
            if not path.exists():
                raise FileNotFoundError(path)
            resolved.append(path.resolve())
        return resolved

    fixed_paths = [
        repo_root / "README.md",
        repo_root / "AGENTS.md",
        repo_root / ".agents" / "AGENTS_INTERNAL_DB.md",
    ]

    excluded_prefixes = [
        repo_root / ".agents" / "scripts",
        repo_root / ".agents" / "work",
    ]

    discovered: list[Path] = []
    for root_dir, suffixes in [
        (repo_root / "docs", ("*.md",)),
        (repo_root / ".agents" / "references", ("*.md",)),
    ]:
        if not root_dir.exists():
            continue
        for suffix in suffixes:
            for path in root_dir.rglob(suffix):
                resolved = path.resolve()
                if any(resolved.is_relative_to(prefix) for prefix in excluded_prefixes):
                    continue
                discovered.append(resolved)

    all_paths = fixed_paths + sorted(discovered)
    unique_paths: list[Path] = []
    seen: set[Path] = set()
    for path in all_paths:
        resolved = path.resolve()
        if resolved in seen or not resolved.exists():
            continue
        seen.add(resolved)
        unique_paths.append(resolved)
    return unique_paths


def extract_title(frontmatter: str, body: str, path: Path) -> str:
    title_match = re.search(r"(?im)^title:\s*[\"']?(.*?)[\"']?\s*$", frontmatter)
    if title_match and title_match.group(1).strip():
        return title_match.group(1).strip()
    heading_match = re.search(r"(?m)^#\s+(.+?)\s*$", body)
    if heading_match:
        return heading_match.group(1).strip()
    return path.stem.replace("_", " ")


def strip_frontmatter(text: str) -> tuple[str, str]:
    if not text.startswith("---\n"):
        return "", text
    parts = text.split("\n---\n", 1)
    if len(parts) != 2:
        return "", text
    return parts[0][4:], parts[1]


def strip_quarto_chunks(text: str) -> str:
    lines = text.splitlines()
    kept: list[str] = []
    in_chunk = False
    closing_fence = ""
    for line in lines:
        stripped = line.lstrip()
        if not in_chunk and re.match(r"^(```|~~~)\{", stripped):
            in_chunk = True
            closing_fence = stripped[:3]
            continue
        if in_chunk and stripped.startswith(closing_fence):
            in_chunk = False
            closing_fence = ""
            continue
        if not in_chunk:
            kept.append(line)
    return "\n".join(kept).strip()


def build_episode_body(path: Path) -> tuple[str, datetime]:
    raw_text = path.read_text(encoding="utf-8")
    frontmatter, without_frontmatter = strip_frontmatter(raw_text)
    cleaned = strip_quarto_chunks(without_frontmatter)
    char_limit = int(os.environ.get("GRAPHITI_DOC_CHAR_LIMIT", "6000"))
    if len(cleaned) > char_limit:
        cleaned = cleaned[:char_limit].rsplit("\n", 1)[0].rstrip() + "\n\n[Truncated for local Graphiti ingestion]"
    title = extract_title(frontmatter, cleaned, path)
    rel_path = path.relative_to(repo_root).as_posix()
    kind = path.suffix.lstrip(".") or "markdown"
    body = (
        f"Source Path: {rel_path}\n"
        f"Document Title: {title}\n"
        f"Document Kind: {kind}\n\n"
        f"{cleaned}\n"
    ).strip()
    reference_time = datetime.fromtimestamp(path.stat().st_mtime, tz=timezone.utc)
    return body, reference_time


async def main() -> None:
    doc_paths = iter_doc_paths()
    if not doc_paths:
        print("No document paths selected.")
        return

    llm_config = LLMConfig(
        api_key="ollama",
        model=os.environ["GRAPHITI_LLM_MODEL"],
        small_model=os.environ["GRAPHITI_LLM_MODEL"],
        base_url=os.environ.get("OLLAMA_BASE_URL", "http://localhost:11434/v1"),
        temperature=0.0,
    )
    llm_client = SanitizingOpenAIGenericClient(config=llm_config)
    embedder = OpenAIEmbedder(
        config=OpenAIEmbedderConfig(
            api_key="ollama",
            embedding_model=os.environ.get("EMBEDDING_MODEL", "qwen3-embedding:4b"),
            embedding_dim=int(os.environ.get("EMBEDDING_DIM", "1024")),
            base_url=os.environ.get("OLLAMA_BASE_URL", "http://localhost:11434/v1"),
        )
    )
    graphiti = Graphiti(
        os.environ.get("NEO4J_URI", "bolt://localhost:7687"),
        os.environ.get("NEO4J_USER", os.environ.get("NEO4J_USERNAME", "neo4j")),
        os.environ["NEO4J_PASSWORD"],
        llm_client=llm_client,
        embedder=embedder,
        cross_encoder=OpenAIRerankerClient(client=llm_client.client, config=llm_config),
        max_coroutines=int(os.environ.get("SEMAPHORE_LIMIT", "2")),
    )

    try:
        await graphiti.build_indices_and_constraints()
        group_id = os.environ.get("GRAPHITI_GROUP_ID", "litkg-docs")
        for path in doc_paths:
            episode_body, reference_time = build_episode_body(path)
            rel_path = path.relative_to(repo_root).as_posix()
            source_description = f"litkg-rs repo {path.suffix.lstrip('.') or 'markdown'} documentation"
            await graphiti.add_episode(
                name=rel_path,
                episode_body=episode_body,
                source=EpisodeType.text,
                source_description=source_description,
                reference_time=reference_time,
                group_id=group_id,
            )
            print(f"Ingested {rel_path}")
    finally:
        await graphiti.close()


asyncio.run(main())
PY
