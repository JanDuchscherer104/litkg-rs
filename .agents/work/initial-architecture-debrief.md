# Initial Architecture Debrief

Date: 2026-04-13

Goal: create a standalone Rust-first literature KG toolkit that can be consumed by `prml-vslam` without embedding repo-specific parsing logic in the client repo.

Initial decisions:

- keep the normalized literature model in `litkg-core`
- keep graphify as the primary first adapter because the first consumer repo already uses `graphify-out/`
- keep Neo4j export secondary and optional
- model the repo-specific boundary as TOML configuration rather than code forks
- treat graphify rebuild as an optional post-step and emit a machine-readable manifest even when the rebuild is skipped
- treat Apple Silicon as a primary local target and prefer Rust-native viewer paths such as `egui + petgraph` before browser-only exploration

Immediate backlog:

- registry merge
- arXiv downloader
- TeX parser
- graphify materialization
- optional Neo4j export bundle
- native viewer/explorer design
- Auto Research and overnight Codex execution loop
