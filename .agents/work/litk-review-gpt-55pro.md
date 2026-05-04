# User Prompt

please inspect our usage of .agents/external/litkg-rs!

how can we improve it and make it litkg the semantic search, consolidation, memory layer for argents working on our aria-nbv project!

# Agent reply

I inspected the current GitHub snapshot, so this reflects the committed repo state I can see; local uncommitted changes may differ.

## Bottom line

You already have a solid litkg-rs integration, but it is currently used mostly as a **KG/literature/code-index operator tool**. To make it the **semantic search, consolidation, and memory layer for ARIA-NBV agents**, the next step is to promote it from “a set of KG commands” into a **mandatory agent context router**:

```text
agent asks question / starts task
        ↓
litkg retrieves relevant code/docs/papers/memory/backlog/experiments
        ↓
agent answers or works with cited, ranked project context
        ↓
agent writes debrief / updates backlog / proposes canonical memory changes
        ↓
litkg re-ingests and consolidates
```

The key missing pieces are not submodule setup or basic ingestion. Those exist. The missing pieces are: a stable query interface for agents, richer memory-source coverage, trust/freshness ranking, automatic consolidation proposals, and a rule that agents must consult litkg for broad or cross-surface project work.

---

## What is already in place

The submodule is correctly declared at `.agents/external/litkg-rs`, pointing to `JanDuchscherer104/litkg-rs` in `.gitmodules`.

The root `AGENTS.md` already routes KG/literature/code-graph work to the `semantic-scholar-litkg` skill and tells agents to keep repo-independent implementation inside `.agents/external/litkg-rs`. It also exposes Make targets such as `kg-sync`, `kg-materialize`, `kg-semantic-enrich`, `kg-export-neo4j`, and `kg-index-code`.

The dedicated skill is already well framed: it tells agents to start in ARIA-NBV, then hand implementation to the litkg-rs submodule, and it defines graphify as the durable sink, Neo4j as optional traversal/export, CodeGraphContext as code-index enrichment, Graphiti as optional temporal ingestion, and mempalace as optional memory/KG integration.

The ARIA-specific TOML profile is already quite rich. It includes literature, Semantic Scholar enrichment, agent guidance, agent memory, Python sources, Quarto docs, Typst docs, glossary artifacts, external-doc library IDs, web ingestion toggles, backend toggles, and storage roots.

The wrapper scripts are thin and appropriate. `scripts/kg/index_code.sh` delegates to the litkg-rs code-index helper and passes `KG_CODE_REPO_ROOT` to keep indexing rooted in ARIA-NBV.  `scripts/kg/ingest_docs.sh` delegates to litkg-rs Graphiti doc ingestion with a default ARIA doc/memory list.  `scripts/kg/ingest_papers.sh` delegates sync/download/parse/materialize/export/Semantic Scholar enrichment to the litkg CLI through `.configs/litkg.toml`.

The Makefile exposes KG targets alongside context, memory, glossary, and LRZ helpers, so the operator surface is discoverable from the root.

The `.agents/kg/README.md` states the intended representation policy: graphify-style Markdown/JSON as durable output, Neo4j JSONL bundles as traversal representation, CodeGraphContext for Python symbol-level queries, optional Graphiti, and MemPalace as separate memory mining rather than the structural KG backend.

litkg-rs itself is already a repo-independent toolkit for registry merge, paper download, TeX parsing, materialization, Neo4j export, Semantic Scholar enrichment, local corpus search/stats/show-paper, code/doc KG helpers, benchmarks, and autoresearch targets.  Its own `AGENTS.md` explicitly requires repo independence and says client-specific assumptions belong in configs, not toolkit code.

The architecture is also already aligned with what you want: a deterministic file-output core, graphify/Neo4j adapters, Semantic Scholar enrichment, read-only inspection commands, and optional runtime KG layers.  The tooling map confirms the local-first philosophy: source files and TOML config are source of truth; generated Markdown, JSONL, Neo4j data, Graphiti episodes, embeddings, and viewer state are derived artifacts.

The previous integration debrief says the ARIA profile found 56 papers, 9 parsed TeX assets, 9 PDFs, 238 sections, 86 figures, 36 tables, 412 citations, Semantic Scholar enriched 24 records, and the generated Neo4j export had 772 nodes and 727 edges.

