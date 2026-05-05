# Local KG Stack

`litkg-rs` now carries the reusable local Neo4j and Graphiti runtime pieces that were first established in NBV and generalized for this repo.

For the full external-tool map, backend status, repo paths, and runtime diagrams,
see [tooling-and-backends.md](tooling-and-backends.md).

## Surfaces

- `infra/neo4j/docker-compose.yml`: local Neo4j + APOC service
- `scripts/kg/up.sh`: start Neo4j and create local data directories
- `scripts/kg/down.sh`: stop Neo4j without deleting persisted data
- `scripts/kg/index_code.sh`: check/bootstrap/index CodeGraphContext for a repo path
- `scripts/kg/start_graphiti.sh`: clone and run the upstream Graphiti MCP server
- `scripts/kg/ingest_docs.sh`: Ollama-backed Graphiti core ingestion for repo docs
- `scripts/kg/enrich_embeddings.py`: local embeddings plus code↔doc `REFERS_TO_CODE` links
- `.env.example`: local defaults for Neo4j, Ollama, Graphiti, and CodeGraphContext
- `.cgcignore`: code-index exclusions for local KG indexing

## Runtime Layout

Runtime-only assets are written under ignored local paths:

- `.data/kg/neo4j/data`
- `.data/kg/neo4j/plugins`
- `.cache/kg/venvs/cgc`
- `.cache/kg/graphiti`

## Basic Workflow

1. Start Neo4j:

```bash
make kg-up
```

If Ollama runs on another machine, expose it to this host and verify the
required models before doc ingestion or embedding enrichment:

```bash
ssh -R 11434:127.0.0.1:11434 <ubuntu-host>
export OLLAMA_BASE_URL=http://127.0.0.1:11434/v1
export EMBEDDING_MODEL=qwen3-embedding:4b
export EMBEDDING_DIM=2560
export GRAPHITI_LLM_MODEL=gemma4:26b
make kg-ollama-check
```

Client repos can store those defaults under `[runtime.ollama]` in their litkg
TOML config and invoke `make kg-ollama-check LITKG_CONFIG=/path/to/litkg.toml`.

2. Index code under the whole repo or one subtree:

```bash
make kg-index-check
make kg-index-bootstrap
make kg-index
make kg-update KG_SRC_DIR=crates/litkg-core
make kg-update KG_CODE_REPO_ROOT=/Users/jd/repos/NBV KG_SRC_DIR=aria_nbv/aria_nbv
```

3. Ingest docs:

```bash
make kg-ingest-docs
./scripts/kg/ingest_docs.sh README.md docs/architecture.md .agents/AGENTS_INTERNAL_DB.md
```

4. Refresh embeddings and `REFERS_TO_CODE` links:

```bash
make kg-enrich
```

5. Stop the stack:

```bash
make kg-down
```

## Notes

- `kg-update` is the main incremental refresh entrypoint for code-path changes.
- `kg-ollama-check` verifies the configured Ollama HTTP endpoint, required chat
  and embedding models, the configured embedding dimension, and a chat
  response. It supports SSH-tunneled model hosts and does not require a local
  `ollama` CLI on this machine.
- `kg-index-check` is the non-mutating readiness gate; it reports missing prerequisites and whether Neo4j is running.
- `kg-index-bootstrap` prepares the local CodeGraphContext runtime once and avoids repeated dependency setup churn on later indexing runs.
- `KG_CODE_REPO_ROOT` lets the toolkit index code in another repo while still using the local Neo4j and embedding runtime. This is the path to CGC-based Python package indexing outside `litkg-rs`.
- `kg-ingest-docs` without explicit paths ingests the repo’s README, AGENTS surface, internal DB, and authored docs/reference markdown.
- When `KG_CODE_REPO_ROOT` points at an external repo, code embeddings still refresh, but doc linking is disabled by default to avoid attaching external code nodes to `litkg-rs` documentation. Re-enable it explicitly with `KG_ENABLE_DOC_LINKS=1` only when the matching external docs have also been ingested into the same Neo4j graph.
