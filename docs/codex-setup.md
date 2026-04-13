# Codex Setup

`litkg-rs` already contains the repo-local surfaces Codex needs for the main file-output pipeline. The only extra setup is deciding whether you want:

- repo-local skills only
- repo-local skills plus the optional local KG MCP stack

The MCP path is useful for graph-backed exploration, but it is not required for `sync-registry`, `download`, `parse`, `materialize`, `rebuild-graph`, benchmark validation, or autoresearch-target rendering.

## Repo-Local Skills

Codex reads repo instructions from `AGENTS.md` and scans `.agents/skills` from the current working directory up to the repository root. For this repo, that means you can launch Codex anywhere under the checkout and it should discover:

- `.agents/skills/litkg-rs`
- `.agents/skills/autoresearch-litkg-rs`
- `.agents/skills/gh-issue-lifecycle`

The current skills already meet the minimal Codex shape: each skill directory has a `SKILL.md` with a `name` and `description`.

### When `agents/openai.yaml` Is Optional

You do not need `agents/openai.yaml` for basic repo-local skill discovery. Add it only when a skill needs extra metadata or a smoother install story, for example:

- dependency hints for tools the skill expects
- richer appearance metadata in Codex surfaces
- a more explicit packaging story for shared skill distribution

For `litkg-rs` today, the checked-in `SKILL.md` files are enough. The repo should prefer simple repo-local skills unless there is a concrete need for extra metadata or dependency declaration.

### Verify Skill Discovery

1. Launch Codex from the repo root or a subdirectory inside this checkout.

```bash
codex -C /absolute/path/to/litgraph-rs
```

2. Ask Codex to use the toolkit workflow, for example:

```text
Use the litkg-rs skill and summarize the pipeline plus required validation commands.
```

3. Discovery is working if the response correctly references the repo pipeline and validation expectations:

- `sync-registry`, `download`, `parse`, `materialize`, and adapter-specific graph actions
- bounded benchmark-driven autoresearch loops when you explicitly ask for `autoresearch-litkg-rs`
- `cargo fmt --all`
- `cargo test`
- `make agents-db`

If a newly edited skill does not appear immediately, restart Codex. Skill discovery is automatic, but live refresh can lag behind file changes.

## Optional Graphiti MCP Setup

Use this only if you want Codex to talk to the local Graphiti/Neo4j stack. The core toolkit does not depend on it.

Relevant repo surfaces:

- `docs/kg-stack.md`
- `scripts/kg/up.sh`
- `scripts/kg/down.sh`
- `scripts/kg/start_graphiti.sh`
- `scripts/kg/ingest_docs.sh`
- `.env.example`

### Prerequisites

- Docker with `docker compose`
- `git`
- `uv`
- optional: `OPENAI_API_KEY` for fuller upstream Graphiti behavior
- optional: Ollama if you want the local ingestion path in `scripts/kg/ingest_docs.sh`

Create a repo-local `.env` when you want to override the defaults from `.env.example`:

```bash
cp .env.example .env
```

Start Neo4j before starting the MCP server:

```bash
make kg-up
```

`scripts/kg/start_graphiti.sh` will clone the upstream Graphiti repo under `.cache/kg/graphiti` on first run, load `.env` or `.env.example`, and then start the MCP server in either `http` or `stdio` mode. For Codex, use `stdio`.

### Project-Scoped Codex Config

Codex supports project-scoped MCP config through `.codex/config.toml` in trusted projects. The following example keeps the MCP setup tied to this repo instead of your global Codex config:

```toml
[mcp_servers.litkg_graphiti]
command = "bash"
args = ["scripts/kg/start_graphiti.sh", "stdio"]
cwd = "/absolute/path/to/litgraph-rs"
env_vars = ["OPENAI_API_KEY"]
startup_timeout_sec = 60
```

Notes:

- Replace `/absolute/path/to/litgraph-rs` with the root of your local checkout.
- If you keep `OPENAI_API_KEY` in the repo-local `.env`, forwarding it through `env_vars` is optional.
- The server script already loads Neo4j defaults from `.env` or `.env.example`.
- Keep this in project-scoped `.codex/config.toml` if you want the MCP server attached only to `litkg-rs`.

### Optional Local Doc Ingestion

The Graphiti MCP server is more useful after the repo docs have been ingested into the local graph:

```bash
make kg-ingest-docs
```

Or scope ingestion explicitly:

```bash
./scripts/kg/ingest_docs.sh README.md docs/architecture.md .agents/AGENTS_INTERNAL_DB.md
```

That path uses the repo's Ollama-backed ingestion flow and is separate from the MCP server startup itself.

### Verify MCP Availability

1. Confirm Neo4j is up:

```bash
make kg-up
```

2. Confirm Codex sees the configured MCP server:

```bash
codex mcp list
codex mcp get litkg_graphiti
```

3. Start or restart Codex from this repo after adding `.codex/config.toml`.

4. If you want a stronger end-to-end check, ingest the repo docs and then ask Codex to use the local KG stack for repo exploration. The main pass condition is that the server starts cleanly and Codex does not report an MCP startup failure.

## Recommended Default

For most `litkg-rs` work, start with repo-local skills only. Add the Graphiti MCP server only when you specifically want:

- graph-backed exploration over ingested repo docs
- Neo4j-backed local experimentation
- a Codex session that can use the optional KG runtime directly

That keeps the repo aligned with its current boundary: file-output adapters and repo-local skills are primary, while MCP and Neo4j remain optional secondary surfaces.

## References

- [Codex AGENTS.md guide](https://developers.openai.com/codex/guides/agents-md)
- [Codex Skills](https://developers.openai.com/codex/skills)
- [Codex MCP](https://developers.openai.com/codex/mcp)
