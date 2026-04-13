# Issue Structure

This skill is based on two inputs:

1. Prior issue history from `JanDuchscherer104/prml-vslam`
2. Official GitHub guidance on issue forms, issue fields, and issue-closing workflow

## What The `prml-vslam` History Shows

The issue history has two clear eras.

Early issues such as `#27` to `#41` are short work-package tickets. They were useful for rough planning, but they are underspecified for autonomous execution because they mostly contain titles and checklists.

Later issues such as:

- `#44` Add typed dataset serving config to dataset-backed pipeline requests
- `#45` Extend SequenceManifest to preserve important ADVIO modalities and static transforms
- `#46` Support all official ADVIO pose providers and frame-serving modes in the replay adapter
- `#52` Include ADVIO fixpoints.csv in ground-truth modality extraction

use a much stronger pattern:

- `Target`
- `Current problem`
- `Related umbrella issues`
- `Required change`
- `Acceptance criteria`
- `Test expectations`
- optional `Change budget`
- optional `Expected touch set`

That later pattern is the preferred model for this repo because it gives enough structure for both humans and agents to execute and verify work.

## Official GitHub Guidance

- GitHub issue forms let repositories collect structured inputs at issue-creation time instead of relying on free-form text. Source: [Syntax for issue forms](https://docs.github.com/en/communities/using-templates-to-encourage-useful-issues-and-pull-requests/syntax-for-issue-forms)
- GitHub repositories can configure issue templates and disable blank issues to steer contributors into the intended structure. Source: [Configuring issue templates for your repository](https://docs.github.com/en/communities/using-templates-to-encourage-useful-issues-and-pull-requests/configuring-issue-templates-for-your-repository)
- GitHub issue fields support structured metadata such as priority or target date and are searchable and API-accessible. Source: [Adding and managing issue fields](https://docs.github.com/en/issues/tracking-your-work-with-issues/using-issues/adding-and-managing-issue-fields)
- GitHub can close issues automatically when a merged PR or commit to the default branch uses closing keywords such as `Closes #123`. Source: [Linking a pull request to an issue](https://docs.github.com/en/issues/tracking-your-work-with-issues/using-issues/linking-a-pull-request-to-an-issue)

## Recommended Default Shape

For this repo, a strong issue body usually includes:

1. `Target`
2. `Current problem`
3. `Required change`
4. `Acceptance criteria`
5. `Test expectations`
6. `Related issues` and linked local TODOs
7. Optional `Change budget`
8. Optional `Expected touch set`
9. Optional `Sources`

## Resolution Pattern

When resolving an issue:

1. Re-check the issue body against the actual repo state.
2. Implement the fix or feature.
3. Verify the explicit test expectations.
4. Link the PR with `Closes #<number>`.
5. Sync the local `.agents` stack to reflect closure.
