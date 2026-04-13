# litkg-rs

`litkg-rs` is a repo-independent Rust toolkit for:

- merging paper manifests and BibTeX into a normalized literature registry
- downloading arXiv source bundles and optional PDFs
- parsing TeX sources into structured paper records
- materializing KG-friendly Markdown corpora
- exporting to multiple graph adapters, including graphify-oriented corpora and optional Neo4j bundles
- validating benchmark catalogs, inspecting benchmark integration readiness, running benchmark adapters, and composing benchmark-driven auto research targets

The first consumer is `prml-vslam`, but this repository is intentionally not tied to any single client repo.

## Human Signal

- [CODEOWNER.md](CODEOWNER.md) stores distilled human-owner requirements and preferences that should persist beyond one chat session.
- [AGENTS.md](AGENTS.md) stores repo policy and operating rules for agents.

## Workspace

- `litkg-core`: normalized paper model, registry merge, downloader, parser, and materializer
- `litkg-cli`: CLI binary `litkg`
- `litkg-graphify`: graphify-friendly Markdown corpus adapter
- `litkg-neo4j`: optional Neo4j export bundle adapter

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
cargo run -p litkg-cli -- validate-benchmarks --catalog examples/benchmarks/kg.toml --results examples/benchmarks/sample-results.toml
cargo run -p litkg-cli -- benchmark-support --catalog examples/benchmarks/kg.toml --integrations examples/benchmarks/integrations.toml
cargo run -p litkg-cli -- render-autoresearch-target --catalog examples/benchmarks/kg.toml --results examples/benchmarks/sample-results.toml --target-id kg_navigation_improvement
cargo run -p litkg-cli -- promote-benchmark-results --catalog examples/benchmarks/kg.toml --results examples/benchmarks/sample-results.toml --status needs_improvement --metric-threshold correctness<=0.7 --metric-threshold pass_at_1<=0.5 --format github-issue
```

If the consumer repo does not have `graphify` installed, `rebuild-graph` degrades cleanly and leaves the generated corpus intact.

## Backlog

Use the local `.agents/` backlog:

```bash
make agents-db
```

Common operator helpers:

```bash
make loc
make loc-rs
make lint-check
make ci
make litkg-sync
make litkg-pipeline LITKG_CONFIG=examples/prml-vslam.toml LITKG_PIPELINE_ARGS="--download-pdfs"
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
make kg-update KG_CODE_REPO_ROOT=/Users/jd/repos/NBV KG_SRC_DIR=aria_nbv/aria_nbv
make kg-graphiti GRAPHITI_MODE=stdio
make kg-smoke
make kg-ingest-docs
make kg-down
```

For external code repos, set `KG_CODE_REPO_ROOT` to the external repo root. This is the main path for indexing Python packages via CodeGraphContext from outside `litkg-rs`.

## Benchmarks And Auto Research Targets

Benchmark metadata, executable integration support, and autoresearch-target composition now live in this repo:

- benchmark catalog: `examples/benchmarks/kg.toml`
- benchmark integration matrix: `examples/benchmarks/integrations.toml`
- sample results bundle: `examples/benchmarks/sample-results.toml`
- validation command: `make benchmark-validate`
- local readiness inspection: `make benchmark-support`
- benchmark execution: `make benchmark-run BENCHMARK_RUN_PLAN=/abs/path/to/run-plan.toml`
- autoresearch-target rendering: `make autoresearch-target`
- result-promotion entrypoint: `cargo run -p litkg-cli -- promote-benchmark-results --catalog ... --results ...`

The catalog covers `SWE-Bench Pro`, `SWE-QA-Pro`, `CodeRepoQA`, `StackRepoQA`, `RepoReason`, `RACE-bench`, `SWD-Bench`, `CCBench`, and `Terminal-Bench`.

The integration matrix records the current upstream state for each benchmark. Some entries already have public runnable harnesses (`SWE-Bench Pro`, `RepoReason`, `Terminal-Bench`), while others are currently dataset-only, paper-only, or site-only and therefore require a custom local command adapter that writes normalized JSON back into `litkg-rs`.

Validated result bundles can now be promoted directly into deterministic autoresearch drafts. The promotion command filters runs by benchmark, status, and metric thresholds, then emits markdown, JSON, or GitHub-issue-ready output without hand-editing the underlying target templates.
