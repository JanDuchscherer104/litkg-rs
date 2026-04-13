# Architecture

`litkg-rs` is split into a normalized literature core plus thin graph adapters.

## Core Pipeline

1. `sync-registry`
   - read manifest JSONL
   - read BibTeX
   - merge into `PaperSourceRecord`
2. `download`
   - fetch arXiv source bundles and optional PDFs
3. `parse`
   - discover TeX root
   - inline includes
   - extract abstract, sections, captions, and citations
4. `materialize`
   - emit deterministic Markdown corpora for graph ingestion
5. `rebuild-graph` / `export-neo4j`
   - adapter-specific graph actions

## Registry Merge Contract

- Registry merge is deterministic: merged `PaperSourceRecord` rows are sorted by `paper_id` before writing JSONL.
- BibTeX entries match manifest rows primarily by arXiv id / `eprint` and secondarily by normalized title.
- Manifest-only rows stay in the registry so download and parse state can advance even when a paper does not yet have a citation key.
- Download and parse status live on the normalized record so the pipeline can resume from the registry without adapter-specific sidecar state.

## Download Contract

- Source download targets arXiv `e-print` bundles and optional PDFs derived from the normalized registry.
- Extraction accepts `tar.gz` first and falls back to plain `tar` for arXiv bundles that are not gzip-compressed.
- Archive extraction is path-safe: only normal relative file paths are allowed, and absolute paths, parent traversal, and empty paths are rejected.
- `overwrite = false` preserves existing extracted trees and PDFs; `overwrite = true` refreshes local assets from upstream.

## TeX Parser Contract

- Root discovery prefers `main.tex`; otherwise it selects the first `.tex` file that contains both `\documentclass` and `\begin{document}`.
- `\input{...}` and `\include{...}` are inlined recursively relative to the including file, with cycle protection through a visited-file set.
- The parser strips TeX comments before extraction, then emits:
  - abstract text from the `abstract` environment
  - section and subsection structure
  - figure and table captions
  - citation keys from `\cite*`-style commands
- Parser output is intentionally lossy for math-heavy regions: TeX command names are removed and the remaining text is whitespace-normalized into readable Markdown-oriented content rather than a full TeX AST.
- When no local TeX tree is available, the paper stays as a metadata-only record instead of failing the whole pipeline.

## Adapter Boundary

- `litkg-graphify`
  - writes graphify-friendly Markdown corpus, index pages, and a graphify manifest
- `litkg-neo4j`
  - writes an export bundle intended for later Neo4j/MCP ingestion

The core crate does not know anything about a client repo’s specific paths beyond the values supplied through `RepoConfig`.

## Adapter Output Contracts

- Graphify materialization writes one Markdown file per paper plus a deterministic `index.md` and `graphify-manifest.json` under `generated_docs_root`.
- Neo4j export writes `nodes.jsonl` and `edges.jsonl` under `neo4j_export_root`, keeping graph export as a file bundle rather than a live database dependency.
- `sink = both` is additive: the same normalized parsed paper set feeds both adapters without adapter-specific branching in the core model.

## Benchmark And Auto Research Layer

- `litkg-core::benchmark`
  - owns the benchmark catalog schema, benchmark result schema, validation rules, and autoresearch-target templates
- `litkg-cli`
  - validates benchmark catalogs and result bundles
  - renders concatenated autoresearch targets from selectable benchmark-aligned components
- `examples/benchmarks/`
  - stores benchmark metadata and sample result bundles used by local validation and target rendering

This layer is intentionally repo-independent. It describes evaluation targets and research-target composition rather than hard-coding one client repo's benchmark harness.

## Graphify Rebuild Contract

- Materialization must succeed even when graphify is absent.
- The adapter emits graphify-ready docs plus `graphify-manifest.json`.
- Rebuild is an optional post-step invoked through the repo config.
- A missing or failing graphify command should be reported as `skipped`, not treated as a hard pipeline failure.

## Viewer Direction

- The repository backlog should treat Apple Silicon-native Rust graph viewing as a first-class future capability.
- The current preferred direction is a native Rust viewer layer rather than a browser-only graph surface.
- Initial viewer requirements are:
  - local-first and Apple Silicon-friendly
  - compatible with the normalized paper model and optional embedding overlays
  - usable without making Neo4j or a browser the mandatory primary exploration surface
- Current shortlist:
  - `egui + eframe + petgraph` as the practical default
  - `gpui + wgpu` as the premium macOS-native path
  - `RDFGlance` as a ready-made explorer if an RDF export path is added
  - `Rerun` as an embedding/scene companion rather than the main structural explorer
