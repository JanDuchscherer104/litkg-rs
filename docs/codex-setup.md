# Codex Setup

`litkg-rs` works in Codex with repo-local scaffolding alone. The local Graphiti and Neo4j stack is optional and secondary to the core file-output pipeline.

## Repo-Scoped Skills

Codex in this repo reads the root `AGENTS.md` plus the repo skills under `.agents/skills/`.

Current repo skills:

- `litkg-rs`: grounding and validation workflow for the toolkit itself
- `gh-issue-lifecycle`: GitHub issue creation, sync, and resolution workflow
- `code-review-litkg-rs`: working-tree, PR, and autoresearch review gate
- `autoresearch-litkg-rs`: bounded benchmark-driven research loop

Optional skill metadata lives in `agents/openai.yaml` inside an individual skill directory.

Use plain `SKILL.md` when the instructions are enough by themselves. Add `agents/openai.yaml` when you want a clearer display name, a short launcher description, a default prompt, or explicit dependency hints in Codex. In this repo, `litkg-rs` stays instruction-only because it mainly provides grounding, while `autoresearch-litkg-rs`, `code-review-litkg-rs`, and `gh-issue-lifecycle` already carry `agents/openai.yaml` metadata for smoother invocation.

## Verify Skill Discovery

1. Open the repo from the repository root so `AGENTS.md` and `.agents/skills/` are in scope.
2. Ask Codex to summarize the repo instructions or invoke one of the explicit skills, for example `Use $litkg-rs to inspect the current workspace.`
3. Confirm that Codex can name the four repo skills above without extra setup.

## Optional MCP Path For The Local KG Stack

Only use this if you want Codex to talk to the local Graphiti and Neo4j stack. The main `litkg-rs` pipeline does not require MCP.

### Prerequisites

- `docker` with `docker compose`
- `uv`
- `git`
- optional `OPENAI_API_KEY` if you want the upstream Graphiti MCP server to use OpenAI-backed features
- optional local `.env`; otherwise the scripts fall back to `.env.example`

### Startup Order

1. Review `.env.example` and copy it to `.env` only if you need local overrides.
2. Start Neo4j with `make kg-up`.
3. Add the Graphiti MCP server to Codex with a project-scoped config or the CLI.
4. Optionally run `make kg-update KG_SRC_DIR=crates/litkg-core` or `make kg-ingest-docs` after the server is available.

### Project-Scoped `.codex/config.toml` Example

Create `.codex/config.toml` at the repo root with:

```toml
[mcp_servers.graphiti_litkg]
command = "bash"
args = ["-lc", "cd \"$(git rev-parse --show-toplevel)\" && ./scripts/kg/start_graphiti.sh stdio"]
env_vars = ["OPENAI_API_KEY"]
startup_timeout_sec = 30
```

This keeps the repo command concrete while resolving the current checkout root at runtime. `./scripts/kg/start_graphiti.sh` reads `.env` first and falls back to `.env.example`, so the repo defaults from the active checkout are used automatically.

### CLI Alternative

From the repo root:

```bash
codex mcp add graphiti-litkg --env OPENAI_API_KEY -- ./scripts/kg/start_graphiti.sh stdio
```

That uses the same repo script and default environment flow as the project-scoped config above.

## Verify MCP Availability

1. Confirm the repo surfaces exist:

   ```bash
   test -x ./scripts/kg/start_graphiti.sh
   test -f .env.example
   test -f infra/neo4j/docker-compose.yml
   ```

2. Confirm Neo4j is running:

   ```bash
   make kg-up
   ```

3. In Codex, use `/mcp` or `codex mcp list` and check that `graphiti_litkg` is active.
4. Ask Codex to list active MCP servers or inspect the repo KG runtime files. If the server fails to start, run `./scripts/kg/start_graphiti.sh stdio` manually once from the repo root to surface missing dependencies.

## Related Repo Surfaces

- `docs/kg-stack.md`
- `.agents/skills/litkg-rs/SKILL.md`
- `.agents/skills/gh-issue-lifecycle/SKILL.md`
- `.agents/skills/code-review-litkg-rs/SKILL.md`
- `.agents/skills/autoresearch-litkg-rs/SKILL.md`
