# Feature Overview

`litkg-rs` is a local-first Rust toolkit for turning research-paper metadata, source bundles, repo docs, and benchmark evidence into deterministic artifacts that can be inspected, exported, and fed into graph tooling.

For a rich map of external tools, backend choices, repo integration points, and
Mermaid diagrams, see [tooling-and-backends.md](tooling-and-backends.md).

## Main Workflows

| Workflow | What it does | Primary commands |
| --- | --- | --- |
| Literature registry | Merge manifest JSONL and BibTeX into stable `PaperSourceRecord` rows. | `sync-registry`, `make litkg-sync` |
| Asset download | Download arXiv source bundles and optional PDFs with safe archive extraction. | `download`, `make litkg-download` |
| TeX parsing | Parse local TeX trees into abstracts, sections, captions, raw citations, and normalized cited-reference metadata. | `parse`, `make litkg-parse` |
| KG materialization | Write one Markdown file per paper plus deterministic graphify-oriented index/manifest output. | `materialize`, `make litkg-materialize` |
| Neo4j export | Write `nodes.jsonl` and `edges.jsonl` import bundles from the same parsed paper model. | `export-neo4j`, `make litkg-export-neo4j` |
| Native graph view | Open a Rust `egui`/`petgraph` inspector over the Neo4j export bundle. | `inspect-graph`, `make inspect-graph` |
| Capability snapshot | Show what is implemented, configured, generated, runtime-ready, and still missing for one config. | `capabilities`, `make capabilities` |
| Corpus inspection | Compute stats, search records/content, and inspect one paper with local citation neighborhood. | `stats`, `search`, `show-paper` |
| Semantic Scholar | Enrich registries, resolve papers, search papers, and get recommendations through official REST APIs. | `enrich-semantic-scholar`, `semantic-scholar-*` |
| Local KG runtime | Start Neo4j, index code with CodeGraphContext, ingest docs with Graphiti, and refresh embedding links. | `make kg-*` |
| Benchmarks | Validate benchmark catalogs, inspect support, run adapters, and normalize result bundles. | `validate-benchmarks`, `benchmark-support`, `run-benchmarks` |
| Auto research targets | Render or promote benchmark-grounded research targets and issue-ready drafts. | `render-autoresearch-target`, `promote-benchmark-results`, `sync-autoresearch-target-issue` |
| Downstream setup | Provide strict starter templates for client repo `AGENTS.md`, `README.md`, and `REQUIREMENTS.md`. | `templates/bootstrap/` |

## Core Pipeline

For a configured repo:

```bash
cargo run -p litkg-cli -- sync-registry --config examples/prml-vslam.toml
cargo run -p litkg-cli -- download --config examples/prml-vslam.toml
cargo run -p litkg-cli -- parse --config examples/prml-vslam.toml
cargo run -p litkg-cli -- materialize --config examples/prml-vslam.toml
cargo run -p litkg-cli -- export-neo4j --config examples/prml-vslam.toml
```

The shortcut is:

```bash
make litkg-pipeline LITKG_CONFIG=examples/prml-vslam.toml
```

Use `--download-pdfs` or `LITKG_PIPELINE_ARGS="--download-pdfs"` only when PDFs should be fetched.

## Capability Snapshot

Use `capabilities` before or after a pipeline run to see the repo-specific
support surface without mutating generated state:

```bash
cargo run -p litkg-cli -- capabilities --config examples/prml-vslam.toml
cargo run -p litkg-cli -- capabilities --config examples/prml-vslam.toml --format json
make capabilities LITKG_CONFIG=examples/prml-vslam.toml
```

The default snapshot checks configured files and generated artifacts only.
Add `--check-runtime` when you also want shallow local checks for Docker/Neo4j,
`uv`, CodeGraphContext, Graphiti helper scripts, and Ollama.

## Configuration Shape

A minimal client config points at manifest, BibTeX, asset roots, generated output roots, and the sink:

