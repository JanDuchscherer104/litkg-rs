#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
COMPOSE_FILE="${REPO_ROOT}/infra/neo4j/docker-compose.yml"
DOCKER_COMPOSE_CMD=()

info() {
  echo "[kg-index] $*"
}

die() {
  echo "[kg-index] $*" >&2
  exit 1
}

usage() {
  cat <<EOF
Usage: ./scripts/kg/index_code.sh [--check] [--bootstrap] [--skip-neo4j-check] [target]

Options:
  --check              Validate indexing prerequisites and exit with a readiness report.
  --bootstrap          Prepare/reuse the local CodeGraphContext runtime and exit.
  --skip-neo4j-check   Skip Neo4j runtime checks and post-index count query.

Environment:
  KG_CODE_REPO_ROOT       Code repository root (default: .)
  CGC_VENV_DIR            Virtualenv path (default: .cache/kg/venvs/cgc)
  CGC_PIP_SPEC            Package spec for CodeGraphContext (default: codegraphcontext)
  KG_CGC_AUTO_SETUP       Create/install missing CGC runtime when indexing (default: 1)
  CGC_FORCE_REINSTALL     Reinstall CGC package on this run (default: 0)
  KG_INDEX_CHECK_ONLY     Equivalent to --check when set to 1
  KG_INDEX_BOOTSTRAP_ONLY Equivalent to --bootstrap when set to 1
EOF
}

