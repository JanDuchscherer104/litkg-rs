# Local KG Stack

`litkg-rs` now carries the reusable local Neo4j and Graphiti runtime pieces that were first established in NBV and generalized for this repo.

## Surfaces

- `infra/neo4j/docker-compose.yml`: local Neo4j + APOC service
- `scripts/kg/up.sh`: start Neo4j and create local data directories
- `scripts/kg/down.sh`: stop Neo4j without deleting persisted data
- `scripts/kg/index_code.sh`: install CodeGraphContext into a repo-local venv and index a repo path
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

2. Index code under the whole repo or one subtree:

```bash
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
- `KG_CODE_REPO_ROOT` lets the toolkit index code in another repo while still using the local Neo4j and embedding runtime. This is the path to CGC-based Python package indexing outside `litkg-rs`.
- `kg-ingest-docs` without explicit paths ingests the repo’s README, AGENTS surface, internal DB, and authored docs/reference markdown.
- When `KG_CODE_REPO_ROOT` points at an external repo, code embeddings still refresh, but doc linking is disabled by default to avoid attaching external code nodes to `litkg-rs` documentation. Re-enable it explicitly with `KG_ENABLE_DOC_LINKS=1` only when the matching external docs have also been ingested into the same Neo4j graph.
