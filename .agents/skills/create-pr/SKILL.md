---
name: create-pr
description: Create, update, or publish pull requests for litkg-rs. Use when Codex needs to write a PR title/body for this repository, update an existing PR description, or publish a branch with a structured PR description that follows the general shape of JanDuchscherer104/prml-vslam PR #51: Summary, focused verification, work-package overview, and per-work-package resolution notes.
---

# Create PR

## Overview

Create litkg-rs pull requests that explain the branch as a small set of reviewer-friendly work packages instead of a raw diff dump. Pair this skill with the GitHub publish workflow when the branch still needs staging, commit, push, or PR creation.

## Workflow

1. Ground the branch.
   - Read `AGENTS.md` and `CODEOWNER.md`.
   - Inspect `git status -sb`, `git diff --stat`, and the key diffs against the intended base branch.
   - Separate unrelated local edits before staging or publishing.
2. Define the PR shape.
   - Group the diff into 2-7 work packages.
   - Name packages by repo boundary or contract, not by commit order.
   - Prefer scopes such as `registry merge`, `download pipeline`, `TeX parser`, `graphify adapter`, `Neo4j export`, `benchmark catalog`, `CLI plumbing`, `docs refresh`, or `agent scaffolding`.
3. Validate before writing.
   - Run the repo-wide minimum when feasible:
     - `cargo fmt --all`
     - `cargo test`
     - `make agents-db`
   - Add focused commands that match the touched surface.
   - If a command could not be run, say so explicitly in the PR body.
4. Write the title.
   - Use a concise sentence that describes the branch outcome.
   - Prefer repo language such as `registry`, `materialize`, `graphify`, `Neo4j export`, `benchmark`, or `autoresearch`.
   - If another workflow requires a `[codex]` prefix, follow that external requirement.
5. Write the body in the default section order shown below.
6. Choose the execution path.
   - If the branch still needs commit/push/PR creation, pair this skill with the GitHub publish workflow.
   - If the branch is already published, create or update the PR directly with `gh pr create` or `gh pr edit`.

## CLI Execution Path

Prefer explicit GitHub CLI commands over interactive prompts when writing a structured body.

1. Resolve the base branch.
   - If the user names a base branch, use it.
   - Otherwise check `git config branch.$(git branch --show-current).gh-merge-base`.
   - If no branch-specific merge base is configured, use the remote default branch.
2. Write the PR body to a temporary Markdown file.
   - Use `--body-file` so GitHub receives real newlines and tables intact.
   - Treat `--template` as a starting point only. If the repo has a PR template, merge any required prompts into the structured body instead of replacing the work-package format.
3. Create the PR explicitly:

```bash
gh pr create \
  --draft \
  --base "$BASE" \
  --head "$(git branch --show-current)" \
  --title "$TITLE" \
  --body-file "$BODY_FILE"
```

4. Add metadata only when requested or clearly implied:
   - `--reviewer`
   - `--label`
   - `--milestone`
   - `--project`
   - `--assignee`
5. Update an existing PR with the same structured body:

```bash
gh pr edit <number-or-branch> \
  --title "$TITLE" \
  --body-file "$BODY_FILE"
```

## GitHub CLI Notes

- `gh pr create --fill` can infer title/body from commits, but explicit `--title` and `--body` or `--body-file` override autofill. For this skill, prefer explicit structured content over raw autofill.
- `gh pr create --fill-first` and `--fill-verbose` are useful only when the commit history is already curated for reviewer-facing prose.
- `gh pr create --draft` is the correct default unless the user explicitly asks for ready review.
- `gh pr create --dry-run` is not side-effect free; it may still push git changes.
- `gh pr create --recover <token>` can recover from a failed creation attempt.
- `gh pr create --no-maintainer-edit` should be used only when the user explicitly wants to disable maintainer edits.
- Referencing issues in the body with phrases such as `Fixes #123` or `Closes #123` will auto-close those issues on merge.
- Adding a PR to a project requires `project` scope. Refresh auth with `gh auth refresh -s project` if needed.
- `--head <user>:<branch>` supports user-owned cross-repo heads. Do not assume organization-owned head syntax works in the same way.

## PR Body Shape

Use this structure by default:

```md
## Summary
<one paragraph describing the branch and why it matters>

Focused verification completed:
- `cargo fmt --all`
- `cargo test`
- `make agents-db`

## Work Packages
| WP | Scope | Primary surfaces | Status |
| --- | --- | --- | --- |
| WP1 | Registry merge cleanup | `litkg-core`, parser tests | resolved |
| WP2 | Graph sink split | `litkg-graphify`, `litkg-neo4j` | partially resolved |

## WP1 — Registry Merge Cleanup
### Scope
- <what changed>

### Key files
- `crates/litkg-core/...`

### Initial success criteria resolution
- `<criterion>`: `resolved`
- `<criterion>`: `partially resolved`
```

Keep the body skimmable:

- `Summary`: one paragraph that explains the branch at the product and architecture level.
- `Focused verification completed:`: only real commands that were actually run.
- `Work Packages`: a compact reviewer map before the detailed sections.
- `## WPn — ...`: one section per work package.

## Section Guidance

- Default subsection order inside each work package:
  - `### Scope`
  - `### Key files`
  - `### Initial success criteria resolution`
- Add `### API/path mapping` only when the branch moves public symbols, config fields, file paths, or CLI commands.
- Add `### Remaining follow-ups` only when the branch intentionally leaves work unresolved.
- Keep work-package sections outcome-oriented. Do not narrate every commit.

## Status Language

Prefer exact status labels when they fit the evidence:

- `resolved`
- `mostly resolved`
- `partially resolved`
- `not carried forward intentionally`
- `new scope, baseline resolved`

If the status needs nuance, explain it after the label instead of inventing vague wording.

## Litkg-Specific Review Lens

- If the branch touches `crates/litkg-core`, state whether the change remains repo-independent.
- If the branch touches `examples/`, clarify whether the change is config-only or reflects a broader core change.
- If the branch touches `litkg-graphify` or `litkg-neo4j`, call out whether the sink boundary stayed clean.
- If the branch touches `.agents/`, summarize the durable scaffolding impact rather than listing files mechanically.
- Mention deterministic output, rebuildability, and Apple Silicon/local-tooling implications when they materially changed.

## Publish Rules

- Prefer the repo's GitHub publish workflow for commit, push, and PR creation.
- Reuse the structured body above instead of GitHub autofill.
- Default to a draft PR unless the user explicitly asks for ready review.
- Never stage unrelated user changes silently.
- Prefer `gh pr edit` over recreating the PR when only the title/body needs revision.

## Output Requirements

- Never paste raw `git diff` into the PR body.
- Never imply validation that did not happen.
- Prefer architecture and contract language over file-by-file changelog prose.
- Keep the body reviewer-first: summary paragraph, verification list, work-package overview, then detailed sections.
- If CLI creation or edit fails, preserve the generated body file path or recovery token long enough to retry instead of retyping the body.
