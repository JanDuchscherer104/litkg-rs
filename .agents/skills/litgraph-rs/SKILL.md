---
name: litgraph-rs
description: Use when working on the standalone Rust literature KG toolkit, especially registry merge, arXiv download, TeX parsing, graphify materialization, Neo4j export, and client-repo adapter configs.
---

# litgraph-rs

## Grounding

Before changing the toolkit:

1. Read `AGENTS.md`
2. Read `README.md`
3. Read `docs/architecture.md`
4. Read `.agents/AGENTS_INTERNAL_DB.md`
5. Inspect `.agents/issues.toml` and `.agents/todos.toml`

## Workflow

- Keep `litkg-core` repo-independent.
- Put client-repo assumptions in config files under `examples/`.
- Treat graph adapters as sinks over the same normalized paper model.
- Prefer deterministic output over aggressive parsing heuristics.
- When a user explicitly asks to generalize an instruction, capture it into the smallest durable scaffold surface that fits:
  - root `AGENTS.md` for repo-wide policy
  - `.agents/AGENTS_INTERNAL_DB.md` for stable operating facts
  - `.agents/issues.toml` or `.agents/todos.toml` for tracked follow-up work
  - this skill for repeatable operator workflow
- Keep Apple Silicon considerations explicit when evaluating viewers, long-running agents, and local performance-sensitive tooling.

## Validation

- `cargo fmt --all`
- `cargo test`
- `make agents-db`