So: the integration exists. The opportunity is to make it the **default cognitive substrate** for agents.

---

## Main diagnosis

Right now litkg is mostly a **tooling island**:

```text
make kg-sync
make kg-materialize
make kg-index-code
make kg-ingest-docs
make kg-export-neo4j
```

That is useful, but agents still mostly operate by manually reading `AGENTS.md`, memory files, generated context, docs, and code. The KG is not yet the first-class “ask the project memory” interface.

To become the semantic search / consolidation / memory layer, litkg needs to answer questions like:

```text
What is the current thesis spine?
What did we decide about entity-aware RRI?
Which files define the current oracle RRI contract?
What are the known CW90 / pose-frame gotchas?
Which TODOs block multi-step rollouts?
Which papers support RRI over coverage?
Which code symbols implement candidate generation?
Which stale docs contradict PROJECT_STATE?
What did the last agent do, and what canonical updates are pending?
```

That requires not just ingestion, but **retrieval, authority ranking, contradiction detection, and memory promotion**.

---

## Biggest gaps to fix

### 1. No single agent-facing query command

litkg-rs has `search`, `stats`, `show-paper`, `capabilities`, `inspect-graph`, Semantic Scholar commands, and local KG helpers.  ARIA-NBV has wrapper targets for ingestion and export, but no obvious root command like:

```bash
make kg-query KG_QUERY="What is the current RRI contract?"
make kg-brief KG_TOPIC="entity-aware RRI"
make kg-route KG_TASK="debug VIN offline store"
```

That means the KG is not yet the natural first step for agents.

### 2. Agent memory ingestion is too narrow

`.configs/litkg.toml` includes `.agents/memory/state/*.md` and the active TOML backlog files under `[sources.agent_memory]`.  That covers canonical state and active work, but not enough episodic or procedural memory.

It should also ingest, with lower authority:

```text
.agents/memory/history/**/*.md
.agents/work/**/*.md
.agents/references/**/*.md
.agents/skills/**/*.md
.agents/archive/**/*.md
docs/_generated/context/*.md
configs and experiment manifests
cache manifests / split manifests
W&B or run-summary artifacts, if curated
```

The distinction matters: `PROJECT_STATE.md` is canonical current truth; debriefs are episodic evidence; old scratchpads are low-authority historical context.

### 3. No explicit source authority / freshness model

The repo already has a source-order rule in `AGENTS.md`: Typst paper, durable memory state, active backlog, generated context, references, owner intent.  litkg should encode that as ranking metadata, not leave it implicit.

A useful authority ordering would be:

```text
tier 0: code + tests + current configs
tier 1: PROJECT_STATE, DECISIONS, OPEN_QUESTIONS, GOTCHAS
tier 2: root/nested AGENTS.md and skills
tier 3: active todos/issues/refactors/resolved
tier 4: current Typst/Quarto authored docs
tier 5: generated context artifacts
tier 6: debrief history and work notes
tier 7: archived scratchpads
tier 8: external papers/web docs
```

Without this, semantic retrieval can surface stale or low-authority notes as if they were current truth.

### 4. Consolidation is not implemented as a loop

ARIA-NBV already has a memory convention: durable current truth lives in `.agents/memory/state/`, while the active maintenance backlog lives in the agent DB.  But litkg does not yet appear to run a “consolidate” pass that promotes debrief facts into canonical memory or backlog patches.

You want:

```bash
make kg-consolidate
```

to produce something like:

```text
Suggested PROJECT_STATE updates:
- Non-myopic rollout scaffold now exists in file X.
- Entity-aware RRI still blocked by TODO todo-005.
- offline_only.toml still cannot load because manifest missing.

Suggested DECISIONS updates:
- Invalid candidates are constraints, not low-RRI labels.

Suggested OPEN_QUESTIONS updates:
- Should target-conditioned RRI be scene+entity weighted or pure entity?

Suggested TODO/refactor updates:
- Add rollout cache schema before reintroducing counterfactual offline-store hooks.
```

This should be a **proposal**, not an automatic overwrite.

### 5. Runtime backend story is slightly confusing

The TOML says:

```toml
[representation]
memory_backend = "mempalace"
memory_backend_mode = "separate-memory-mining"

[backends]
graphiti = false
mempalace = false
```

while `.agents/kg/README.md` says MemPalace remains separate through `make memory-mine`.  That is reasonable, but the naming can confuse agents: is MemPalace part of litkg or separate?

