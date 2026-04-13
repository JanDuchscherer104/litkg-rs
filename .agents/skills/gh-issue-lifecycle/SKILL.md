---
name: gh-issue-lifecycle
description: Use when creating, syncing, triaging, or resolving GitHub issues for this repo or a similar local backlog. Covers issue discovery, de-duplication, issue-body structure, local-stack sync, and issue-closing workflow through linked PRs.
---

# Gh Issue Lifecycle

## Overview

Use this skill when a user asks to create GitHub issues, mirror a local backlog into GitHub, improve issue quality, or resolve an issue cleanly from issue body through merged change.

For this repo, the source-of-truth surfaces are:

- `.agents/issues.toml`
- `.agents/todos.toml`
- `.agents/resolved.toml`
- `.github/ISSUE_TEMPLATE/01-backlog-item.yml`
- `.github/pull_request_template.md`

If you need rationale or examples, read [references/issue-structure.md](references/issue-structure.md).

## Create Issues

1. Read the local stack and list existing GitHub issues before creating anything.
2. De-duplicate by local ID and title. For this repo, the GitHub title should normally start with `[ISSUE-xxxx]`.
3. Use the repo issue form shape even when creating issues through CLI or API. The body should usually contain:
   - `Target`
   - `Current problem`
   - `Required change`
   - `Acceptance criteria`
   - `Test expectations`
   - `Related issues` or linked local TODOs
   - optional `Change budget`
   - optional `Expected touch set`
4. Prefer concrete evidence over abstract intent. Good issues say what is wrong in the current repo state, not just what feature would be nice.
5. Keep acceptance criteria falsifiable. If you cannot tell whether the work is done, the issue is underspecified.
6. When the local stack already has linked TODOs, include them in the GitHub issue body.

## Structure Issues Well

Use the later `prml-vslam` issues as the model, not the earlier checklist-only work-package tickets.

- Strong examples explain the current broken or missing behavior first.
- They separate the requested implementation from the validation criteria.
- They state tests explicitly instead of assuming them.
- For riskier changes, they bound expected scope with change budget and touch set.

## Resolve Issues

1. Start from the GitHub issue plus the linked local stack entry.
2. Before coding, restate the acceptance criteria in your own words and verify the issue is still specific enough.
3. Implement the change and run the relevant repo validation.
4. Update the local stack:
   - mark the `TODO-*` as `done`
   - mark the `ISSUE-*` as `closed` when fully satisfied
   - add the IDs to `.agents/resolved.toml`
5. When opening or updating a PR, include `Closes #<number>` in the PR body or commit history targeting the default branch so GitHub closes the issue automatically after merge.
6. Leave a short closing comment or PR summary that maps the final change back to the issue’s acceptance criteria.

## Sync Local Backlog And GitHub

- Record GitHub issue metadata in `.agents/issues.toml` when the repo is actively synced to GitHub.
- Keep local IDs stable even if the GitHub number changes between repos.
- GitHub is the collaborative surface; `.agents/*` remains the durable local planning surface.

## Templates

- Use `.github/ISSUE_TEMPLATE/01-backlog-item.yml` for new backlog-driven issues.
- Use `.github/pull_request_template.md` so issue-closing keywords and validation are not forgotten during resolution.

## Quick Checks

- `gh issue list --repo <owner>/<repo> --state all`
- `gh issue view <number> --repo <owner>/<repo>`
- `gh issue create --repo <owner>/<repo> --title ... --body ...`
- `make agents-db`

Read [references/issue-structure.md](references/issue-structure.md) when you need the reasoning, examples, or source links behind this workflow.
