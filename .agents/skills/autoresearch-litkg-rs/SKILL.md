---
name: autoresearch-litkg-rs
description: Adapt Karpathy's autoresearch loop to litkg-rs. Use when the user wants Codex to run bounded, evidence-driven research iterations in this repo with a frozen benchmark target or evaluation harness, a dedicated research branch, a `.logs/autoresearch/` experiment log, safe keep-or-discard trial branches, and repo-specific validation such as `make benchmark-validate`, `make autoresearch-target`, targeted `cargo test`, `cargo fmt --all`, and `make agents-db`.
---

# Autoresearch For litkg-rs

## Summary

This skill adapts the core
[karpathy/autoresearch](https://github.com/karpathy/autoresearch) loop to
`litkg-rs`:

- define a narrow research question
- freeze one evaluation harness
- keep the mutable surface small
- run repeated experiments
- keep only winning changes
- log the outcome of every trial

The adaptation for this repository is intentionally stricter than the upstream
loop:

- research loops are **bounded**, not infinite
- destructive git flows such as `git reset --hard` are **not allowed**
- the repo has multiple valid evaluation surfaces, so each run must declare its
  own frozen harness
- benchmark-driven runs should freeze both the benchmark inputs and the rendered
  target prompt
- winning changes must still respect repo rules such as `cargo fmt --all`,
  `cargo test`, `make benchmark-validate`, and `make agents-db`

Read [references/upstream-autoresearch.md](references/upstream-autoresearch.md)
for the upstream mechanics and the exact adaptation notes.

## When To Use

Use this skill when the user wants Codex to act like an autonomous researcher
inside this repo, for example:

- iterating on benchmark catalog, result-promotion, or target-rendering logic
- comparing alternative retrieval, grounding, or target-composition strategies
- running a small series of parser, downloader, registry, or adapter
  experiments with one fixed metric
- performing overnight or multi-trial research loops with a shared run log

Do not use this skill for:

- one-off bug fixes
- open-ended architecture brainstorming without a fixed evaluation harness
- broad refactors that touch many unrelated subsystems at once

## Setup

Before starting a loop:

1. Read the local sources of truth:
   - `AGENTS.md`
   - `README.md`
   - `docs/architecture.md`
   - `docs/benchmarks.md`
   - `.agents/AGENTS_INTERNAL_DB.md`
   - `.agents/issues.toml`
   - `.agents/todos.toml`
2. Define a **research brief** with all of the following:
   - research question
   - primary metric
   - secondary guardrails
   - mutable surface
   - immutable surface
   - max experiment count or time budget
   - benchmark catalog/results inputs if the run is benchmark-driven
3. If the run is benchmark-driven, freeze the target inputs before editing:
   - `make benchmark-validate`
   - `make autoresearch-target AUTORESEARCH_TARGET_ID=<target>`
4. Create a dedicated branch, for example
   `codex/autoresearch-2026-04-13-kg-navigation`.
5. Initialize the run log:

```bash
python3 .agents/skills/autoresearch-litkg-rs/scripts/init_run.py \
  --tag 2026-04-13-kg-navigation \
  --question "Improve benchmark-driven target rendering for KG navigation work." \
  --primary-metric "benchmark validation + target render success" \
  --evaluation-cmd "make benchmark-validate" \
  --evaluation-cmd "make autoresearch-target AUTORESEARCH_TARGET_ID=kg_navigation_improvement" \
  --target-id kg_navigation_improvement \
  --benchmark-id swe-qa-pro \
  --benchmark-id reporeason \
  --component-id retrieval_ablation \
  --component-id reasoning_diagnostics \
  --mutable crates/litkg-core/src/benchmark.rs \
  --mutable crates/litkg-cli/src/main.rs \
  --mutable examples/benchmarks/kg.toml \
  --immutable AGENTS.md \
  --immutable docs/architecture.md \
  --immutable docs/benchmarks.md
```

This creates:

- `.logs/autoresearch/<tag>/brief.md`
- `.logs/autoresearch/<tag>/results.tsv`

## Research Brief Rules

Every run must freeze these decisions up front.

### Research Question

Good:

- "Can we turn validated benchmark results into deterministic issue-ready autoresearch targets?"
- "Can we improve KG navigation target quality without widening the core paper model?"

Bad:

- "Make the repo better"
- "Try some stuff"

### Primary Metric

Choose one objective metric that decides winners. Typical choices in this repo:

- `make benchmark-validate` pass/fail
- successful `make autoresearch-target AUTORESEARCH_TARGET_ID=<target>`
- `cargo test` or a targeted package test pass/fail
- deterministic output diff or artifact-shape checks
- explicit improvement in rendered target quality under a fixed evaluation rubric

### Secondary Guardrails

Guardrails do not define the winner, but they prevent bad wins:

- no client-repo assumptions leaking into `litkg-core`
- no new undeclared dependencies
- no drift from `README.md`, `docs/architecture.md`, and `docs/benchmarks.md`
- no nondeterministic output changes without an explicit reason

### Mutable Surface

Keep the editable surface small. Prefer one module cluster such as:

- `crates/litkg-core/src/benchmark.rs` plus its tests
- `crates/litkg-cli/src/main.rs` plus the benchmark docs
- one adapter crate plus its tests
- one config and docs slice under `examples/benchmarks/`

### Immutable Surface

At minimum, freeze the evaluation harness and unrelated source-of-truth docs.
Like upstream `prepare.py`, these are not edited during the run unless the
research question explicitly targets them.

## Benchmark-Driven Brief Extensions

When the run is benchmark-driven, freeze these inputs in the brief:

- benchmark catalog path
- benchmark results bundle path
- target id, if using `render-autoresearch-target`
- selected benchmark ids
- selected component ids

Use real catalog entries when possible. The current repo examples include:

- target ids:
  - `kg_navigation_improvement`
  - `kg_release_gate`
- component ids:
  - `retrieval_ablation`
  - `reasoning_diagnostics`
  - `docs_plus_code`
  - `execution_gate`

If a winning change alters benchmark semantics, target composition, or operator
commands, update `README.md`, `docs/benchmarks.md`, and
`.agents/AGENTS_INTERNAL_DB.md` in the same winning change.

## Safe Git Flow

Do **not** use `git reset --hard`.

Use a winner branch plus ephemeral trial branches:

1. Keep the current best state on `codex/autoresearch-<tag>`.
2. For each experiment, create a short-lived branch from that winner, for
   example `codex/autoresearch-<tag>-trial-03`.
3. Make one focused change and commit it.
4. Run the frozen evaluation harness.
5. If the experiment wins:
   - switch back to the winner branch
   - cherry-pick the winning commit
   - log `keep`
6. If the experiment loses:
   - log `discard` or `crash`
   - abandon the trial branch

If the worktree is already dirty with unrelated user changes, stop and isolate
the work before starting a loop. Do not let a research loop trample unrelated
local edits.

## Experiment Loop

Repeat until the declared budget is exhausted:

1. Re-read the research brief.
2. Form exactly one experiment hypothesis.
3. Edit only the mutable surface.
4. Run the frozen evaluation command(s).
5. If the idea changes benchmark schema, rendered target semantics, or
   operator-facing contracts, update the matching docs in the same winning
   experiment.
6. Decide:
   - `keep` if the primary metric improves
   - `keep` if the primary metric is equal and the result is materially simpler
   - `discard` otherwise
7. Record the outcome in `.logs/autoresearch/<tag>/results.tsv`.

When the loop ends and the user wants a final deliverable:

- `cargo fmt --all`
- `cargo test`
- `make benchmark-validate`
- `make agents-db`

Run `make autoresearch-target AUTORESEARCH_TARGET_ID=<target>` again when the
winning change affects benchmark-driven target rendering.

## Logging Format

`results.tsv` uses these columns:

```text
experiment_id	commit	status	primary_metric	secondary_metric	description
```

Status must be one of:

- `keep`
- `discard`
- `crash`

The description should state the exact hypothesis, not a vague summary.

Good:

- `promote failing benchmark metrics into deterministic issue-ready target sections`
- `narrow component override logic to preserve target ordering guarantees`

Bad:

- `cleanup`
- `refactor`

## Suggested Evaluation Surfaces

Choose one and freeze it in the brief:

- benchmark catalog and result bundle work:
  - `make benchmark-validate`
- target-rendering work:
  - `make autoresearch-target AUTORESEARCH_TARGET_ID=kg_navigation_improvement`
  - `make autoresearch-target AUTORESEARCH_TARGET_ID=kg_release_gate`
- core pipeline work:
  - `cargo test -p litkg-core`
- CLI contract work:
  - `cargo test -p litkg-cli`
- adapter work:
  - `cargo test -p litkg-graphify`
  - `cargo test -p litkg-neo4j`
- release validation:
  - `cargo test`
  - `cargo fmt --all --check`

## Keep/Discard Heuristics

Prefer:

- equal or better metric with less code
- narrower ownership boundaries between core and adapters
- more deterministic output
- thinner CLI orchestration over stable `litkg-core` contracts
- tighter alignment with `README.md`, `docs/architecture.md`,
  `docs/benchmarks.md`, and `AGENTS.md`

Be skeptical of:

- wins that push one client-repo assumption into the generic core model
- wins that add benchmark metadata complexity without improving target quality
- wins that make output less deterministic or harder to diff
- wins that quietly change the benchmark question

## Repo-Specific Differences From Upstream

The upstream autoresearch loop edits one mutable program, runs one fixed
evaluator, and can safely discard experiments with destructive resets. This
repo is different:

- multiple crates and multiple valid evaluation surfaces are in play
- benchmark validation and target rendering are first-class research surfaces
- destructive rollback is disallowed
- repo memory lives in `.agents/AGENTS_INTERNAL_DB.md`,
  `.agents/issues.toml`, and `.agents/todos.toml`
- source-of-truth docs must stay aligned when operator contracts change

That means this skill favors short, explicit, reproducible loops over the
upstream "never stop" autonomy model.
