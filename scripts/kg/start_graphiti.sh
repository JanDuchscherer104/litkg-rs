#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

load_env() {
  local env_file
  for env_file in "${REPO_ROOT}/.env" "${REPO_ROOT}/.env.example"; do
    if [[ -f "${env_file}" ]]; then
      while IFS= read -r raw_line || [[ -n "${raw_line}" ]]; do
        line="${raw_line#"${raw_line%%[![:space:]]*}"}"
        [[ -z "${line}" || "${line}" == \#* || "${line}" != *=* ]] && continue
        key="${line%%=*}"
        value="${line#*=}"
        value="${value%\"}"
        value="${value#\"}"
        value="${value%\'}"
        value="${value#\'}"
        if [[ -z "${!key+x}" ]]; then
          export "${key}=${value}"
        fi
      done < "${env_file}"
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

load_env

require_cmd git
require_cmd uv

MODE="${1:-http}"
if [[ "${MODE}" != "http" && "${MODE}" != "stdio" ]]; then
  echo "Usage: $0 [http|stdio]" >&2
  exit 1
fi

GRAPHITI_REPO_ABS="$(resolve_repo_path "${GRAPHITI_REPO_DIR:-.cache/kg/graphiti}")"
ensure_graphiti_checkout "${GRAPHITI_REPO_ABS}"

cd "${GRAPHITI_REPO_ABS}/mcp_server"
uv sync --extra providers

export CONFIG_PATH="config/config-docker-neo4j.yaml"
export NEO4J_URI="${NEO4J_URI:-bolt://localhost:7687}"
export NEO4J_USER="${NEO4J_USER:-${NEO4J_USERNAME:-neo4j}}"
export NEO4J_PASSWORD="${NEO4J_PASSWORD:-litkglocal}"
export NEO4J_DATABASE="${NEO4J_DATABASE:-neo4j}"
export GRAPHITI_GROUP_ID="${GRAPHITI_GROUP_ID:-litkg-docs}"
export SEMAPHORE_LIMIT="${SEMAPHORE_LIMIT:-2}"

if [[ -z "${OPENAI_API_KEY:-}" ]]; then
  echo "OPENAI_API_KEY is not set; the upstream Graphiti MCP server will start with limited LLM-backed capabilities." >&2
  echo "Use scripts/kg/ingest_docs.sh for the Ollama-backed ingestion path used by this repo." >&2
fi

exec uv run main.py \
  --config "${CONFIG_PATH}" \
  --database-provider neo4j \
  --group-id "${GRAPHITI_GROUP_ID}" \
  --transport "${MODE}"
