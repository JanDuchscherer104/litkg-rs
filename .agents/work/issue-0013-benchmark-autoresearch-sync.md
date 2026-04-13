# ISSUE-0013 Draft

## Title

Promote benchmark results into selectable autoresearch targets and GitHub issue sync

## Summary

Extend the benchmark-result layer so validated result bundles can feed overnight autoresearch targets directly and sync issue-ready target proposals into GitHub once the repo has a real remote.

## Motivation

- The benchmark catalog and target renderer now exist locally, but the result bundles are still mostly validation-oriented rather than execution-oriented.
- Overnight or autonomous research loops need a deterministic way to turn benchmark outcomes into concrete next-target prompts.
- Once `litkg-rs` has a real GitHub remote, those target proposals should be convertible into issue-ready payloads without rewriting the same structure by hand.

## Proposed Scope

1. Define a benchmark-result promotion policy from raw run results to candidate autoresearch targets.
2. Add score thresholds, benchmark filters, and component-selection policies for target generation.
3. Add a first-class issue-rendering format so the same target can be emitted as:
   - operator markdown
   - machine-readable JSON
   - GitHub issue body
4. Add an optional GitHub sync layer that only activates when the repo has a configured remote and authenticated `gh`.

## Acceptance Criteria

- Benchmark result bundles can be transformed into one or more deterministic autoresearch targets.
- The target-generation path supports benchmark subset selection and component concatenation without ad hoc prompt editing.
- The rendered target can be emitted in a GitHub-issue-ready form.
- GitHub issue creation is gated behind explicit repo configuration and does not fire when the repo has no remote.
