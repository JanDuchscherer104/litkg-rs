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

repo_relative_path() {
  local raw_path="$1"
  local abs_path
  abs_path="$(resolve_repo_path "${raw_path}")"
  if [[ "${abs_path}" == "${REPO_ROOT}" ]]; then
    printf '.\n'
    return
  fi
  case "${abs_path}" in
    "${REPO_ROOT}"/*)
      printf '%s\n' "${abs_path#${REPO_ROOT}/}"
      ;;
    *)
      echo "Target path must live under the repo root: ${raw_path}" >&2
      exit 2
      ;;
  esac
}

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "Missing required command: $1" >&2
    exit 1
  fi
}

load_env

require_cmd python3
require_cmd docker

INDEX_TARGET_INPUT="${1:-.}"
INDEX_TARGET_ABS="$(resolve_repo_path "${INDEX_TARGET_INPUT}")"
if [[ ! -e "${INDEX_TARGET_ABS}" ]]; then
  echo "Index target does not exist: ${INDEX_TARGET_INPUT}" >&2
  exit 2
fi
INDEX_TARGET_REL="$(repo_relative_path "${INDEX_TARGET_ABS}")"

CGC_VENV_DIR_ABS="$(resolve_repo_path "${CGC_VENV_DIR:-.cache/kg/venvs/cgc}")"
mkdir -p "$(dirname "${CGC_VENV_DIR_ABS}")"

if [[ ! -x "${CGC_VENV_DIR_ABS}/bin/python" ]]; then
  python3 -m venv "${CGC_VENV_DIR_ABS}"
fi

"${CGC_VENV_DIR_ABS}/bin/pip" install --upgrade pip
"${CGC_VENV_DIR_ABS}/bin/pip" install codegraphcontext

export DEFAULT_DATABASE=neo4j
export NEO4J_URI="${NEO4J_URI:-bolt://localhost:7687}"
export NEO4J_USERNAME="${NEO4J_USERNAME:-neo4j}"
export NEO4J_USER="${NEO4J_USER:-${NEO4J_USERNAME}}"
export NEO4J_PASSWORD="${NEO4J_PASSWORD:-litkglocal}"
export NEO4J_DATABASE="${NEO4J_DATABASE:-neo4j}"

cd "${REPO_ROOT}"
echo "Indexing code graph for: ${INDEX_TARGET_REL}"
"${CGC_VENV_DIR_ABS}/bin/cgc" index "${INDEX_TARGET_REL}"
"${CGC_VENV_DIR_ABS}/bin/cgc" list

docker compose -f "${REPO_ROOT}/infra/neo4j/docker-compose.yml" exec -T neo4j \
  cypher-shell -u "${NEO4J_USERNAME}" -p "${NEO4J_PASSWORD}" \
  "RETURN
     count { (:Repository) } AS repositories,
     count { (:File) } AS files,
     count { (:Function) } AS functions,
     count { (:Class) } AS classes,
     count { (:Module) } AS modules;"
