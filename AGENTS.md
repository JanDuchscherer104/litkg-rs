# Litgraph RS Agent Guidance

This repository owns a repo-independent Rust toolkit for downloading research literature, normalizing paper metadata, parsing TeX sources, materializing KG-friendly corpora, and exporting to multiple graph backends.

## Sources Of Truth

- `AGENTS.md`: repo-wide policy and workflow.
- `CODEOWNER.md`: distilled human-owner intent, preferences, and durable high-signal requirements.
- `README.md`: operator setup, CLI usage, and integration examples.
- `docs/architecture.md`: architecture and adapter boundaries.
- `.agents/AGENTS_INTERNAL_DB.md`: stable internal operating context.
- `.agents/skills/litgraph-rs/SKILL.md`: agent-facing workflow for the toolkit.

## Repo Map

- `crates/litkg-core`: core models, config, registry merge, downloader, TeX parser, and materializer.
- `crates/litkg-cli`: CLI entrypoint.
- `crates/litkg-graphify`: graphify adapter/materializer.
- `crates/litkg-neo4j`: optional Neo4j export adapter.
- `examples/`: consumer repo configs such as `prml-vslam.toml`.
- `.agents/`: agent scaffolding, backlog, and internal references.

## Repo-Wide Rules

- Keep the toolkit repo-independent. Repo-specific assumptions belong only in adapter configs under `examples/` or client repos.
- Prefer deterministic outputs. Generated Markdown, registry JSONL, and export bundles should be stable under repeated runs.
- Do not widen the core model for one client repo unless the field is generally useful across consumers.
- Keep graphify as a file-output adapter and Neo4j as an optional export adapter. Do not make one adapter leak assumptions into the other.
- Before making a commit, run `cargo fmt`, `cargo test`, and `make agents-db`.
- Treat Apple Silicon macOS as a first-class optimization target for local UX, performance, and packaging decisions.
- When the user explicitly asks to generalize an instruction into scaffolding, capture it in the most specific durable surface that fits: root `AGENTS.md`, `.agents/AGENTS_INTERNAL_DB.md`, backlog entries, or `.agents/skills/litgraph-rs/SKILL.md`.
- If the user marks content inside `<...>` and explicitly says it should be added to `AGENTS.md`, distill that content into concise, cleaned-up repo guidance rather than copying it verbatim.
- If the instruction is broader human-owner intent rather than repo policy, put the distilled version into `CODEOWNER.md`.
