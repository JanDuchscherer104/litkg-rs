# AGENTS Internal Database

Purpose: a compact operational memory surface for this toolkit repo. Record stable decisions, repo boundaries, and important maintenance facts here instead of scattering them across implementation files.

## Mission Snapshot

- Build a repo-independent Rust toolkit for literature download, parsing, and graph-oriented materialization.
- Keep the core model generic across client repos.
- Treat graph adapters as replaceable sinks over the same normalized paper model.

## Stable Ownership Snapshot

- `litkg-core` owns registry schema, download, TeX parsing, and materialization primitives.
- `litkg-graphify` owns graphify corpus output only.
- `litkg-neo4j` owns optional Neo4j export bundle generation only.
- `litkg-cli` owns operator-facing orchestration and command plumbing.

## Current Stable Facts

- `prml-vslam` is the first consumer but not the design center.
- `graphify` is the preferred first adapter because `prml-vslam` already uses `graphify-out/`.
- Neo4j/MCP export remains optional and secondary for the initial rollout.
- Apple Silicon macOS is a primary operator platform and should bias native viewer, performance, packaging, and overnight-agent workflow decisions.
- The scaffolding should support explicit capture of user instructions into durable repo memory or skills when requested.
- Benchmark metadata, benchmark-result validation, and benchmark-driven autoresearch-target composition now live in `litkg-core::benchmark` and are surfaced through `litkg-cli`.
- The bounded benchmark-driven autoresearch operator workflow lives under `.agents/skills/autoresearch-litkg-rs/` and writes run briefs/results under `.logs/autoresearch/`.
- The repo-local code review workflow lives under `.agents/skills/code-review-litkg-rs/` and covers working-tree review, GitHub PR review, and autoresearch winner review gates.
- The local Neo4j and Graphiti runtime scripts under `scripts/kg/` are reusable repo tooling, not client-specific pipeline logic.
- GitHub issue creation and issue resolution now have a dedicated repo-local skill plus `.github` templates for issue intake and PR-based closure.