I would clarify this as:

```text
litkg = structural project KG and retrieval layer
MemPalace = optional episodic memory mining backend
Graphiti = optional temporal runtime graph
Neo4j = optional traversal/runtime DB
graphify/JSONL = durable default
```

### 6. Experiments and caches are not yet first-class KG citizens

Your project’s truth increasingly depends on runs, split manifests, offline stores, configs, cache hashes, candidate-generation profiles, and metrics. The TODO list already calls for an experiment registry / final claim matrix mapping final figures/tables/claims to run IDs, config hashes, cache hashes, split manifests, metrics, and artifacts.

litkg should ingest this registry once it exists. Otherwise agents will remember papers and docs but not the evidence behind project claims.

### 7. Current KG support is explicitly marked as internal, not public

The refactor backlog already says graph and memory tooling should be scoped as KG support, not public docs; canonical truth should remain in Typst/Quarto, memory state, and code docstrings; KG artifacts should cite those sources.  That is exactly right. The improvement should not make public docs noisier. It should make agents smarter.

---

## Recommended target architecture

Make litkg the **ARIA-NBV Agent Memory Plane** with four layers.

### Layer 1: Ingestion

Inputs:

```text
code:
  aria_nbv/aria_nbv/**/*.py
  tests/**/*.py
  configs/**/*.toml
  scripts/**/*.py|sh

project truth:
  docs/typst/paper/**/*.typ
  docs/typst/thesis/**/*.typ
  docs/contents/thesis/**/*.qmd
  docs/contents/impl/**/*.qmd

agent guidance:
  AGENTS.md
  **/AGENTS.md
  .agents/skills/**/*.md
  .agents/references/**/*.md

agent memory:
  .agents/memory/state/*.md
  .agents/memory/history/**/*.md
  .agents/issues.toml
  .agents/todos.toml
  .agents/refactors.toml
  .agents/resolved.toml

generated context:
  docs/_generated/context/source_index.md
  docs/_generated/context/data_contracts.md
  docs/_generated/context/literature_index.md

research literature:
  docs/literature/sources.jsonl
  docs/references.bib
  docs/literature/tex-src/**
  docs/literature/pdf/*.pdf

experiments:
  experiment registry
  run summaries
  metric tables
  config hashes
  cache manifests
  split manifests
```

### Layer 2: Normalized KG

Node types:

```text
Project
Claim
Decision
OpenQuestion
Gotcha
Issue
Todo
Refactor
ResolvedItem
Skill
AgentGuide
DocPage
CodeFile
Symbol
Config
Dataset
CacheManifest
ExperimentRun
Metric
Paper
Method
DatasetEntity
ExternalLibrary
```

Edges:

```text
SUPPORTS
CONTRADICTS
IMPLEMENTS
DEFINED_IN
TOUCHES
DEPENDS_ON
BLOCKED_BY
RESOLVES
VALIDATED_BY
CITES
USES_METHOD
USES_DATASET
MENTIONS_SYMBOL
DERIVED_FROM
HAS_AUTHORITY_TIER
SUPERSEDED_BY
```

### Layer 3: Retrieval

Expose a single agent interface:

```bash
make kg-query KG_QUERY="What is the current target-aware RRI plan?"
make kg-brief KG_TOPIC="VIN offline store diagnostics"
make kg-route KG_TASK="debug candidate pose frame mismatch"
make kg-claim-check KG_CLAIM="VINv3 is a full NBV policy"
make kg-related KG_PATH="aria_nbv/aria_nbv/rri_metrics/oracle_rri.py"
```

Retrieval should be hybrid:

```text
lexical search
+ semantic embeddings
+ graph-neighborhood expansion
+ authority/freshness ranking
+ symbol/doc/paper linking
+ citations back to source files
```

### Layer 4: Consolidation

Expose:

```bash
make kg-consolidate
make kg-memory-diff
make kg-stale
make kg-contradictions
```

Outputs should be patch proposals, not silent edits:

```text
proposed PROJECT_STATE changes
proposed DECISIONS changes
proposed OPEN_QUESTIONS changes
proposed GOTCHAS changes
proposed todos/issues/refactors
stale docs to review
contradictory statements
missing citations / unsupported claims
```

---

## Concrete repo changes I would make

### 1. Add root Make targets for agent retrieval

