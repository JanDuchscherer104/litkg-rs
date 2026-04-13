---
name: autoresearch-litkg-rs
description: Adapt bounded autoresearch loops to litkg-rs. Use when the user wants Codex to run evidence-driven benchmark or code experiments in this repo with a frozen verify harness, explicit baseline, separate guard commands, safe winner/trial branches, and resumable `.logs/autoresearch/` run artifacts.
---

# Autoresearch For litkg-rs

## Summary

This skill adapts the core
[karpathy/autoresearch](https://github.com/karpathy/autoresearch) loop to
`litkg-rs`:

- define a narrow research question
- freeze one verify harness
- freeze separate guard commands
- record a baseline before editing
- keep the mutable surface small
- run repeated experiments
- keep only winning changes
- log the outcome of every trial in resumable run artifacts

The adaptation for this repository is intentionally stricter than the upstream
and narrower than generic autoresearch skills:

- research loops are **bounded**, not infinite
- destructive git flows such as `git reset --hard` are **not allowed**
- the repo has multiple valid evaluation surfaces, so each run must declare its
  own frozen verify harness
- benchmark-driven runs should freeze both the benchmark inputs and the rendered
  target prompt
- keep/discard decisions must separate the winning metric from safety guards
- repeated non-wins should trigger a pivot instead of blind retry loops
- candidate winning experiments must clear the repo-local review gate before
  promotion
- winning changes must still respect repo rules such as `cargo fmt --all`,
  `cargo test`, `make benchmark-validate`, and `make agents-db`

Read [references/upstream-autoresearch.md](references/upstream-autoresearch.md)
for the upstream mechanics and exact litkg-rs adaptation notes.

Read [references/repo-comparison.md](references/repo-comparison.md) only when
you need the rationale behind the extra patterns adopted from other public
autoresearch repos.

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
   - metric direction such as `lower`, `higher`, `pass/fail`, or `rubric`
   - verify command(s)
   - guard command(s)
   - secondary guardrails
   - mutable surface
   - immutable surface
   - max experiment count or time budget
   - explicit stop condition
   - noise policy if the metric is not deterministic
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
  --direction pass/fail \
  --verify-cmd "make benchmark-validate" \
  --verify-cmd "make autoresearch-target AUTORESEARCH_TARGET_ID=kg_navigation_improvement" \
  --guard-cmd "cargo fmt --all --check" \
  --guard-cmd "cargo test -p litkg-core" \
  --guard-cmd "make agents-db" \
  --target-format markdown \
  --max-experiments 8 \
  --time-budget "4h" \
  --stop-when "budget exhausted or two pivots without a new keep" \
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
- `.logs/autoresearch/<tag>/state.json`

`--evaluation-cmd` remains supported as a compatibility alias for
`--verify-cmd`.

6. Run the baseline on the untouched winner branch and record it before any
   experimental edit:

```bash
python3 .agents/skills/autoresearch-litkg-rs/scripts/record_result.py \
  --tag 2026-04-13-kg-navigation \
  --experiment-id 00-baseline \
  --commit "$(git rev-parse --short HEAD)" \
  --status baseline \
  --primary-metric "pass" \
  --guardrail-status "pass" \
  --description "baseline on frozen harness" \
  --set-best
```

If the baseline fails, stop. Fix the harness, inputs, or branch isolation first.

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

Direction matters. Call out whether better means `lower`, `higher`,
`pass/fail`, or an explicit rubric grade. Do not start iterating until this is
clear.

### Verify vs Guard Commands

Keep these separate:

- **Verify commands** decide whether an experiment improved the run goal.
- **Guard commands** prevent bad wins. A candidate that wins the primary metric
  but fails a guard must be discarded or repaired before it can be kept.

Typical verify commands:

- `make benchmark-validate`
- `make autoresearch-target AUTORESEARCH_TARGET_ID=<target>`
- `cargo test -p litkg-core`
- `cargo test -p litkg-cli`

Typical guard commands:

- `cargo fmt --all --check`
- targeted crate tests adjacent to the mutable surface
- `make agents-db`

Repo policy still applies before any trial commit. Even when the frozen verify
surface is narrower, run the repo-wide minimum validation before committing a
trial snapshot:

- `cargo fmt --all`
- `cargo test`
- `make agents-db`

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

At minimum, freeze the verify harness and unrelated source-of-truth docs.
Like upstream `prepare.py`, these are not edited during the run unless the
research question explicitly targets them.

## Benchmark-Driven Brief Extensions

When the run is benchmark-driven, freeze these inputs in the brief:

- benchmark catalog path
- benchmark results bundle path
- target id, if using `render-autoresearch-target`
- target format, if not using the default markdown render
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
3. Make one focused change.
4. Run the frozen verify commands. Only run the guard commands if the candidate
   clears the verify step.
5. Before creating any trial commit, run `cargo fmt --all`, `cargo test`, and
   `make agents-db`.
6. If the experiment produced a validated trial snapshot, commit it on the
   trial branch. If it crashed before that point, record the commit as `-`.
7. If the experiment wins:
   - switch back to the winner branch
   - cherry-pick the winning commit
   - log `keep` and update the best-known state
8. If the experiment loses:
   - log `discard` or `crash`
   - abandon the trial branch

If the worktree is already dirty with unrelated user changes, stop and isolate
the work before starting a loop. Do not let a research loop trample unrelated
local edits.

Do not stage or commit `.logs/autoresearch/` artifacts.

## Review Gate

Before promoting a trial into the winner branch:

1. Review the exact evaluated diff with `.agents/skills/code-review-litkg-rs/SKILL.md`.
2. Do not promote changes with unresolved `P0` or `P1` findings.
3. If only `P2` or `P3` findings remain, either fix them in the same trial or
   record the debt explicitly in `.logs/autoresearch/<tag>/results.tsv`, the
   linked issue, or `.agents/todos.toml`.
4. When the run is backed by a GitHub PR, inspect unresolved review comments or
   threads before closing the loop.

## Experiment Loop

Repeat until the declared budget is exhausted:

1. Re-read `brief.md`, `state.json`, and the last few results before choosing
   the next hypothesis.
2. Form exactly one experiment hypothesis.
3. Edit only the mutable surface.
4. Run the frozen verify command(s).
5. If the verify step passes, run the frozen guard command(s).
6. Before creating a trial commit, run `cargo fmt --all`, `cargo test`, and
   `make agents-db` to satisfy repo policy.
7. If the trial produced a validated snapshot, commit it on the trial branch.
   If it crashed before that point, log the commit as `-`.
8. If the idea changes benchmark schema, rendered target semantics, or
   operator-facing contracts, update the matching docs in the same winning
   experiment.
9. For any candidate `keep`, run the repo-local review gate before promotion.
10. Decide:
   - `keep` if the primary metric improves and review has no unresolved `P0` or
     `P1` findings
   - `keep` if the primary metric is equal, the result is materially simpler,
     and review has no unresolved `P0` or `P1` findings
   - `discard` otherwise
11. Record the outcome with
    `python3 .agents/skills/autoresearch-litkg-rs/scripts/record_result.py ...`.
12. If the metric is noisy or rubric-based, confirm a candidate win with a
    second run or an explicit rubric note before keeping it.
13. If three experiments in a row end in `discard` or `crash`, pivot to a
    materially different idea instead of brute-force retries.
14. If two pivots still do not produce a new `keep`, stop and summarize the
    blocker.

When the loop ends and the user wants a final deliverable:

- `cargo fmt --all`
- `cargo test`
- `make benchmark-validate`
- `make agents-db`

When the winning change affects benchmark-driven target rendering, rerun the
exact frozen render command from the brief or `state.json`. If the run used
non-default benchmark inputs or a non-default format, pass
`BENCHMARK_CATALOG`, `BENCHMARK_RESULTS`, `AUTORESEARCH_TARGET_ID`, and
`AUTORESEARCH_TARGET_FORMAT` explicitly instead of relying on Makefile
defaults.

## Logging Format

`results.tsv` uses these columns:

```text
experiment_id	commit	status	primary_metric	guardrail_status	description
```

Status must be one of:

- `baseline`
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

Use `guardrail_status` to record whether the guard commands passed, failed, or
were skipped, for example:

- `pass`
- `fail:cargo test -p litkg-core`
- `skipped-after-verify-fail`

Do not record a guarded failure as `keep`. A kept result must have
`guardrail_status` starting with `pass`.

Record results with the helper script instead of hand-editing the TSV whenever
possible:

```bash
python3 .agents/skills/autoresearch-litkg-rs/scripts/record_result.py \
  --tag 2026-04-13-kg-navigation \
  --experiment-id 03-ordering-guard \
  --commit "$(git rev-parse --short HEAD)" \
  --status keep \
  --primary-metric "pass" \
  --guardrail-status "pass" \
  --description "preserve ordered target sections while narrowing component overrides" \
  --set-best
```

`record_result.py` rejects duplicate `experiment_id` values so reruns do not
silently skew pivot counts or the best-known state.

## Suggested Evaluation Surfaces

Choose one primary verify surface and then add only the guards the run really
needs:

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
- clearer verify/guard separation
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

## Resume Protocol

If the run is interrupted:

1. Re-open `.logs/autoresearch/<tag>/brief.md`.
2. Re-open `.logs/autoresearch/<tag>/state.json`.
3. Read the tail of `.logs/autoresearch/<tag>/results.tsv`.
4. Confirm the current winner branch still matches the recorded run tag and the
   worktree does not contain unrelated edits.
5. Resume from the best-known branch state, not from an abandoned trial branch.
6. If `state.json` says `needs_pivot: true`, begin with a new direction rather
   than another small variation of the last failed idea.

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
- run state should survive interruption through `.logs/autoresearch/<tag>/state.json`

That means this skill favors short, explicit, reproducible loops over the
upstream "never stop" autonomy model.
