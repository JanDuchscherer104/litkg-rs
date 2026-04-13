# Code Owner Signal

Purpose: preserve high-signal human-owner requirements, preferences, and durable operating expectations that should survive beyond a single chat session. This file is intentionally concise and policy-oriented. It is not a backlog, implementation design, or task log.

## Usage

- Read this file before making architecture, UX, agent-loop, or platform decisions.
- Distill new human-owner guidance into this file when it is durable, broadly applicable, and not better placed in a narrower skill or issue.
- Prefer concise rules and priorities over raw prompt transcripts.
- Treat this file as complementary to `AGENTS.md`:
  - `AGENTS.md` owns repo policy and working rules.
  - `CODEOWNER.md` owns distilled human intent, priorities, and preferences.

## Product Direction

- Build `litgraph-rs` as a **repo-independent** toolkit. It must not assume one client repo.
- Keep the first consumer (`prml-vslam`) thin. Client repos should supply config and adapters, not own the core parsing or graph logic.
- Keep `graphify` as the preferred first local graph adapter for current client repos when that matches their existing workflow.
- Keep Neo4j/MCP support optional and secondary unless a client repo explicitly promotes it to primary.

## Technology Direction

- The toolkit should be written in **Rust**.
- Native local tooling matters. Prefer Rust-native binaries, viewers, and exploration paths over browser-only or Python-heavy control planes when practical.
- Apple Silicon macOS is a primary operator platform and should strongly influence local performance, packaging, and viewer choices.
- A native Rust graph viewer and exploration tool is a strategic requirement, not an optional nicety.

## Literature And KG Requirements

- Support both:
  - full download and parsing of manifest-backed papers
  - metadata-only inclusion of BibTeX-only references
- Preserve provenance rigorously:
  - source manifest
  - BibTeX key
  - download status
  - parse status
  - generated output paths
- Make graph outputs deterministic and rebuildable.
- Support easy local querying over the KG using Ollama-hosted local models, especially embeddings and Gemma-class models.

## Agent And Automation Requirements

- The system should become strong enough to run valuable long-lived Codex workflows overnight.
- Auto Research should be a native capability, not an afterthought.
- Long-running agent work should be organized around **target functions** with explicit success criteria, review gates, and resumable state.
- Good target functions include:
  - resolving existing to-dos
  - proposing new to-dos that survive review
  - maintaining and updating the agents/craft database
  - reducing source LOC when it improves quality
  - increasing test quantity and quality
  - proposing and validating new high-value automation targets
- Every pull request should eventually support a sub-agent review fan-out model with review aggregation and resolution tracking.
- The system should support a review loop where agents can inspect UI or visual outputs that matter to humans and suggest improvements.

## Human-Signal Capture

- There must be an easy mode where subsequent user statements can be interpreted as candidate scaffold material.
- When the user explicitly signals that a statement should be generalized into scaffolding, the system should:
  1. distill it
  2. decide the right destination
  3. persist it into the smallest durable surface that fits
- Possible destinations include:
  - `CODEOWNER.md` for broad human-owner intent
  - `AGENTS.md` for repo policy
  - `.agents/AGENTS_INTERNAL_DB.md` for stable operational facts
  - a skill file for repeatable workflows
  - backlog items for future implementation work
- Content marked inside `<...>` should be treated as candidate distilled guidance, not copied verbatim.

## Repo Bootstrap Expectations

- The toolkit should eventually provide strict bootstrap templates for:
  - `AGENTS.md`
  - `README.md`
  - `REQUIREMENTS.md`
- These templates should be informed by the stronger patterns already used in:
  - `~/repos/NBV`
  - `~/repos/prml-vslam`

## External Comparison

- Track `Memory-Palace-MCP` as an explicit comparison target for MCP-native memory UX and long-running agent-memory design.