```toml
manifest_path = "docs/literature/sources.jsonl"
bib_path = "docs/references.bib"
tex_root = "docs/literature/tex-src"
pdf_root = "docs/literature/pdf"
generated_docs_root = ".agents/kg/generated/literature"
registry_path = ".agents/kg/generated/literature/registry.jsonl"
parsed_root = ".agents/kg/generated/literature/parsed"
neo4j_export_root = ".agents/kg/generated/neo4j-export"
sink = "both"
download_pdfs = false

[semantic_scholar]
enabled = true
api_key_env = "SEMANTIC_SCHOLAR_API_KEY"
min_interval_s = 1.05
```

Keep repo-specific paths in client configs. Keep reusable parser, enrichment, and adapter behavior in `litkg-rs`.

## Literature Parsing And Citation Links

The TeX parser is deliberately deterministic and lossy: it extracts graph-friendly structure instead of trying to be a full TeX engine. It currently extracts:

- root title, abstract, sections, subsections, figure captions, and table captions
- `\input{...}` and `\include{...}` trees with cycle protection
- citation keys from common natbib/biblatex forms such as `\cite`, `\citep`, `\citet`, `\parencite`, and `\textcite`
- source-local BibTeX metadata for cited keys: title, DOI, arXiv ID, and URL

Raw citation keys are preserved. Additional `CITES_PAPER` edges are inferred by exact key, DOI, arXiv ID, and normalized title so different TeX source trees can cite the same paper with different local keys.

The detailed parsing and citation-resolution flow is documented in
[architecture.md](architecture.md#tex-parser-contract), with the external tool
map in [tooling-and-backends.md](tooling-and-backends.md).

## Semantic Scholar Usage

Set the key in the environment, not in config:

```bash
export SEMANTIC_SCHOLAR_API_KEY=...
```

Then use:

```bash
cargo run -p litkg-cli -- enrich-semantic-scholar --config examples/prml-vslam.toml
cargo run -p litkg-cli -- semantic-scholar-paper --paper ARXIV:2406.10224 --format json
cargo run -p litkg-cli -- semantic-scholar-search --query '"next best view" + reconstruction' --limit 10
cargo run -p litkg-cli -- semantic-scholar-recommend --positive <paperId> --limit 25
```

The client uses official Semantic Scholar REST endpoints, sends the key as `x-api-key`, throttles to about one request per second by default, and retries 429/5xx responses.

## Local KG Runtime

The file-output pipeline does not require live services. Use the runtime stack only when you need local graph/code/doc enrichment:

```bash
make kg-up
make kg-index KG_SRC_DIR=crates/litkg-core
make kg-ingest-docs
make kg-enrich
make kg-down
```

For external repos:

```bash
make kg-update KG_CODE_REPO_ROOT=/path/to/repo KG_SRC_DIR=aria_nbv/aria_nbv
```

Runtime caches live under ignored `.cache/kg` and `.data/kg` paths.

## Interactive Exploration

Generate or refresh the export bundle:

```bash
make litkg-export-neo4j LITKG_CONFIG=examples/prml-vslam.toml
```

Open the native viewer:

```bash
cargo run -p litkg-cli -- inspect-graph --config examples/prml-vslam.toml
```

The viewer loads `nodes.jsonl` and `edges.jsonl`, lays out papers, sections, citations, memory surfaces, and enrichment edges, and provides search, selection, neighbor inspection, pan, zoom, and filter controls.

## Benchmark And Auto Research Usage

Validate catalog and sample results:

```bash
make benchmark-validate
```

Inspect what can run locally:

```bash
make benchmark-support
```

Run configured adapters:

```bash
make benchmark-run BENCHMARK_RUN_PLAN=/abs/path/to/run-plan.toml
```

Render or promote research targets:

```bash
make autoresearch-target
cargo run -p litkg-cli -- promote-benchmark-results \
  --catalog examples/benchmarks/kg.toml \
  --results examples/benchmarks/sample-results.toml \
  --target-id kg_navigation_improvement \
  --status needs_improvement \
  --format github-issue
```

## Development Checks

Use these before committing toolkit changes:

```bash
cargo fmt --all --check
cargo test
make agents-db
```

For runtime helper changes:

```bash
make kg-smoke
```

For benchmark/schema changes:

```bash
make benchmark-validate
make benchmark-support
```
