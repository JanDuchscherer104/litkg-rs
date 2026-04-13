# Benchmark Catalog

`litkg-rs` owns the benchmark metadata and validation surfaces used to evaluate the local KG stack and to compose benchmark-driven auto research targets.

## Files

- benchmark catalog: `examples/benchmarks/kg.toml`
- sample results bundle: `examples/benchmarks/sample-results.toml`
- validation entrypoint: `cargo run -p litkg-cli -- validate-benchmarks --catalog ... --results ...`
- target rendering entrypoint: `cargo run -p litkg-cli -- render-autoresearch-target --catalog ... --results ... --target-id ...`
- target render formats: `markdown`, `issue` (alias `github-issue`), and `json`

## Included Benchmarks

- `SWE-Bench Pro`: long-horizon issue resolution
- `SWE-QA-Pro`: repository-grounded QA
- `CodeRepoQA`: large-scale multilingual QA
- `StackRepoQA`: retrieval-sensitive repository QA
- `RepoReason`: white-box repository reasoning diagnostics
- `RACE-bench`: feature-addition reasoning with intermediate labels
- `SWD-Bench`: docs-grounded repository understanding
- `CCBench`: contamination-resistant small private-style codebases
- `Terminal-Bench`: CLI-native long-horizon tasks

## Why These Benchmarks Are Grouped Here

The set is intentionally mixed:

- top-line execution benchmarks for release-gate decisions
- repository QA benchmarks for retrieval and grounding quality
- white-box diagnostics for reasoning failure analysis
- docs-plus-code benchmarks for context-surface evaluation
- contamination-resistant checks for realism outside public OSS leaderboards

## Validation

The validation command checks:

- non-empty benchmark metadata
- unique benchmark, metric, component, and target identifiers
- source links for every benchmark
- that benchmark results reference known benchmarks and known metric ids
- that autoresearch targets only reference existing components and benchmarks

Run it with:

```bash
make benchmark-validate
```

## Auto Research Target Composition

Autoresearch targets are assembled from reusable components stored in the same benchmark catalog. Each target chooses a default benchmark subset and a default ordered component list, but the CLI also allows overriding those selections.

Example:

```bash
make autoresearch-target
cargo run -p litkg-cli -- render-autoresearch-target \
  --catalog examples/benchmarks/kg.toml \
  --results examples/benchmarks/sample-results.toml \
  --target-id kg_navigation_improvement \
  --format issue
```

Or, with explicit benchmark/component overrides:

```bash
cargo run -p litkg-cli -- render-autoresearch-target \
  --catalog examples/benchmarks/kg.toml \
  --results examples/benchmarks/sample-results.toml \
  --target-id kg_navigation_improvement \
  --component-id retrieval_ablation \
  --component-id reasoning_diagnostics \
  --benchmark-id swe-qa-pro \
  --benchmark-id reporeason
```

This makes the benchmark-driven research prompt fragments explicitly selectable and concatenable, which is the intended contract for later overnight or autonomous research loops.

When the target needs to be promoted into an issue-ready operator artifact, render it with:

```bash
cargo run -p litkg-cli -- render-autoresearch-target \
  --catalog examples/benchmarks/kg.toml \
  --results examples/benchmarks/sample-results.toml \
  --target-id kg_navigation_improvement \
  --format github-issue
```

When a result bundle is present, the rendered target also emits a deterministic
promotion assessment:

- `validation_only` runs are deferred and treated as schema or smoke checks
- recognized execution failure statuses such as `observed_failure` or `failed`
  are marked as promotable inputs for the next research target
- known control-plane statuses such as `timeout`, `pending`, or `infra_error`
  are deferred
- unknown statuses must be classified explicitly before benchmark validation
  passes

This keeps schema-validation fixtures from masquerading as evidence of benchmark
deficits during overnight loops.

The renderer supports three stable output surfaces:

- `markdown` for operator reading
- `issue` for issue-ready bodies
- `json` for machine consumption

The JSON payload includes:

- promotion counts for promotable versus deferred runs
- sanitized run summary text plus structured score evidence
- `result_summaries` as the canonical normalized result view for downstream
  automation

## GitHub Sync

When the repo has a live GitHub remote, the rendered GitHub-issue surface can be
synced directly into GitHub:

```bash
make autoresearch-issue
cargo run -p litkg-cli -- sync-autoresearch-target-issue \
  --catalog examples/benchmarks/kg.toml \
  --results examples/benchmarks/sample-results.toml \
  --target-id kg_navigation_improvement \
  --dry-run
```

The sync command:

- renders the same deterministic issue body used by `--format github-issue`
- infers `owner/repo` from `origin` by default, or accepts `--repo [HOST/]owner/repo`
- can add labels with repeated `--label`
- uses `--dry-run` to preview the title and body without opening a GitHub issue

For a live issue create, drop `--dry-run` and optionally pass labels:

```bash
cargo run -p litkg-cli -- sync-autoresearch-target-issue \
  --catalog examples/benchmarks/kg.toml \
  --results examples/benchmarks/sample-results.toml \
  --target-id kg_navigation_improvement \
  --label autoresearch
```