load_env() {
  local env_file raw_line line key value
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

resolve_against_root() {
  local root_path="$1"
  local raw_path="$2"
  if [[ "${raw_path}" = /* ]]; then
    printf '%s\n' "${raw_path}"
  else
    printf '%s\n' "${root_path}/${raw_path#./}"
  fi
}

repo_relative_path() {
  local root_path="$1"
  local raw_path="$2"
  local abs_path
  abs_path="$(resolve_against_root "${root_path}" "${raw_path}")"
  if [[ "${abs_path}" == "${root_path}" ]]; then
    printf '.\n'
    return
  fi
  case "${abs_path}" in
    "${root_path}"/*)
      printf '%s\n' "${abs_path#${root_path}/}"
      ;;
    *)
      die "Target path must live under the code repo root: ${raw_path}"
      ;;
  esac
}

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    die "Missing required command: $1"
  fi
}

has_cmd() {
  command -v "$1" >/dev/null 2>&1
}

setup_docker_compose_cmd() {
  if docker compose version >/dev/null 2>&1; then
    DOCKER_COMPOSE_CMD=(docker compose)
    return
  fi
  if command -v docker-compose >/dev/null 2>&1; then
    DOCKER_COMPOSE_CMD=(docker-compose)
    return
  fi
  die "Missing Docker Compose. Install Docker Desktop (or docker-compose)."
}

neo4j_running() {
  local services
  services="$("${DOCKER_COMPOSE_CMD[@]}" -f "${COMPOSE_FILE}" ps --services --status running 2>/dev/null || true)"
  grep -Fxq "neo4j" <<< "${services}"
}

ensure_cgc_runtime() {
  local python_bin cgc_bin pip_spec auto_setup force_reinstall
  python_bin="${CGC_VENV_DIR_ABS}/bin/python"
  cgc_bin="${CGC_VENV_DIR_ABS}/bin/cgc"
  pip_spec="${CGC_PIP_SPEC:-codegraphcontext}"
  auto_setup="${KG_CGC_AUTO_SETUP:-1}"
  force_reinstall="${CGC_FORCE_REINSTALL:-0}"

  if [[ ! -x "${python_bin}" ]] || ! "${python_bin}" -m pip --version >/dev/null 2>&1; then
    if [[ "${auto_setup}" != "1" ]]; then
      die "CGC venv missing or lacks pip at ${CGC_VENV_DIR_ABS} and KG_CGC_AUTO_SETUP=0."
    fi
    if has_cmd uv; then
      info "Creating CGC virtualenv at ${CGC_VENV_DIR_ABS} with uv"
      uv venv --seed --clear "${CGC_VENV_DIR_ABS}"
    else
      info "Creating CGC virtualenv at ${CGC_VENV_DIR_ABS}"
      python3 -m venv "${CGC_VENV_DIR_ABS}"
    fi
    force_reinstall=1
  fi

  if [[ ! -x "${cgc_bin}" && "${auto_setup}" != "1" ]]; then
    die "CodeGraphContext missing in ${CGC_VENV_DIR_ABS} and KG_CGC_AUTO_SETUP=0."
  fi

  if [[ "${force_reinstall}" == "1" || ! -x "${cgc_bin}" ]]; then
    info "Installing CodeGraphContext (${pip_spec}) in ${CGC_VENV_DIR_ABS}"
    "${python_bin}" -m pip install --disable-pip-version-check "${pip_spec}"
  else
    info "Using existing CodeGraphContext install in ${CGC_VENV_DIR_ABS}"
  fi
}

readiness_check() {
  local status=0
  info "Readiness report"
  info "Code repo root: ${CODE_REPO_ROOT_ABS}"
  info "Index target: ${INDEX_TARGET_REL}"
  info "CGC venv: ${CGC_VENV_DIR_ABS}"

  if has_cmd python3; then
    info "[ok] python3 is available"
  else
    info "[missing] python3 is required"
    status=1
  fi

  if has_cmd docker; then
    info "[ok] docker is available"
  else
    info "[missing] docker is required"
    status=1
  fi

  if docker compose version >/dev/null 2>&1 || has_cmd docker-compose; then
    info "[ok] docker compose is available"
  else
    info "[missing] docker compose is required"
    status=1
  fi

  if [[ -x "${CGC_VENV_DIR_ABS}/bin/python" ]]; then
    info "[ok] CGC virtualenv exists"
  else
    info "[missing] CGC virtualenv is missing"
    info "          run: ./scripts/kg/index_code.sh --bootstrap"
    status=1
  fi

  if [[ -x "${CGC_VENV_DIR_ABS}/bin/cgc" ]]; then
    info "[ok] codegraphcontext CLI is installed"
  else
    info "[missing] codegraphcontext CLI is missing"
    info "          run: ./scripts/kg/index_code.sh --bootstrap"
    status=1
  fi

  if [[ "${KG_SKIP_NEO4J_CHECK}" == "1" ]]; then
    info "[skip] Neo4j runtime check disabled (KG_SKIP_NEO4J_CHECK=1)"
  elif has_cmd docker && (docker compose version >/dev/null 2>&1 || has_cmd docker-compose); then
    setup_docker_compose_cmd
    if neo4j_running; then
      info "[ok] Neo4j service is running"
    else
      info "[missing] Neo4j service is not running"
      info "          run: make kg-up"
      status=1
    fi
  fi

  return "${status}"
}

load_env

INDEX_TARGET_INPUT="."
KG_INDEX_CHECK_ONLY="${KG_INDEX_CHECK_ONLY:-0}"
KG_INDEX_BOOTSTRAP_ONLY="${KG_INDEX_BOOTSTRAP_ONLY:-0}"
KG_SKIP_NEO4J_CHECK="${KG_SKIP_NEO4J_CHECK:-0}"

for arg in "$@"; do
  case "${arg}" in
    --check|--readiness)
      KG_INDEX_CHECK_ONLY=1
      ;;
    --bootstrap)
      KG_INDEX_BOOTSTRAP_ONLY=1
      ;;
    --skip-neo4j-check)
      KG_SKIP_NEO4J_CHECK=1
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    -*)
      die "Unknown option: ${arg}"
      ;;
    *)
      if [[ "${INDEX_TARGET_INPUT}" != "." ]]; then
        die "Only one target path is supported."
      fi
      INDEX_TARGET_INPUT="${arg}"
      ;;
  esac
done

CODE_REPO_ROOT_INPUT="${KG_CODE_REPO_ROOT:-.}"
CODE_REPO_ROOT_ABS="$(resolve_repo_path "${CODE_REPO_ROOT_INPUT}")"
if [[ ! -d "${CODE_REPO_ROOT_ABS}" ]]; then
  die "Code repo root does not exist or is not a directory: ${CODE_REPO_ROOT_INPUT}"
fi

INDEX_TARGET_ABS="$(resolve_against_root "${CODE_REPO_ROOT_ABS}" "${INDEX_TARGET_INPUT}")"
if [[ ! -e "${INDEX_TARGET_ABS}" ]]; then
  die "Index target does not exist: ${INDEX_TARGET_INPUT}"
fi
INDEX_TARGET_REL="$(repo_relative_path "${CODE_REPO_ROOT_ABS}" "${INDEX_TARGET_ABS}")"

CGC_VENV_DIR_ABS="$(resolve_repo_path "${CGC_VENV_DIR:-.cache/kg/venvs/cgc}")"
mkdir -p "$(dirname "${CGC_VENV_DIR_ABS}")"

if [[ "${KG_INDEX_CHECK_ONLY}" == "1" ]]; then
  if readiness_check; then
    info "Readiness check passed."
    exit 0
  fi
  die "Readiness check failed."
fi

require_cmd python3
require_cmd docker
setup_docker_compose_cmd

if [[ "${KG_INDEX_BOOTSTRAP_ONLY}" == "1" ]]; then
  ensure_cgc_runtime
  info "Bootstrap complete."
  exit 0
fi

if [[ "${KG_SKIP_NEO4J_CHECK}" != "1" ]] && ! neo4j_running; then
  die "Neo4j service is not running. Start it with make kg-up or set KG_SKIP_NEO4J_CHECK=1."
fi

ensure_cgc_runtime

info "Code repo root: ${CODE_REPO_ROOT_ABS}"
info "Index target: ${INDEX_TARGET_REL}"
info "CGC venv: ${CGC_VENV_DIR_ABS}"

export DEFAULT_DATABASE=neo4j
export NEO4J_URI="${NEO4J_URI:-bolt://localhost:7687}"
export NEO4J_USERNAME="${NEO4J_USERNAME:-neo4j}"
export NEO4J_USER="${NEO4J_USER:-${NEO4J_USERNAME}}"
export NEO4J_PASSWORD="${NEO4J_PASSWORD:-litkglocal}"
export NEO4J_DATABASE="${NEO4J_DATABASE:-neo4j}"

cd "${CODE_REPO_ROOT_ABS}"
info "Indexing code graph for: ${INDEX_TARGET_REL}"
"${CGC_VENV_DIR_ABS}/bin/cgc" index "${INDEX_TARGET_REL}"
"${CGC_VENV_DIR_ABS}/bin/cgc" list

if [[ "${KG_SKIP_NEO4J_CHECK}" == "1" ]]; then
  info "Skipping Neo4j count query because KG_SKIP_NEO4J_CHECK=1."
  exit 0
fi

"${DOCKER_COMPOSE_CMD[@]}" -f "${COMPOSE_FILE}" exec -T neo4j \
  cypher-shell -u "${NEO4J_USERNAME}" -p "${NEO4J_PASSWORD}" \
  "RETURN
     count { (:Repository) } AS repositories,
     count { (:File) } AS files,
     count { (:Function) } AS functions,
     count { (:Class) } AS classes,
     count { (:Module) } AS modules;"
