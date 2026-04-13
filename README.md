# litkg-rs

`litkg-rs` is a repo-independent Rust toolkit for:

- merging paper manifests and BibTeX into a normalized literature registry
- downloading arXiv source bundles and optional PDFs
- parsing TeX sources into structured paper records
- materializing KG-friendly Markdown corpora
- exporting to multiple graph adapters, including graphify-oriented corpora and optional Neo4j bundles
- validating benchmark catalogs and composing benchmark-driven auto research targets

The first consumer is `prml-vslam`, but this repository is intentionally not tied to any single client repo.

## Human Signal

- [CODEOWNER.md](CODEOWNER.md) stores distilled human-owner requirements and preferences that should persist beyond one chat session.
- [AGENTS.md](AGENTS.md) stores repo policy and operating rules for agents.

## Codex Setup

See [docs/codex-setup.md](docs/codex-setup.md) for repo-scoped skill discovery, optional `agents/openai.yaml` metadata, and the optional Graphiti and Neo4j MCP path. That MCP path remains secondary to the main file-output pipeline.

## Bootstrap Templates

Downstream repos can start from the strict templates under [templates/bootstrap/](templates/bootstrap/) and the usage notes in [docs/bootstrap-templates.md](docs/bootstrap-templates.md).

## Workspace

- `litkg-core`: normalized paper model, registry merge, downloader, parser, and materializer
- `litkg-cli`: CLI binary `litkg`
- `litkg-graphify`: graphify-friendly Markdown corpus adapter
- `litkg-neo4j`: optional Neo4j export bundle adapter
- `litkg-viewer`: native `egui`/`petgraph` graph inspector over the repo-owned Neo4j export bundle

## Platform Bias

- Apple Silicon macOS is a primary local development and execution target.
- Native Rust graph/viewer exploration should prefer Apple Silicon-friendly stacks and avoid unnecessary x86/browser indirection where possible.

## Quick Start

```bash
cargo run -p litkg-cli -- sync-registry --config examples/prml-vslam.toml
cargo run -p litkg-cli -- download --config examples/prml-vslam.toml --download-pdfs
cargo run -p litkg-cli -- parse --config examples/prml-vslam.toml
cargo run -p litkg-cli -- materialize --config examples/prml-vslam.toml
cargo run -p litkg-cli -- rebuild-graph --config examples/prml-vslam.toml
cargo run -p litkg-cli -- inspect-graph --config examples/prml-vslam.toml
cargo run -p litkg-cli -- validate-benchmarks --catalog examples/benchmarks/kg.toml --results examples/benchmarks/sample-results.toml
cargo run -p litkg-cli -- render-autoresearch-target --catalog examples/benchmarks/kg.toml --results examples/benchmarks/sample-results.toml --target-id kg_navigation_improvement
```

If the consumer repo does not have `graphify` installed, `rebuild-graph` degrades cleanly and leaves the generated corpus intact.

`inspect-graph` launches the repo-owned native viewer. If the Neo4j export bundle is missing, the command generates `nodes.jsonl` and `edges.jsonl` from the parsed paper set before opening the viewer.

## Backlog

Use the local `.agents/` backlog:

```bash
make agents-db
```

## Local KG Runtime

The repository now carries the reusable local Neo4j and Graphiti runtime pieces that were first proven in NBV:

- `scripts/kg/up.sh` / `down.sh` for local Neo4j lifecycle
- `scripts/kg/index_code.sh` for CodeGraphContext indexing on a repo path
- `scripts/kg/ingest_docs.sh` for Ollama-backed Graphiti ingestion of repo docs
- `scripts/kg/enrich_embeddings.py` for local embeddings and `REFERS_TO_CODE` link enrichment
- `infra/neo4j/docker-compose.yml` and `.env.example` for local defaults

Useful commands:

```bash
make kg-up
make kg-update KG_SRC_DIR=crates/litkg-core
make kg-ingest-docs
make kg-down
```

## Benchmarks And Auto Research Targets

Benchmark metadata, validation, and autoresearch-target composition now live in this repo:

- benchmark catalog: `examples/benchmarks/kg.toml`
- sample results bundle: `examples/benchmarks/sample-results.toml`
- validation command: `make benchmark-validate`
- autoresearch-target rendering: `make autoresearch-target`

Rendered autoresearch targets now distinguish between:

- validation-only result bundles that should stay as schema/smoke evidence
- recognized execution failure runs that are eligible to shape the next deterministic target
- status values outside the recognized promotion/control set must be classified
  explicitly before benchmark validation passes

The renderer also supports `--format issue` for an issue-ready Markdown body
that carries the same benchmark context, promoted inputs, and component scaffold.
`--format json` now includes promotion counts, sanitized run summary text, and
structured score evidence under `result_summaries`; downstream automation should
consume that normalized view rather than the raw source bundle.

The catalog covers `SWE-Bench Pro`, `SWE-QA-Pro`, `CodeRepoQA`, `StackRepoQA`, `RepoReason`, `RACE-bench`, `SWD-Bench`, `CCBench`, and `Terminal-Bench`.
