# ISSUE-0014 Draft

## Title

Document repo-scoped Codex skill and MCP setup

## Summary

Add a focused operator guide that explains how `litkg-rs` works inside Codex: repo instruction layering, repo-scoped skill discovery, optional `agents/openai.yaml` metadata, and the optional project-scoped MCP path for the local Graphiti/Neo4j stack.

## Motivation

- The repo already has Codex-facing scaffolding, but the operator setup path is still implicit.
- Repo-local skills are useful only when operators know where Codex discovers them and which skill is the main entrypoint.
- The local KG stack is reusable, but it should remain an optional secondary surface rather than turning MCP into a prerequisite for the core toolkit workflow.

## Proposed Scope

1. Document Codex instruction and skill discovery for this repo.
2. Establish `litkg-rs` as the main repo skill and document specialized helper skills as auxiliary.
3. Add a project-scoped `.codex/config.toml` example for `scripts/kg/start_graphiti.sh stdio`.
4. Add an operator verification flow for both skill discovery and MCP availability.
5. Sync the local `.agents` backlog with `ISSUE-0014`.

## Acceptance Criteria

- The repo contains a Codex setup guide linked from `README.md`.
- The guide explains skill discovery from `.agents/skills` and distinguishes the main repo skill from auxiliary skills.
- The guide contains a concrete project-scoped MCP example tied to repo-owned scripts and defaults.
- The guide states clearly that MCP and Neo4j are optional and secondary relative to the main file-output pipeline.
- The local backlog includes `ISSUE-0014` and a linked TODO entry.
