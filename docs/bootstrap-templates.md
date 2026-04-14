# Bootstrap Templates

This repo now carries strict starter templates for downstream repos that want the same `AGENTS.md`, `README.md`, and `REQUIREMENTS.md` shape used here. The template split is informed by the stronger scaffolding patterns already visible in `NBV` and `prml-vslam`: narrow sources of truth, explicit repo maps, and a clear separation between operator docs, durable requirements, and agent policy.

## Files

- `templates/bootstrap/AGENTS.md`
- `templates/bootstrap/README.md`
- `templates/bootstrap/REQUIREMENTS.md`

## How To Apply Them

1. Copy the template set into a new repo before major implementation work starts.
2. Replace every `{{...}}` token immediately. The templates are intentionally strict and should not be committed with unresolved placeholders.
3. Keep responsibilities separated:
   - `AGENTS.md` for repo policy, validation rules, and instruction-capture routing
   - `README.md` for operator onboarding and day-one commands
   - `REQUIREMENTS.md` for durable product and technical requirements, not backlog tasks

## Instruction Capture Routing

When a user asks to generalize guidance into durable scaffolding, distill it into the smallest surface that fits:

- `AGENTS.md` for repo policy and workflow
- `CODEOWNER.md` for broad human-owner intent and preferences
- `.agents/AGENTS_INTERNAL_DB.md` for stable operating facts
- `.agents/skills/*/SKILL.md` for repeatable specialized workflows
- `.agents/issues.toml` and `.agents/todos.toml` for follow-up work that is not yet implemented

Additional rules carried by the template set:

- When an issue is being worked, leave a TODO or status comment on the GitHub issue that states the planned change and whether work is happening in the local checkout or an isolated worktree.
- When guidance is wrapped in `<...>`, distill the rule instead of copying the raw chat text into scaffolding.
- If guidance reflects product direction rather than repo policy, move the distilled version to `CODEOWNER.md`.

## Why These Templates Exist

`litkg-rs` already depends on durable scaffolding for agents, issue tracking, and Codex setup. Shipping the templates makes that structure reusable instead of forcing downstream repos to reconstruct it from ad hoc chat history.
