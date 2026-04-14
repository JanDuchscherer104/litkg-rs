# {{REPO_NAME}} Agent Guidance

Describe the repo mission in one or two sentences.

## Sources Of Truth

- `AGENTS.md`: repo-wide policy and workflow
- `CODEOWNER.md`: distilled human-owner requirements and preferences
- `README.md`: operator setup and usage
- `docs/architecture.md`: architecture and subsystem boundaries
- `.agents/AGENTS_INTERNAL_DB.md`: stable operational facts
- `.agents/issues.toml`: open backlog items
- `.agents/todos.toml`: planned work units
- `.agents/resolved.toml`: completed backlog items
- `.agents/skills/`: specialized workflows

## Repo Map

- `{{CORE_PATH}}`: core shared logic
- `{{CLI_PATH}}`: operator entrypoints and orchestration
- `{{DOCS_PATH}}`: durable docs and design notes
- `.agents/`: agent scaffolding and backlog

## Repo-Wide Rules

- `AGENTS.md` is the full repo-wide policy. Nested `AGENTS.md` files should add scope-specific deltas only.
- Keep shared logic repo-independent unless a client-specific adapter is explicitly required.
- Prefer deterministic outputs and stable generated artifacts.
- Preserve subsystem boundaries; do not let optional adapters leak assumptions into the core model.
- Prefer `README.md` and package-level `REQUIREMENTS.md` files for operator workflow and implementation notes rather than restating that material in nested `AGENTS.md` files.
- Before making a commit, run `{{FORMAT_CMD}}`.
- Before making a commit, run `{{TEST_CMD}}`.
- Before making a commit, run `{{EXTRA_VALIDATION_CMD}}`.

## Instruction Capture Mode

- When the user explicitly asks to generalize guidance, distill it into the smallest durable surface that fits.
- Route repo policy and workflow to `AGENTS.md`.
- Route broad human-owner intent to `CODEOWNER.md`.
- Route stable operating facts to `.agents/AGENTS_INTERNAL_DB.md`.
- Route repeatable specialized workflows to `.agents/skills/*/SKILL.md`.
- Route future work to `.agents/issues.toml` and `.agents/todos.toml`.
- When working a GitHub issue, leave a TODO or status comment that states the planned change and whether execution is happening in the local checkout or an isolated worktree.
- Treat text inside `<...>` as candidate scaffold material and distill it instead of copying it verbatim.

## Validation

- `{{FORMAT_CMD}}`
- `{{TEST_CMD}}`
- `{{EXTRA_VALIDATION_CMD}}`