Add targets like:

```make
kg-capabilities:
	@cargo run --manifest-path .agents/external/litkg-rs/Cargo.toml \
	  -p litkg-cli -- capabilities --config .configs/litkg.toml

kg-search:
	@if [ -z "$(strip $(KG_QUERY))" ]; then echo "KG_QUERY required"; exit 2; fi
	@cargo run --manifest-path .agents/external/litkg-rs/Cargo.toml \
	  -p litkg-cli -- search --config .configs/litkg.toml --query "$(KG_QUERY)"

kg-show-paper:
	@if [ -z "$(strip $(KG_PAPER))" ]; then echo "KG_PAPER required"; exit 2; fi
	@cargo run --manifest-path .agents/external/litkg-rs/Cargo.toml \
	  -p litkg-cli -- show-paper --config .configs/litkg.toml --paper "$(KG_PAPER)"
```

Then add future targets:

```make
kg-query
kg-brief
kg-route
kg-consolidate
kg-claim-check
kg-stale
kg-contradictions
kg-refresh
```

`kg-refresh` should not always do everything. Make it staged:

```bash
make kg-refresh-light     # memory/docs/config/search index only
make kg-refresh-code      # CodeGraphContext
make kg-refresh-lit       # papers + Semantic Scholar
make kg-refresh-full      # everything
```

### 2. Expand `.configs/litkg.toml`

Add agent history and skills as separate source classes with authority metadata:

```toml
[sources.agent_state]
required = true
authority = "canonical"
include = [
  ".agents/memory/state/PROJECT_STATE.md",
  ".agents/memory/state/DECISIONS.md",
  ".agents/memory/state/OPEN_QUESTIONS.md",
  ".agents/memory/state/GOTCHAS.md",
]

[sources.agent_backlog]
required = true
authority = "active_backlog"
include = [
  ".agents/issues.toml",
  ".agents/todos.toml",
  ".agents/refactors.toml",
  ".agents/resolved.toml",
]

[sources.agent_history]
required = false
authority = "episodic"
include = [
  ".agents/memory/history/**/*.md",
  ".agents/work/**/*.md",
]

[sources.agent_skills]
required = true
authority = "workflow"
include = [
  ".agents/skills/**/*.md",
  ".agents/references/**/*.md",
  "AGENTS.md",
  "**/AGENTS.md",
]

[sources.generated_context]
required = false
authority = "generated"
include = [
  "docs/_generated/context/*.md",
]
```

Add experiment/cache sources once your registry exists:

```toml
[sources.experiments]
required = false
authority = "evidence"
include = [
  ".agents/experiments/**/*.toml",
  ".agents/experiments/**/*.jsonl",
  "docs/_generated/experiments/**/*.jsonl",
  ".data/**/manifest.json",
]
exclude = [
  ".data/**/samples/**",
  ".data/**/tensor_blocks/**",
]
```

The key is to ingest **manifests and summaries**, not giant tensors or raw stores.

### 3. Add an agent-facing memory skill

Keep `semantic-scholar-litkg` for literature/KG implementation, but add a higher-level skill:

```text
.agents/skills/aria-litkg-memory/SKILL.md
```

Purpose:

```text
Use this when answering broad project questions, planning work,
debugging cross-surface issues, checking current truth, consolidating
agent debriefs, or retrieving related code/docs/papers/backlog state.
```

Workflow:

```text
1. Run kg-capabilities if unsure whether generated state exists.
2. Run kg-query / kg-search for the task.
3. Inspect cited canonical sources before acting.
4. If work changes truth, update memory/backlog and refresh light KG.
5. Leave a debrief for non-trivial work.
```

This separates “how to modify litkg-rs” from “how ARIA agents use litkg as memory.”

### 4. Add source authority and freshness fields to litkg outputs

Every retrieved result should include:

```json
{
  "source_path": ".agents/memory/state/PROJECT_STATE.md",
  "source_type": "agent_state",
  "authority": "canonical",
  "updated": "2026-04-30",
  "status": "active",
  "score_lexical": 0.71,
  "score_semantic": 0.83,
  "score_graph": 0.42,
  "score_final": 0.91,
  "why": [
    "matches query terms: RRI, non-myopic, rollout",
    "canonical memory source",
    "recently updated"
  ]
}
```

For stale sources, output:

