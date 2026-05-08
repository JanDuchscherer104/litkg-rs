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

load_env

KG_DATA_ROOT_ABS="$(resolve_repo_path "${KG_DATA_ROOT:-.data/kg}")"

mkdir -p \
  "${KG_DATA_ROOT_ABS}/neo4j/data" \
  "${KG_DATA_ROOT_ABS}/neo4j/plugins"

run_docker_compose() {
  if [[ -n "${DOCKER_CONFIG:-}" ]]; then
    docker compose "$@"
    return
  fi

  local docker_config="${HOME}/.docker/config.json"
  if [[ ! -f "${docker_config}" ]] || ! grep -q '"credsStore"[[:space:]]*:[[:space:]]*"desktop"' "${docker_config}"; then
    docker compose "$@"
    return
  fi

  local tmp_config
  tmp_config="$(mktemp -d)"
  trap "rm -rf '${tmp_config}'" EXIT
  printf '{"auths":{}}\n' > "${tmp_config}/config.json"
  DOCKER_CONFIG="${tmp_config}" docker compose "$@"
}

run_docker_compose -f "${REPO_ROOT}/infra/neo4j/docker-compose.yml" up -d --force-recreate
