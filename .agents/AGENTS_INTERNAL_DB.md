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
- Benchmark integration inspection, normalized command-adapter execution, and deterministic result promotion now live alongside the benchmark schema in `litkg-core` and are surfaced through `litkg-cli`.
- Rendered autoresearch targets now classify selected benchmark runs into deferred validation-only or successful evidence versus promotable recognized execution failure runs.
- `render-autoresearch-target` supports `markdown`, `issue`/`github-issue`, and `json` outputs over the same deterministic target selection, and `sync-autoresearch-target-issue` can publish the GitHub-issue render through `gh`.
- Benchmark validation rejects unknown run statuses until they are classified into promotion or control-plane behavior explicitly.
- JSON target renders now expose promotion counts, sanitized run summary text, and structured score evidence.
- The bounded benchmark-driven autoresearch operator workflow lives under `.agents/skills/autoresearch-litkg-rs/` and writes run briefs, results, and resumable state under `.logs/autoresearch/`.
- The autoresearch skill now carries repo-local helper scripts for deterministic resume summaries and next-trial allocation over `.logs/autoresearch/<tag>/`.
- For machine consumption, `result_summaries` is the canonical normalized result view; raw benchmark bundles remain source input, not the automation contract.
- The repo-local code review workflow lives under `.agents/skills/code-review-litkg-rs/` and covers working-tree review, GitHub PR review, and autoresearch winner review gates.
- The local Neo4j and Graphiti runtime scripts under `scripts/kg/` are reusable repo tooling, not client-specific pipeline logic.
- GitHub issue creation and issue resolution now have a dedicated repo-local skill plus `.github` templates for issue intake and PR-based closure.
- Repo-scoped Codex setup for skills and the optional Graphiti/Neo4j MCP path is documented in `docs/codex-setup.md`.
- Strict downstream bootstrap templates for `AGENTS.md`, `README.md`, and `REQUIREMENTS.md` live under `templates/bootstrap/`.
- Issue work now requires a short TODO or status comment on the GitHub issue that states the planned change and whether execution is happening in the local checkout or an isolated worktree.