```text
warning: source is archived / historical / superseded by PROJECT_STATE
```

### 5. Add a consolidation command

In litkg-rs, add or stage a command like:

```bash
cargo run -p litkg-cli -- consolidate-memory \
  --config .configs/litkg.toml \
  --since 2026-04-01 \
  --format markdown
```

It should scan:

```text
new debriefs
resolved items
open TODOs
changed docs/code
generated context
experiment manifests
```

and emit:

```text
canonical_updates_needed:
  PROJECT_STATE.md:
    - add: "Multi-step counterfactual rollouts now support ..."
  DECISIONS.md:
    - add: "Invalidity is represented as constraints/reasons, not low RRI."
  OPEN_QUESTIONS.md:
    - add: "How should entity RRI be weighted against scene RRI?"
  GOTCHAS.md:
    - add: "Do not rotate only candidate poses for CW90; rotate cameras/reference in lockstep."
```

### 6. Add contradiction and stale-doc detection

Useful checks:

```bash
make kg-stale
make kg-contradictions
make kg-claim-check KG_CLAIM="The system learns an end-to-end NBV policy"
```

For example, your current paper says the system is not yet a fully integrated NBV policy; it first establishes oracle supervision and trains a candidate scorer, with full policy learning remaining future work.  A claim-check command should flag “end-to-end policy” language unless it is clearly marked as future work.

This matters because the project is evolving quickly and agents can easily overclaim.

### 7. Add experiment registry ingestion

The TODO list already calls for a registry that maps claims to run IDs, configs, cache hashes, split manifests, and metrics.  litkg should become the query layer over that registry.

Example schema:

```toml
[[run]]
id = "rtjvfyyp"
kind = "vin-training"
config = ".configs/offline_only.toml"
config_hash = "..."
cache_id = "vin_offline_883"
split = "ase_mesh80_train706_val177"
checkpoint = "..."
wandb_url = "..."
status = "complete"

[[run.metric]]
name = "val_spearman"
value = 0.501
step = "best"

[[run.claim]]
supports = "VINv3 learns nontrivial ordinal RRI ranking"
strength = "moderate"
```

Then agents can ask:

```text
Which evidence supports the VINv3 baseline?
Which claims are unsupported?
Which final table rows are reproducible?
```

### 8. Make generated KG artifacts intentionally internal

Keep `.agents/kg/generated/**` internal. Do not put raw KG output into public docs navigation. This matches the refactor backlog’s guidance: KG outputs should improve retrieval without adding public-doc sprawl.

Public docs should cite curated Typst/Quarto/code outputs, not raw graph dumps.

---

## Suggested “agent protocol” using litkg

### Before any non-trivial task

Agents should run or conceptually perform:

```bash
make kg-query KG_QUERY="<task>"
```

Minimum result required:

```text
top canonical memory result
top code/symbol result
top docs result
top backlog result
top literature result, if relevant
```

### During the task

Agents should use litkg to answer:

```text
What files define this contract?
What known gotchas exist?
What active TODOs mention this area?
What previous debriefs touched this?
Which paper/source supports the claim?
```

### After the task

Agents should write:

```text
.agents/memory/history/YYYY/MM/<date>_<topic>.md
```

Then run:

```bash
make check-agent-memory
make kg-refresh-light
```

If the task changes durable truth:

```text
PROJECT_STATE.md
DECISIONS.md
OPEN_QUESTIONS.md
GOTCHAS.md
issues/todos/refactors/resolved
```

must be updated or explicitly listed as `canonical_updates_needed`.

---

## How this maps to ARIA-NBV specifically

ARIA-NBV is built around RRI, candidate generation, EFM/EVL evidence, oracle labels, VIN scoring, and emerging non-myopic rollouts. The current project state says the strongest story is geometry-first NBV research with growing planning capability, not a finished end-to-end RL system.

That means litkg should prioritize these semantic clusters:

```text
RRI / oracle / Chamfer / accuracy / completeness
candidate generation / LookAway / LookAt / collision / free-space
VIN / CORAL / ordinal labels / calibration / Spearman / top-k
EVL / EFM3D / voxel evidence / semi-dense projection
offline store / cache manifests / split manifests
multi-step rollout / cumulative RRI / beam / Gumbel / stochastic search
entity-aware RRI / OBB crop / target selection / invalid reasons
LRZ / SLURM / Enroot / DSS / job arrays
```

