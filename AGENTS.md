# Litkg RS Agent Guidance

This repository owns a repo-independent Rust toolkit for downloading research literature, normalizing paper metadata, parsing TeX sources, materializing KG-friendly corpora, and exporting to multiple graph backends.

## Sources Of Truth

- `AGENTS.md`: repo-wide policy and workflow.
- `CODEOWNER.md`: distilled human-owner intent, preferences, and durable high-signal requirements.
- `README.md`: operator setup, CLI usage, and integration examples.
- `docs/architecture.md`: architecture and adapter boundaries.
- `.agents/AGENTS_INTERNAL_DB.md`: stable internal operating context.
- `.agents/skills/litkg-rs/SKILL.md`: agent-facing workflow for the toolkit.
- `.agents/skills/code-review-litkg-rs/SKILL.md`: agent-facing workflow for working-tree review, PR review, and autoresearch review gates.
- `.agents/skills/autoresearch-litkg-rs/SKILL.md`: agent-facing bounded autoresearch workflow for benchmark-driven litkg-rs work.
- `.agents/skills/gh-issue-lifecycle/SKILL.md`: agent-facing GitHub issue creation and resolution workflow.

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
- Use `.agents/skills/code-review-litkg-rs/SKILL.md` as the default workflow for working-tree review, pull-request review, and candidate autoresearch winner review before promoting substantial changes.
- Before making a commit, run `cargo fmt`, `cargo test`, and `make agents-db`.
- Treat Apple Silicon macOS as a first-class optimization target for local UX, performance, and packaging decisions.

## Instruction Capture Mode

- When the user explicitly asks to generalize an instruction into scaffolding, capture it in the smallest durable surface that fits.
- Route repo policy and workflow to `AGENTS.md`.
- Route broader human-owner intent and preferences to `CODEOWNER.md`.
- Route stable operating facts to `.agents/AGENTS_INTERNAL_DB.md`.
- Route specialized repeatable workflows to `.agents/skills/*/SKILL.md`.
- Route not-yet-implemented follow-up work to `.agents/issues.toml` and `.agents/todos.toml`.
- When working a GitHub issue, leave a short TODO or status comment on the issue that states the intended change and whether execution is happening in the local checkout or an isolated worktree.
- Treat content inside `<...>` as candidate scaffold material and distill the durable rule rather than copying the raw text verbatim.
