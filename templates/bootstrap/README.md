# {{REPO_NAME}}

{{REPO_DESCRIPTION}}

## Human Signal

- `CODEOWNER.md` stores distilled human-owner requirements and preferences that should persist beyond a single chat session.
- `AGENTS.md` stores repo policy and workflow for agents.

## Workspace

- `{{CORE_PATH}}`: core shared logic
- `{{CLI_PATH}}`: operator entrypoints
- `{{DOCS_PATH}}`: durable docs and design notes
- `.agents/`: agent scaffolding and backlog

## Documentation Map

- `README.md`: onboarding, setup, repo workflow, and high-level project framing
- `{{CORE_PATH}}/README.md`: implementation guidance and extension notes close to the code
- `{{CORE_PATH}}/REQUIREMENTS.md`: durable package contracts and current-state boundaries
- `AGENTS.md` and nested `AGENTS.md`: repo policy and agent-facing workflow guidance

## Quick Start

```bash
{{SETUP_CMD}}
{{RUN_CMD_1}}
{{RUN_CMD_2}}
```

## Validation

```bash
{{FORMAT_CMD}}
{{TEST_CMD}}
{{EXTRA_VALIDATION_CMD}}
```

## Agent Surfaces

- Repo skills live under `.agents/skills/`.
- Local backlog state lives under `.agents/`.
- If the repo has optional Codex or MCP setup, document it in `{{DOCS_PATH}}/codex-setup.md` and keep it optional relative to the main product path.
