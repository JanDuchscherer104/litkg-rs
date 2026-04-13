# Benchmark Catalog

`litkg-rs` owns the benchmark metadata and validation surfaces used to evaluate the local KG stack and to compose benchmark-driven auto research targets.

## Files

- benchmark catalog: `examples/benchmarks/kg.toml`
- integration matrix: `examples/benchmarks/integrations.toml`
- sample results bundle: `examples/benchmarks/sample-results.toml`
- validation entrypoint: `cargo run -p litkg-cli -- validate-benchmarks --catalog ... --results ...`
- support inspection entrypoint: `cargo run -p litkg-cli -- benchmark-support --catalog ... --integrations ...`
- execution entrypoint: `cargo run -p litkg-cli -- run-benchmarks --catalog ... --integrations ... --plan ... --output ...`
- target rendering entrypoint: `cargo run -p litkg-cli -- render-autoresearch-target --catalog ... --results ... --target-id ...`
- target render formats: `markdown`, `issue` (alias `github-issue`), and `json`
- result-promotion entrypoint: `cargo run -p litkg-cli -- promote-benchmark-results --catalog ... --results ...`

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

## Integration Readiness

The integration matrix makes the current execution state explicit per benchmark:

- `official_harness` or `official_pipeline`: a public runnable surface exists upstream, but `litkg-rs` still expects a local command wrapper to emit normalized JSON
- `dataset_only`: the dataset is public, but no upstream evaluator is currently packaged for direct invocation
- `paper_only` or `benchmark_site`: the catalog currently points to a paper or site only, so local execution must be supplied through a custom command adapter

Inspect the current machine state with:

```bash
cargo run -p litkg-cli -- benchmark-support \
  --catalog examples/benchmarks/kg.toml \
  --integrations examples/benchmarks/integrations.toml
```

## Running Benchmarks

`litkg-rs` runs benchmarks through external command adapters. Each configured command receives:

- `LITKG_BENCHMARK_ID`
- `LITKG_BENCHMARK_RUN_ID`
- `LITKG_BENCHMARK_OUTPUT_PATH`
- `LITKG_BENCHMARK_ARTIFACT_DIR`

The command must write normalized JSON to `LITKG_BENCHMARK_OUTPUT_PATH` with this shape:

```json
{
  "status": "error",
  "summary": "Short run summary",
  "scores": [
    {
      "metric_id": "correctness",
      "value": 0.42,
      "unit": "ratio"
    }
  ],
  "diagnostics": ["optional diagnostic text"],
  "artifacts": [
    {
      "label": "raw-log",
      "kind": "log",
      "location": "artifacts/run.log"
    }
  ]
}
```

With a run plan in place, execute the configured adapters with:

```bash
cargo run -p litkg-cli -- run-benchmarks \
  --catalog examples/benchmarks/kg.toml \
  --integrations examples/benchmarks/integrations.toml \
  --plan /abs/path/to/run-plan.toml \
  --output /abs/path/to/results.toml
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

## Result Promotion

Validated benchmark bundles can also be promoted into deterministic autoresearch drafts. Promotion works as a pure transformation over the catalog plus a results bundle:

- benchmark filters narrow which runs are eligible
- status filters let operators focus on states such as `error` or `failed`
- metric thresholds use inline rules such as `correctness<=0.7` or `pass_at_1<0.5`
- component selection policies choose whether promotion keeps the template components only, appends benchmark-matched components, or uses matched components only

Example:

```bash
cargo run -p litkg-cli -- promote-benchmark-results \
  --catalog examples/benchmarks/kg.toml \
  --results examples/benchmarks/sample-results.toml \
  --target-id kg_navigation_improvement \
  --status error \
  --metric-threshold correctness<=0.7 \
  --component-selection template-and-matched \
  --format github-issue
```

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