The literature side should connect the scientific roots:

```text
VIN-NBV → RRI objective, candidate ranking, oracle labels, CORAL
GenNBV → MDP, 5-DoF action space, RL policy, state embedding
SceneScript / ASE → synthetic egocentric scenes and Aria-compatible data
EFM3D / EVL → frozen egocentric foundation backbone
```

VIN-NBV is especially central because it defines RRI as reconstruction-quality improvement and trains a model to predict oracle RRI for candidate views.  GenNBV is central for the MDP/free-space RL contrast, with observations, actions, state embeddings, coverage reward, and PPO optimization.

---

## Practical MVP plan

### Week 1: Make litkg useful to agents without new backends

Implement:

```text
make kg-capabilities
make kg-search
make kg-show-paper
make kg-refresh-light
```

Expand `.configs/litkg.toml` to include:

```text
agent history
skills
references
generated context
experiment registry placeholder
```

Add `.agents/skills/aria-litkg-memory/SKILL.md`.

Acceptance test:

```bash
make kg-search KG_QUERY="offline_only manifest missing"
make kg-search KG_QUERY="CW90 candidate pose"
make kg-search KG_QUERY="entity-aware RRI invalid candidates"
```

Each should return canonical memory/backlog/docs/code-adjacent context.

### Week 2: Add hybrid semantic retrieval

Add or expose:

```text
semantic embeddings over Markdown/JSONL chunks
source authority ranking
freshness ranking
graph-neighborhood expansion
JSON output for agents
```

If Neo4j is not running, fallback to file-based search. A fresh checkout should still have useful retrieval.

### Week 3: Add consolidation proposal mode

Implement:

```bash
make kg-consolidate
make kg-stale
make kg-contradictions
```

Output patch-like Markdown, not automatic edits.

### Week 4: Integrate experiments and claims

Add:

```text
.agents/experiments/registry.toml
docs/_generated/experiments/*.jsonl
kg ingestion for run/config/cache/split metrics
claim-check command
```

This turns litkg into a thesis-evidence layer, not just a memory layer.

---

## Concrete issue list to add

I would add these to `.agents/todos.toml` or `.agents/refactors.toml`:

```text
todo: Add aria-litkg-memory skill
priority: high
acceptance: Broad project tasks route through kg-query before manual browsing.

todo: Add kg-query / kg-brief / kg-route Make targets
priority: high
acceptance: Agents can retrieve canonical memory, docs, code, papers, and backlog from one command.

todo: Expand litkg source coverage for agent history, skills, references, generated context, and experiment registry
priority: high
acceptance: .configs/litkg.toml separates canonical, active, workflow, episodic, generated, and evidence sources.

todo: Add source authority and freshness scoring to litkg retrieval
priority: high
acceptance: Search output ranks PROJECT_STATE/DECISIONS above old debriefs and marks stale/archive sources.

todo: Add kg-consolidate memory proposal command
priority: medium
acceptance: New debriefs produce suggested updates for PROJECT_STATE/DECISIONS/OPEN_QUESTIONS/GOTCHAS/backlog.

todo: Add stale-doc and contradiction detection
priority: medium
acceptance: Overclaims such as “finished end-to-end RL policy” are flagged against canonical memory/paper state.

todo: Add experiment/run/cache manifest ingestion
priority: medium
acceptance: Agents can query which runs/configs/cache hashes support each thesis claim.

refactor: Clarify MemPalace/Graphiti/litkg backend roles
priority: low
acceptance: AGENTS, .agents/kg/README.md, and .configs/litkg.toml consistently state that litkg is the structural KG, MemPalace is optional episodic memory, Graphiti is optional temporal runtime, and graphify/JSONL is durable default.
```

---

## Final recommendation

Do **not** turn litkg into a live mandatory database dependency. Keep the current local-first design:

```text
canonical source files
→ deterministic generated KG artifacts
→ optional Neo4j/Graphiti/MemPalace runtime
```

But do make litkg mandatory as the **agent retrieval/consolidation protocol**. In practice:

```text
Every broad agent task starts with litkg retrieval.
Every non-trivial agent task ends with debrief + possible consolidation.
Every final thesis claim is traceable through litkg to source docs/code/runs/papers.
```

That gives you the memory layer you want without polluting public docs or making agents dependent on a fragile live service.
