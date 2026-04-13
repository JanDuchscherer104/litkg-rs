# External Autoresearch Comparison

This note records which public autoresearch repos influenced the current
`litkg-rs` skill and which patterns were intentionally not copied.

## Comparison Table

| Repo | Strong pattern | Adopted in `litkg-rs` | Intentionally not adopted |
|---|---|---|---|
| [karpathy/autoresearch](https://github.com/karpathy/autoresearch) | tiny mutable surface, frozen evaluator, baseline-first loop, `results.tsv` | winner/trial discipline, explicit baseline, frozen verify harness, append-only result log | infinite loop, destructive reset flow |
| [uditgoenka/autoresearch](https://github.com/uditgoenka/autoresearch) | generic verify-versus-guard split and setup discipline | separate verify and guard commands in the brief and logs | generic multi-domain command tree; this repo skill stays repo-local and single-purpose |
| [leo-lilinxiao/codex-autoresearch](https://github.com/leo-lilinxiao/codex-autoresearch) | resumable state, pivot after repeated failures, structured run setup | `state.json`, explicit stop conditions, pivot after three consecutive non-keeps | detached runtime controller, hooks, background/foreground orchestration |
| [davebcn87/pi-autoresearch](https://github.com/davebcn87/pi-autoresearch) | session artifacts survive restarts, noisy-metric awareness, finalization mindset | resumable run artifacts and explicit confirmation for noisy or rubric-based wins | UI widgets, dashboards, browser export, repo-agnostic session commands |
| [trevin-creator/autoresearch-mlx](https://github.com/trevin-creator/autoresearch-mlx) | hardware-local baselines matter, especially on Apple Silicon | capture platform metadata in the brief/state and treat benchmark wins as host-local unless proven otherwise | MLX-specific training guidance; not relevant to `litkg-rs` |

## Resulting Design Rules

The current `litkg-rs` autoresearch skill should keep these boundaries:

- stay bounded and repo-specific
- keep benchmark and operator contract changes explicit
- separate winner metric from guardrails
- preserve resumable state inside `.logs/autoresearch/<tag>/`
- prefer small helper scripts over manual edits for run artifacts

## Why The Skill Stays Narrow

Some public autoresearch repos try to become full control planes with background
daemons, dashboards, generic command families, or UI hooks. That is useful in a
general-purpose autoresearch product, but it would bloat this repo-local skill.

`litkg-rs` only needs the parts that improve deterministic overnight research in
this repo:

- a frozen brief
- a safe git flow
- a reliable baseline
- append-only results
- resumable state
- explicit pivot and stop rules

Anything broader should live in a separate generic autoresearch skill, not in
the repo-specific `litkg-rs` operator workflow.
