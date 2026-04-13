use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct BenchmarkCatalog {
    #[serde(default)]
    pub benchmarks: Vec<BenchmarkSpec>,
    #[serde(default)]
    pub autoresearch_components: Vec<AutoResearchComponent>,
    #[serde(default)]
    pub autoresearch_targets: Vec<AutoResearchTargetTemplate>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BenchmarkSpec {
    pub id: String,
    pub name: String,
    pub best_use: String,
    pub task_scale: String,
    pub summary: String,
    #[serde(default)]
    pub dataset_notes: Vec<String>,
    #[serde(default)]
    pub metrics: Vec<BenchmarkMetric>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub sources: Vec<BenchmarkSource>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BenchmarkMetric {
    pub id: String,
    pub label: String,
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BenchmarkSource {
    pub label: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AutoResearchComponent {
    pub id: String,
    pub title: String,
    pub prompt_fragment: String,
    #[serde(default)]
    pub benchmark_ids: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AutoResearchTargetTemplate {
    pub id: String,
    pub title: String,
    pub summary: String,
    #[serde(default)]
    pub benchmark_ids: Vec<String>,
    #[serde(default)]
    pub component_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct BenchmarkResults {
    #[serde(default)]
    pub runs: Vec<BenchmarkRun>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BenchmarkRun {
    pub benchmark_id: String,
    pub run_id: String,
    pub status: String,
    pub summary: String,
    #[serde(default)]
    pub scores: Vec<BenchmarkScore>,
    #[serde(default)]
    pub diagnostics: Vec<String>,
    #[serde(default)]
    pub artifacts: Vec<BenchmarkArtifact>,
    #[serde(default)]
    pub execution: Option<BenchmarkExecutionRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BenchmarkScore {
    pub metric_id: String,
    pub value: f64,
    pub unit: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BenchmarkArtifact {
    pub label: String,
    pub kind: String,
    pub location: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BenchmarkExecutionRecord {
    pub runner_kind: String,
    pub command: String,
    pub workdir: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ValidationSummary {
    pub benchmark_count: usize,
    pub metric_count: usize,
    pub component_count: usize,
    pub target_count: usize,
    pub run_count: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct RenderedAutoResearchTarget {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub benchmarks: Vec<BenchmarkSpec>,
    pub components: Vec<AutoResearchComponent>,
    pub runs: Vec<BenchmarkRun>,
    pub has_results_bundle: bool,
    pub promotion_summary: PromotionSummary,
    pub result_summaries: Vec<PromotedRunSummary>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
struct RenderedAutoResearchJsonTarget {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub benchmarks: Vec<BenchmarkSpec>,
    pub components: Vec<AutoResearchComponent>,
    pub has_results_bundle: bool,
    pub promotion_summary: PromotionSummary,
    pub result_summaries: Vec<PromotedRunSummary>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoResearchRenderFormat {
    Markdown,
    Issue,
    Json,
    GitHubIssue,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PromotedRunSummary {
    pub benchmark_id: String,
    pub run_id: String,
    pub status: String,
    pub disposition: AutoResearchResultDisposition,
    pub reason: String,
    pub summary: String,
    pub score_evidence: Vec<PromotedRunScoreEvidence>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutoResearchResultDisposition {
    Promote,
    Defer,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PromotionSummary {
    pub promotable_count: usize,
    pub deferred_count: usize,
    pub has_promotable_results: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PromotedRunScoreEvidence {
    pub metric_id: String,
    pub value: f64,
    pub unit: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BenchmarkRunStatusClass {
    ValidationOnly,
    Success,
    PromotableFailure,
    DeferredControl,
}

pub fn load_benchmark_catalog(path: impl AsRef<Path>) -> Result<BenchmarkCatalog> {
    let path = path.as_ref();
    let raw = fs::read_to_string(path)
        .with_context(|| format!("Failed to read benchmark catalog {}", path.display()))?;
    toml::from_str(&raw)
        .with_context(|| format!("Failed to parse benchmark catalog {}", path.display()))
}

pub fn load_benchmark_results(path: impl AsRef<Path>) -> Result<BenchmarkResults> {
    let path = path.as_ref();
    let raw = fs::read_to_string(path)
        .with_context(|| format!("Failed to read benchmark results {}", path.display()))?;
    toml::from_str(&raw)
        .with_context(|| format!("Failed to parse benchmark results {}", path.display()))
}

pub fn write_benchmark_results(path: impl AsRef<Path>, results: &BenchmarkResults) -> Result<()> {
    let path = path.as_ref();
    let raw = toml::to_string_pretty(results)
        .with_context(|| format!("Failed to serialize benchmark results {}", path.display()))?;
    fs::write(path, raw)
        .with_context(|| format!("Failed to write benchmark results {}", path.display()))
}

pub fn validate_benchmark_catalog(catalog: &BenchmarkCatalog) -> Result<ValidationSummary> {
    if catalog.benchmarks.is_empty() {
        bail!("Benchmark catalog must define at least one benchmark");
    }

    let benchmark_ids = ensure_unique_named_items(
        catalog
            .benchmarks
            .iter()
            .map(|benchmark| benchmark.id.as_str()),
        "benchmark",
    )?;

    let component_ids = ensure_unique_named_items(
        catalog
            .autoresearch_components
            .iter()
            .map(|component| component.id.as_str()),
        "autoresearch component",
    )?;

    ensure_unique_named_items(
        catalog
            .autoresearch_targets
            .iter()
            .map(|target| target.id.as_str()),
        "autoresearch target",
    )?;

    let mut metric_count = 0usize;
    for benchmark in &catalog.benchmarks {
        if benchmark.name.trim().is_empty() {
            bail!("Benchmark `{}` must have a non-empty name", benchmark.id);
        }
        if benchmark.summary.trim().is_empty() {
            bail!("Benchmark `{}` must have a non-empty summary", benchmark.id);
        }
        if benchmark.best_use.trim().is_empty() {
            bail!(
                "Benchmark `{}` must have a non-empty best_use",
                benchmark.id
            );
        }
        if benchmark.task_scale.trim().is_empty() {
            bail!(
                "Benchmark `{}` must have a non-empty task_scale",
                benchmark.id
            );
        }
        if benchmark.metrics.is_empty() {
            bail!(
                "Benchmark `{}` must define at least one metric",
                benchmark.id
            );
        }
        if benchmark.sources.is_empty() {
            bail!(
                "Benchmark `{}` must define at least one source",
                benchmark.id
            );
        }
        ensure_unique_named_items(
            benchmark.metrics.iter().map(|metric| metric.id.as_str()),
            &format!("metric for benchmark `{}`", benchmark.id),
        )?;
        metric_count += benchmark.metrics.len();
        for source in &benchmark.sources {
            if source.label.trim().is_empty() || source.url.trim().is_empty() {
                bail!(
                    "Benchmark `{}` contains a source with an empty label or url",
                    benchmark.id
                );
            }
        }
    }

    for component in &catalog.autoresearch_components {
        if component.title.trim().is_empty() {
            bail!(
                "Autoresearch component `{}` must have a non-empty title",
                component.id
            );
        }
        if component.prompt_fragment.trim().is_empty() {
            bail!(
                "Autoresearch component `{}` must have a non-empty prompt_fragment",
                component.id
            );
        }
        for benchmark_id in &component.benchmark_ids {
            if !benchmark_ids.contains(benchmark_id) {
                bail!(
                    "Autoresearch component `{}` references unknown benchmark `{}`",
                    component.id,
                    benchmark_id
                );
            }
        }
    }

    for target in &catalog.autoresearch_targets {
        if target.title.trim().is_empty() {
            bail!(
                "Autoresearch target `{}` must have a non-empty title",
                target.id
            );
        }
        if target.summary.trim().is_empty() {
            bail!(
                "Autoresearch target `{}` must have a non-empty summary",
                target.id
            );
        }
        if target.component_ids.is_empty() {
            bail!(
                "Autoresearch target `{}` must reference at least one component",
                target.id
            );
        }
        for component_id in &target.component_ids {
            if !component_ids.contains(component_id) {
                bail!(
                    "Autoresearch target `{}` references unknown component `{}`",
                    target.id,
                    component_id
                );
            }
        }
        for benchmark_id in &target.benchmark_ids {
            if !benchmark_ids.contains(benchmark_id) {
                bail!(
                    "Autoresearch target `{}` references unknown benchmark `{}`",
                    target.id,
                    benchmark_id
                );
            }
        }
    }

    Ok(ValidationSummary {
        benchmark_count: catalog.benchmarks.len(),
        metric_count,
        component_count: catalog.autoresearch_components.len(),
        target_count: catalog.autoresearch_targets.len(),
        run_count: 0,
    })
}

pub fn validate_benchmark_results(
    catalog: &BenchmarkCatalog,
    results: &BenchmarkResults,
) -> Result<ValidationSummary> {
    let mut summary = validate_benchmark_catalog(catalog)?;
    let benchmark_by_id = benchmark_map(catalog);
    let mut seen_run_ids = BTreeSet::new();

    for run in &results.runs {
        if run.run_id.trim().is_empty() {
            bail!(
                "Benchmark run for benchmark `{}` must have a non-empty run_id",
                run.benchmark_id
            );
        }
        if !seen_run_ids.insert(run.run_id.clone()) {
            bail!("Duplicate benchmark run id `{}`", run.run_id);
        }
        let benchmark = benchmark_by_id.get(&run.benchmark_id).ok_or_else(|| {
            anyhow::anyhow!(
                "Benchmark run `{}` references unknown benchmark `{}`",
                run.run_id,
                run.benchmark_id
            )
        })?;
        if run.status.trim().is_empty() {
            bail!(
                "Benchmark run `{}` must have a non-empty status",
                run.run_id
            );
        }
        classify_run_status(&run.status).with_context(|| {
            format!(
                "Benchmark run `{}` uses an unsupported status `{}`",
                run.run_id, run.status
            )
        })?;
        if run.summary.trim().is_empty() {
            bail!(
                "Benchmark run `{}` must have a non-empty summary",
                run.run_id
            );
        }
        for diagnostic in &run.diagnostics {
            if diagnostic.trim().is_empty() {
                bail!(
                    "Benchmark run `{}` contains an empty diagnostic entry",
                    run.run_id
                );
            }
        }
        for artifact in &run.artifacts {
            if artifact.label.trim().is_empty()
                || artifact.kind.trim().is_empty()
                || artifact.location.trim().is_empty()
            {
                bail!(
                    "Benchmark run `{}` contains an artifact with an empty label, kind, or location",
                    run.run_id
                );
            }
        }
        if let Some(execution) = &run.execution {
            if execution.runner_kind.trim().is_empty()
                || execution.command.trim().is_empty()
                || execution.workdir.trim().is_empty()
            {
                bail!(
                    "Benchmark run `{}` contains incomplete execution metadata",
                    run.run_id
                );
            }
        }
        let metric_ids: BTreeSet<&str> = benchmark
            .metrics
            .iter()
            .map(|metric| metric.id.as_str())
            .collect();
        let mut seen_metrics = BTreeSet::new();
        for score in &run.scores {
            if !metric_ids.contains(score.metric_id.as_str()) {
                bail!(
                    "Benchmark run `{}` references unknown metric `{}` for benchmark `{}`",
                    run.run_id,
                    score.metric_id,
                    run.benchmark_id
                );
            }
            if !seen_metrics.insert(score.metric_id.clone()) {
                bail!(
                    "Benchmark run `{}` repeats metric `{}`",
                    run.run_id,
                    score.metric_id
                );
            }
            if score.unit.trim().is_empty() {
                bail!(
                    "Benchmark run `{}` metric `{}` must have a non-empty unit",
                    run.run_id,
                    score.metric_id
                );
            }
            if !score.value.is_finite() {
                bail!(
                    "Benchmark run `{}` metric `{}` must have a finite numeric value",
                    run.run_id,
                    score.metric_id
                );
            }
        }
    }

    summary.run_count = results.runs.len();
    Ok(summary)
}

pub fn render_autoresearch_target(
    catalog: &BenchmarkCatalog,
    results: Option<&BenchmarkResults>,
    target_id: &str,
    component_ids: &[String],
    benchmark_ids: &[String],
    format: AutoResearchRenderFormat,
) -> Result<String> {
    validate_benchmark_catalog(catalog)?;
    if let Some(results) = results {
        validate_benchmark_results(catalog, results)?;
    }

    let target = catalog
        .autoresearch_targets
        .iter()
        .find(|target| target.id == target_id)
        .with_context(|| format!("Unknown autoresearch target `{target_id}`"))?;

    let selected_benchmark_ids = if benchmark_ids.is_empty() {
        target.benchmark_ids.clone()
    } else {
        benchmark_ids.to_vec()
    };
    let selected_component_ids = if component_ids.is_empty() {
        target.component_ids.clone()
    } else {
        component_ids.to_vec()
    };

    let benchmark_by_id = benchmark_map(catalog);
    let component_by_id = component_map(catalog);
    let selected_benchmarks = selected_benchmark_ids
        .iter()
        .map(|benchmark_id| {
            benchmark_by_id
                .get(benchmark_id)
                .cloned()
                .with_context(|| format!("Unknown benchmark `{benchmark_id}`"))
        })
        .collect::<Result<Vec<_>>>()?;
    let selected_components = selected_component_ids
        .iter()
        .map(|component_id| {
            component_by_id
                .get(component_id)
                .cloned()
                .with_context(|| format!("Unknown autoresearch component `{component_id}`"))
        })
        .collect::<Result<Vec<_>>>()?;

    let selected_runs = if let Some(results) = results {
        results
            .runs
            .iter()
            .filter(|run| selected_benchmark_ids.contains(&run.benchmark_id))
            .cloned()
            .collect()
    } else {
        Vec::new()
    };

    let result_summaries = summarize_runs_for_promotion(results, &selected_benchmark_ids);
    let rendered = RenderedAutoResearchTarget {
        id: target.id.clone(),
        title: target.title.clone(),
        summary: target.summary.clone(),
        benchmarks: selected_benchmarks,
        components: selected_components,
        runs: selected_runs,
        has_results_bundle: results.is_some(),
        promotion_summary: build_promotion_summary(&result_summaries),
        result_summaries,
    };

    match format {
        AutoResearchRenderFormat::Markdown => Ok(render_markdown_target(&rendered)),
        AutoResearchRenderFormat::Issue => Ok(render_issue_target(&rendered)),
        AutoResearchRenderFormat::Json => {
            serde_json::to_string_pretty(&RenderedAutoResearchJsonTarget::from_rendered(&rendered))
                .context("Failed to serialize autoresearch target as JSON")
        }
        AutoResearchRenderFormat::GitHubIssue => Ok(render_github_issue_target(&rendered)),
    }
}

fn render_markdown_target(rendered: &RenderedAutoResearchTarget) -> String {
    let mut lines = vec![
        format!("# Auto Research Target: {}", rendered.title),
        String::new(),
        rendered.summary.clone(),
        String::new(),
        "## Selected Benchmarks".to_string(),
        String::new(),
    ];

    for benchmark in &rendered.benchmarks {
        lines.push(format!(
            "- `{}`: {}. Task scale: {}",
            benchmark.id, benchmark.best_use, benchmark.task_scale
        ));
    }

    if !rendered.runs.is_empty() {
        lines.push(String::new());
        lines.push("## Available Results".to_string());
        lines.push(String::new());
        for run in &rendered.runs {
            let score_summary = if run.scores.is_empty() {
                "no scores recorded".to_string()
            } else {
                format_score_summary_from_scores(&run.scores)
            };
            lines.push(format!(
                "- `{}` on `{}` [{}]: {}",
                run.run_id,
                run.benchmark_id,
                sanitize_inline_markdown(&run.status),
                score_summary
            ));
        }
    }

    if rendered.has_results_bundle {
        lines.push(String::new());
        lines.push("## Result Promotion Assessment".to_string());
        lines.push(String::new());
        lines.push(format!(
            "Promotion summary: {} promotable, {} deferred.",
            rendered.promotion_summary.promotable_count, rendered.promotion_summary.deferred_count
        ));

        if rendered.result_summaries.is_empty() {
            lines.push(String::new());
            lines.push(
                "No selected benchmark runs were found in the provided results bundle.".to_string(),
            );
        } else {
            lines.push(String::new());
            for summary in &rendered.result_summaries {
                lines.push(format!(
                    "- {} `{}` on `{}` [{}]: {}",
                    match summary.disposition {
                        AutoResearchResultDisposition::Promote => "promote",
                        AutoResearchResultDisposition::Defer => "defer",
                    },
                    summary.run_id,
                    summary.benchmark_id,
                    summary.status,
                    summary.reason
                ));
                lines.push(format!("  Summary: {}", summary.summary));
                if !summary.score_evidence.is_empty() {
                    lines.push(format!(
                        "  Evidence: {}",
                        format_score_evidence_list(&summary.score_evidence)
                    ));
                }
            }

            if rendered
                .result_summaries
                .iter()
                .all(|summary| matches!(summary.disposition, AutoResearchResultDisposition::Defer))
            {
                lines.push(String::new());
                lines.push(
                    "No promotable execution results were found. Treat the selected runs as schema or smoke checks rather than evidence of benchmark deficits."
                        .to_string(),
                );
            }
        }
    }

    lines.push(String::new());
    lines.push("## Concatenated Components".to_string());
    lines.push(String::new());
    for (index, component) in rendered.components.iter().enumerate() {
        lines.push(format!("### {}. {}", index + 1, component.title));
        lines.push(String::new());
        lines.push(component.prompt_fragment.clone());
        lines.push(String::new());
    }

    lines.join("\n")
}

fn render_github_issue_target(rendered: &RenderedAutoResearchTarget) -> String {
    render_issue_target(rendered)
}

fn render_issue_target(rendered: &RenderedAutoResearchTarget) -> String {
    let mut lines = vec![
        format!("# Autoresearch Target: {}", rendered.title),
        String::new(),
        "## Target".to_string(),
        String::new(),
        "Set this to the target repository default branch.".to_string(),
        String::new(),
        "## Current problem".to_string(),
        String::new(),
        rendered.summary.clone(),
        String::new(),
        "Selected benchmark context:".to_string(),
        String::new(),
    ];

    for benchmark in &rendered.benchmarks {
        lines.push(format!("- `{}`: {}", benchmark.id, benchmark.best_use));
    }

    let promoted = rendered
        .result_summaries
        .iter()
        .filter(|summary| matches!(summary.disposition, AutoResearchResultDisposition::Promote))
        .collect::<Vec<_>>();
    let deferred = rendered
        .result_summaries
        .iter()
        .filter(|summary| matches!(summary.disposition, AutoResearchResultDisposition::Defer))
        .collect::<Vec<_>>();

    lines.push(String::new());
    lines.push("Promoted result inputs:".to_string());
    lines.push(String::new());
    if promoted.is_empty() {
        lines.push(
            "No promotable result inputs are currently available. Use the component scaffold below as the next bounded experiment."
                .to_string(),
        );
    } else {
        for summary in promoted {
            lines.push(format!(
                "- `{}` on `{}` [{}]: {} Summary: {}{}",
                summary.run_id,
                summary.benchmark_id,
                summary.status,
                summary.reason,
                summary.summary,
                render_score_evidence_suffix(summary)
            ));
        }
    }

    if !deferred.is_empty() {
        lines.push(String::new());
        lines.push("Deferred result inputs:".to_string());
        lines.push(String::new());
        for summary in deferred {
            lines.push(format!(
                "- `{}` on `{}` [{}]: {} Summary: {}{}",
                summary.run_id,
                summary.benchmark_id,
                summary.status,
                summary.reason,
                summary.summary,
                render_score_evidence_suffix(summary)
            ));
        }
    }

    lines.push(String::new());
    lines.push("## Required change".to_string());
    lines.push(String::new());
    lines.push(
        "Run a bounded follow-up autoresearch pass that targets the promoted benchmark evidence below and updates the normalized results bundle with the outcome."
            .to_string(),
    );
    lines.push(String::new());
    lines.push("Proposed research work:".to_string());
    lines.push(String::new());
    for (index, component) in rendered.components.iter().enumerate() {
        lines.push(format!("### {}. {}", index + 1, component.title));
        lines.push(String::new());
        lines.push(component.prompt_fragment.clone());
        lines.push(String::new());
    }

    lines.push("## Acceptance criteria".to_string());
    lines.push(String::new());
    lines.push(
        "- The selected autoresearch target is grounded in the promoted benchmark evidence or explicitly records that no promotable evidence is currently available."
            .to_string(),
    );
    lines.push(
        "- Follow-up benchmark execution updates the normalized results bundle for the selected benchmark set."
            .to_string(),
    );
    lines.push(
        "- Deferred runs remain excluded from benchmark-deficit targeting unless their statuses are reclassified explicitly."
            .to_string(),
    );
    lines.push(String::new());
    lines.push("## Test expectations".to_string());
    lines.push(String::new());
    lines.push("- `cargo fmt --all`".to_string());
    lines.push("- `cargo test`".to_string());
    lines.push("- `make benchmark-validate`".to_string());
    lines.push(
        "- Run the relevant benchmark harnesses and refresh the normalized results bundle used for follow-up targeting."
            .to_string(),
    );
    lines.push(String::new());
    lines.push("## Related issues and local TODOs".to_string());
    lines.push(String::new());
    lines.push(
        "- Add any linked GitHub issues or local backlog IDs before publishing when this target extends existing work."
            .to_string(),
    );
    lines.join("\n")
}

fn ensure_unique_named_items<'a>(
    ids: impl IntoIterator<Item = &'a str>,
    label: &str,
) -> Result<BTreeSet<String>> {
    let mut seen = BTreeSet::new();
    for id in ids {
        let trimmed = id.trim();
        if trimmed.is_empty() {
            bail!("Found {} with an empty id", label);
        }
        if !seen.insert(trimmed.to_string()) {
            bail!("Duplicate {} id `{}`", label, trimmed);
        }
    }
    Ok(seen)
}

fn benchmark_map(catalog: &BenchmarkCatalog) -> BTreeMap<String, BenchmarkSpec> {
    catalog
        .benchmarks
        .iter()
        .cloned()
        .map(|benchmark| (benchmark.id.clone(), benchmark))
        .collect()
}

fn component_map(catalog: &BenchmarkCatalog) -> BTreeMap<String, AutoResearchComponent> {
    catalog
        .autoresearch_components
        .iter()
        .cloned()
        .map(|component| (component.id.clone(), component))
        .collect()
}

fn summarize_runs_for_promotion(
    results: Option<&BenchmarkResults>,
    selected_benchmark_ids: &[String],
) -> Vec<PromotedRunSummary> {
    let Some(results) = results else {
        return Vec::new();
    };

    selected_benchmark_ids
        .iter()
        .flat_map(|benchmark_id| {
            results
                .runs
                .iter()
                .filter(move |run| run.benchmark_id == *benchmark_id)
                .map(summarize_run_for_promotion)
        })
        .collect()
}

fn summarize_run_for_promotion(run: &BenchmarkRun) -> PromotedRunSummary {
    let normalized_status = run.status.trim().to_ascii_lowercase();
    let status_class = classify_run_status(&run.status)
        .expect("benchmark results should be validated before rendering");
    let sanitized_summary = sanitize_inline_markdown(&run.summary);
    let (disposition, reason) = match status_class {
        BenchmarkRunStatusClass::ValidationOnly => (
            AutoResearchResultDisposition::Defer,
            "validation-only run; keep it as a schema check, not as evidence for the next target"
                .to_string(),
        ),
        BenchmarkRunStatusClass::Success => (
            AutoResearchResultDisposition::Defer,
            "successful execution run; keep it for release tracking rather than the next target"
                .to_string(),
        ),
        BenchmarkRunStatusClass::PromotableFailure => (
            AutoResearchResultDisposition::Promote,
            "recognized execution failure run; eligible to shape the next deterministic research target"
                .to_string(),
        ),
        BenchmarkRunStatusClass::DeferredControl => (
            AutoResearchResultDisposition::Defer,
            "control-plane run status; keep it out of benchmark-deficit targeting"
                .to_string(),
        ),
    };

    PromotedRunSummary {
        benchmark_id: run.benchmark_id.clone(),
        run_id: run.run_id.clone(),
        status: normalized_status,
        disposition,
        reason,
        summary: sanitized_summary,
        score_evidence: collect_run_evidence(run),
    }
}

fn classify_run_status(status: &str) -> Result<BenchmarkRunStatusClass> {
    let normalized_status = status.trim().to_ascii_lowercase();
    let class = match normalized_status.as_str() {
        "validation_only" => BenchmarkRunStatusClass::ValidationOnly,
        "success" | "successful" | "passed" | "pass" | "completed" | "ok" => {
            BenchmarkRunStatusClass::Success
        }
        "observed_failure" | "failure" | "failed" | "regression" | "degraded"
        | "partial_failure" | "error" => BenchmarkRunStatusClass::PromotableFailure,
        "timeout"
        | "timed_out"
        | "pending"
        | "queued"
        | "canceled"
        | "cancelled"
        | "infra_error"
        | "infra_failure"
        | "unsupported"
        | "skipped"
        | "unconfigured"
        | "unavailable"
        | "runner_failed"
        | "normalization_error" => BenchmarkRunStatusClass::DeferredControl,
        _ => bail!(
            "Classify benchmark run status `{}` before using it in autoresearch promotion",
            status
        ),
    };
    Ok(class)
}

fn build_promotion_summary(result_summaries: &[PromotedRunSummary]) -> PromotionSummary {
    let promotable_count = result_summaries
        .iter()
        .filter(|summary| matches!(summary.disposition, AutoResearchResultDisposition::Promote))
        .count();
    let deferred_count = result_summaries.len().saturating_sub(promotable_count);

    PromotionSummary {
        promotable_count,
        deferred_count,
        has_promotable_results: promotable_count > 0,
    }
}

fn collect_run_evidence(run: &BenchmarkRun) -> Vec<PromotedRunScoreEvidence> {
    run.scores
        .iter()
        .map(|score| PromotedRunScoreEvidence {
            metric_id: score.metric_id.clone(),
            value: score.value,
            unit: score.unit.clone(),
        })
        .collect()
}

fn sanitize_inline_markdown(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn format_score_evidence_list(evidence: &[PromotedRunScoreEvidence]) -> String {
    evidence
        .iter()
        .map(|score| {
            format!(
                "score {}={} {}",
                sanitize_inline_markdown(&score.metric_id),
                score.value,
                sanitize_inline_markdown(&score.unit)
            )
        })
        .collect::<Vec<_>>()
        .join("; ")
}

fn format_score_summary_from_scores(scores: &[BenchmarkScore]) -> String {
    scores
        .iter()
        .map(|score| {
            format!(
                "{}={} {}",
                sanitize_inline_markdown(&score.metric_id),
                score.value,
                sanitize_inline_markdown(&score.unit)
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn render_score_evidence_suffix(summary: &PromotedRunSummary) -> String {
    if summary.score_evidence.is_empty() {
        String::new()
    } else {
        format!(
            " Evidence: {}",
            format_score_evidence_list(&summary.score_evidence)
        )
    }
}

impl RenderedAutoResearchJsonTarget {
    fn from_rendered(rendered: &RenderedAutoResearchTarget) -> Self {
        Self {
            id: rendered.id.clone(),
            title: rendered.title.clone(),
            summary: rendered.summary.clone(),
            benchmarks: rendered.benchmarks.clone(),
            components: rendered.components.clone(),
            has_results_bundle: rendered.has_results_bundle,
            promotion_summary: rendered.promotion_summary.clone(),
            result_summaries: rendered.result_summaries.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn sample_catalog() -> BenchmarkCatalog {
        BenchmarkCatalog {
            benchmarks: vec![BenchmarkSpec {
                id: "swe-qa-pro".into(),
                name: "SWE-QA-Pro".into(),
                best_use: "Repository-grounded QA".into(),
                task_scale: "260 questions from 26 repositories".into(),
                summary: "Repository QA benchmark".into(),
                dataset_notes: vec!["Python only".into()],
                metrics: vec![BenchmarkMetric {
                    id: "overall".into(),
                    label: "Overall".into(),
                    notes: "Judge average".into(),
                }],
                tags: vec!["qa".into()],
                sources: vec![BenchmarkSource {
                    label: "paper".into(),
                    url: "https://arxiv.org/abs/2603.16124".into(),
                }],
            }],
            autoresearch_components: vec![AutoResearchComponent {
                id: "ablation".into(),
                title: "Ablation".into(),
                prompt_fragment: "Compare graph-only with hybrid retrieval.".into(),
                benchmark_ids: vec!["swe-qa-pro".into()],
                tags: vec!["retrieval".into()],
            }],
            autoresearch_targets: vec![AutoResearchTargetTemplate {
                id: "kg-navigation".into(),
                title: "KG navigation".into(),
                summary: "Improve repository navigation quality.".into(),
                benchmark_ids: vec!["swe-qa-pro".into()],
                component_ids: vec!["ablation".into()],
            }],
        }
    }

    fn two_benchmark_catalog() -> BenchmarkCatalog {
        BenchmarkCatalog {
            benchmarks: vec![
                BenchmarkSpec {
                    id: "swe-qa-pro".into(),
                    name: "SWE-QA-Pro".into(),
                    best_use: "Repository-grounded QA".into(),
                    task_scale: "260 questions from 26 repositories".into(),
                    summary: "Repository QA benchmark".into(),
                    dataset_notes: vec![],
                    metrics: vec![BenchmarkMetric {
                        id: "overall".into(),
                        label: "Overall".into(),
                        notes: "Judge average".into(),
                    }],
                    tags: vec![],
                    sources: vec![BenchmarkSource {
                        label: "paper".into(),
                        url: "https://example.com/swe-qa-pro".into(),
                    }],
                },
                BenchmarkSpec {
                    id: "reporeason".into(),
                    name: "RepoReason".into(),
                    best_use: "Reasoning diagnostics".into(),
                    task_scale: "2492 tasks".into(),
                    summary: "Repo reasoning benchmark".into(),
                    dataset_notes: vec![],
                    metrics: vec![BenchmarkMetric {
                        id: "pass_at_1".into(),
                        label: "Pass@1".into(),
                        notes: "Task accuracy".into(),
                    }],
                    tags: vec![],
                    sources: vec![BenchmarkSource {
                        label: "paper".into(),
                        url: "https://example.com/reporeason".into(),
                    }],
                },
            ],
            autoresearch_components: vec![AutoResearchComponent {
                id: "ablation".into(),
                title: "Ablation".into(),
                prompt_fragment: "Compare graph-only with hybrid retrieval.".into(),
                benchmark_ids: vec!["swe-qa-pro".into(), "reporeason".into()],
                tags: vec!["retrieval".into()],
            }],
            autoresearch_targets: vec![AutoResearchTargetTemplate {
                id: "kg-navigation".into(),
                title: "KG navigation".into(),
                summary: "Improve repository navigation quality.".into(),
                benchmark_ids: vec!["swe-qa-pro".into(), "reporeason".into()],
                component_ids: vec!["ablation".into()],
            }],
        }
    }

    #[test]
    fn validates_catalog_and_results() {
        let catalog = sample_catalog();
        let summary = validate_benchmark_catalog(&catalog).unwrap();
        assert_eq!(summary.benchmark_count, 1);
        assert_eq!(summary.metric_count, 1);

        let results = BenchmarkResults {
            runs: vec![BenchmarkRun {
                benchmark_id: "swe-qa-pro".into(),
                run_id: "baseline".into(),
                status: "validation_only".into(),
                summary: "Catalog validated".into(),
                scores: vec![BenchmarkScore {
                    metric_id: "overall".into(),
                    value: 1.0,
                    unit: "pass".into(),
                }],
                diagnostics: Vec::new(),
                artifacts: Vec::new(),
                execution: None,
            }],
        };
        let result_summary = validate_benchmark_results(&catalog, &results).unwrap();
        assert_eq!(result_summary.run_count, 1);
    }

    #[test]
    fn renders_target_markdown() {
        let catalog = sample_catalog();
        let results = BenchmarkResults {
            runs: vec![BenchmarkRun {
                benchmark_id: "swe-qa-pro".into(),
                run_id: "baseline".into(),
                status: "validation_only".into(),
                summary: "Catalog validated".into(),
                scores: vec![BenchmarkScore {
                    metric_id: "overall".into(),
                    value: 1.0,
                    unit: "pass".into(),
                }],
                diagnostics: Vec::new(),
                artifacts: Vec::new(),
                execution: None,
            }],
        };
        let rendered = render_autoresearch_target(
            &catalog,
            Some(&results),
            "kg-navigation",
            &[],
            &[],
            AutoResearchRenderFormat::Markdown,
        )
        .unwrap();
        assert!(rendered.contains("Auto Research Target: KG navigation"));
        assert!(rendered.contains("Compare graph-only with hybrid retrieval."));
        assert!(rendered.contains("baseline"));
        assert!(rendered.contains("Result Promotion Assessment"));
        assert!(rendered.contains("Promotion summary: 0 promotable, 1 deferred."));
        assert!(rendered.contains("No promotable execution results were found."));
    }

    #[test]
    fn promotes_non_validation_runs() {
        let catalog = sample_catalog();
        let results = BenchmarkResults {
            runs: vec![BenchmarkRun {
                benchmark_id: "swe-qa-pro".into(),
                run_id: "retrieval-regression".into(),
                status: "observed_failure".into(),
                summary: "Hybrid retrieval missed the expected file cluster.".into(),
                scores: vec![BenchmarkScore {
                    metric_id: "overall".into(),
                    value: 0.42,
                    unit: "ratio".into(),
                }],
                diagnostics: Vec::new(),
                artifacts: Vec::new(),
                execution: None,
            }],
        };

        let rendered = render_autoresearch_target(
            &catalog,
            Some(&results),
            "kg-navigation",
            &[],
            &[],
            AutoResearchRenderFormat::Markdown,
        )
        .unwrap();

        assert!(rendered.contains("promote `retrieval-regression`"));
        assert!(rendered.contains("recognized execution failure run"));
        assert!(rendered.contains("Summary: Hybrid retrieval missed the expected file cluster."));
        assert!(rendered.contains("Evidence: score overall=0.42 ratio"));
    }

    #[test]
    fn defers_successful_execution_runs() {
        let catalog = sample_catalog();
        let results = BenchmarkResults {
            runs: vec![BenchmarkRun {
                benchmark_id: "swe-qa-pro".into(),
                run_id: "retrieval-win".into(),
                status: "success".into(),
                summary: "Hybrid retrieval improved grounded answer quality.".into(),
                scores: vec![BenchmarkScore {
                    metric_id: "overall".into(),
                    value: 0.91,
                    unit: "ratio".into(),
                }],
                diagnostics: Vec::new(),
                artifacts: Vec::new(),
                execution: None,
            }],
        };

        let rendered = render_autoresearch_target(
            &catalog,
            Some(&results),
            "kg-navigation",
            &[],
            &[],
            AutoResearchRenderFormat::Markdown,
        )
        .unwrap();

        assert!(rendered.contains("defer `retrieval-win`"));
        assert!(rendered.contains("successful execution run"));
    }

    #[test]
    fn defers_unknown_non_success_statuses() {
        let catalog = sample_catalog();
        let results = BenchmarkResults {
            runs: vec![BenchmarkRun {
                benchmark_id: "swe-qa-pro".into(),
                run_id: "infra-timeout".into(),
                status: "timeout".into(),
                summary: "Harness timed out before benchmark execution completed.".into(),
                scores: vec![],
                diagnostics: Vec::new(),
                artifacts: Vec::new(),
                execution: None,
            }],
        };

        let rendered = render_autoresearch_target(
            &catalog,
            Some(&results),
            "kg-navigation",
            &[],
            &[],
            AutoResearchRenderFormat::Markdown,
        )
        .unwrap();

        assert!(rendered.contains("defer `infra-timeout`"));
        assert!(rendered.contains("control-plane run status"));
        assert!(rendered.contains("Promotion summary: 0 promotable, 1 deferred."));
    }

    #[test]
    fn sanitizes_multiline_summaries_for_markdown_outputs() {
        let catalog = sample_catalog();
        let results = BenchmarkResults {
            runs: vec![BenchmarkRun {
                benchmark_id: "swe-qa-pro".into(),
                run_id: "retrieval-regression".into(),
                status: "observed_failure".into(),
                summary: "Hybrid retrieval missed the expected file cluster.\n\n# follow-up".into(),
                scores: vec![BenchmarkScore {
                    metric_id: "overall".into(),
                    value: 0.42,
                    unit: "ratio".into(),
                }],
                diagnostics: Vec::new(),
                artifacts: Vec::new(),
                execution: None,
            }],
        };

        let rendered = render_autoresearch_target(
            &catalog,
            Some(&results),
            "kg-navigation",
            &[],
            &[],
            AutoResearchRenderFormat::Issue,
        )
        .unwrap();

        assert!(rendered
            .contains("Summary: Hybrid retrieval missed the expected file cluster. # follow-up"));
        assert!(!rendered.contains("\n# follow-up"));
    }

    #[test]
    fn sanitizes_multiline_score_units_for_issue_output() {
        let catalog = sample_catalog();
        let results = BenchmarkResults {
            runs: vec![BenchmarkRun {
                benchmark_id: "swe-qa-pro".into(),
                run_id: "retrieval-regression".into(),
                status: "observed_failure".into(),
                summary: "Hybrid retrieval missed the expected file cluster.".into(),
                scores: vec![BenchmarkScore {
                    metric_id: "overall".into(),
                    value: 0.42,
                    unit: "ratio\n# injected".into(),
                }],
                diagnostics: Vec::new(),
                artifacts: Vec::new(),
                execution: None,
            }],
        };

        let rendered = render_autoresearch_target(
            &catalog,
            Some(&results),
            "kg-navigation",
            &[],
            &[],
            AutoResearchRenderFormat::Issue,
        )
        .unwrap();

        assert!(rendered.contains("Evidence: score overall=0.42 ratio # injected"));
        assert!(!rendered.contains("\n# injected"));
    }

    #[test]
    fn renders_markdown_assessment_when_results_bundle_has_no_selected_runs() {
        let catalog = sample_catalog();
        let results = BenchmarkResults { runs: vec![] };

        let rendered = render_autoresearch_target(
            &catalog,
            Some(&results),
            "kg-navigation",
            &[],
            &[],
            AutoResearchRenderFormat::Markdown,
        )
        .unwrap();

        assert!(rendered.contains("Result Promotion Assessment"));
        assert!(rendered.contains("Promotion summary: 0 promotable, 0 deferred."));
        assert!(rendered
            .contains("No selected benchmark runs were found in the provided results bundle."));
    }

    #[test]
    fn rejects_unknown_statuses_during_validation() {
        let catalog = sample_catalog();
        let results = BenchmarkResults {
            runs: vec![BenchmarkRun {
                benchmark_id: "swe-qa-pro".into(),
                run_id: "new-failure".into(),
                status: "new_failure_mode".into(),
                summary: "Harness introduced a new failure status.".into(),
                scores: vec![BenchmarkScore {
                    metric_id: "overall".into(),
                    value: 0.1,
                    unit: "ratio".into(),
                }],
                diagnostics: Vec::new(),
                artifacts: Vec::new(),
                execution: None,
            }],
        };

        let error = validate_benchmark_results(&catalog, &results).unwrap_err();
        assert!(error
            .to_string()
            .contains("unsupported status `new_failure_mode`"));
    }

    #[test]
    fn rejects_empty_run_ids_during_validation() {
        let catalog = sample_catalog();
        let results = BenchmarkResults {
            runs: vec![BenchmarkRun {
                benchmark_id: "swe-qa-pro".into(),
                run_id: "".into(),
                status: "validation_only".into(),
                summary: "Catalog validated".into(),
                scores: vec![BenchmarkScore {
                    metric_id: "overall".into(),
                    value: 1.0,
                    unit: "pass".into(),
                }],
                diagnostics: Vec::new(),
                artifacts: Vec::new(),
                execution: None,
            }],
        };

        let error = validate_benchmark_results(&catalog, &results).unwrap_err();
        assert!(error.to_string().contains("non-empty run_id"));
    }

    #[test]
    fn rejects_non_finite_score_values_during_validation() {
        let catalog = sample_catalog();
        let results = BenchmarkResults {
            runs: vec![BenchmarkRun {
                benchmark_id: "swe-qa-pro".into(),
                run_id: "nan-run".into(),
                status: "observed_failure".into(),
                summary: "Runner failed with NaN score.".into(),
                scores: vec![BenchmarkScore {
                    metric_id: "overall".into(),
                    value: f64::NAN,
                    unit: "ratio".into(),
                }],
                diagnostics: Vec::new(),
                artifacts: Vec::new(),
                execution: None,
            }],
        };

        let error = validate_benchmark_results(&catalog, &results).unwrap_err();
        assert!(error.to_string().contains("finite numeric value"));
    }

    #[test]
    fn promotes_error_statuses() {
        let catalog = sample_catalog();
        let results = BenchmarkResults {
            runs: vec![BenchmarkRun {
                benchmark_id: "swe-qa-pro".into(),
                run_id: "runner-error".into(),
                status: "error".into(),
                summary: "Benchmark execution failed inside the task runner.".into(),
                scores: vec![BenchmarkScore {
                    metric_id: "overall".into(),
                    value: 0.0,
                    unit: "ratio".into(),
                }],
                diagnostics: Vec::new(),
                artifacts: Vec::new(),
                execution: None,
            }],
        };

        let rendered = render_autoresearch_target(
            &catalog,
            Some(&results),
            "kg-navigation",
            &[],
            &[],
            AutoResearchRenderFormat::Issue,
        )
        .unwrap();

        assert!(rendered.contains("runner-error"));
        assert!(rendered.contains("recognized execution failure run"));
    }

    #[test]
    fn preserves_selected_benchmark_order_in_promotion_assessment() {
        let catalog = two_benchmark_catalog();
        let results = BenchmarkResults {
            runs: vec![
                BenchmarkRun {
                    benchmark_id: "reporeason".into(),
                    run_id: "reasoning-regression".into(),
                    status: "observed_failure".into(),
                    summary: "RepoReason found a reasoning regression.".into(),
                    scores: vec![BenchmarkScore {
                        metric_id: "pass_at_1".into(),
                        value: 0.2,
                        unit: "ratio".into(),
                    }],
                    diagnostics: Vec::new(),
                    artifacts: Vec::new(),
                    execution: None,
                },
                BenchmarkRun {
                    benchmark_id: "swe-qa-pro".into(),
                    run_id: "qa-regression".into(),
                    status: "observed_failure".into(),
                    summary: "SWE-QA-Pro found a grounding regression.".into(),
                    scores: vec![BenchmarkScore {
                        metric_id: "overall".into(),
                        value: 0.3,
                        unit: "ratio".into(),
                    }],
                    diagnostics: Vec::new(),
                    artifacts: Vec::new(),
                    execution: None,
                },
            ],
        };

        let rendered = render_autoresearch_target(
            &catalog,
            Some(&results),
            "kg-navigation",
            &[],
            &[],
            AutoResearchRenderFormat::Markdown,
        )
        .unwrap();

        let assessment = rendered
            .split("## Result Promotion Assessment")
            .nth(1)
            .unwrap();
        let swe_index = assessment.find("qa-regression").unwrap();
        let repo_index = assessment.find("reasoning-regression").unwrap();
        assert!(swe_index < repo_index);
    }

    #[test]
    fn renders_issue_format_with_promoted_inputs() {
        let catalog = sample_catalog();
        let results = BenchmarkResults {
            runs: vec![BenchmarkRun {
                benchmark_id: "swe-qa-pro".into(),
                run_id: "retrieval-regression".into(),
                status: "observed_failure".into(),
                summary: "Hybrid retrieval missed the expected file cluster.".into(),
                scores: vec![BenchmarkScore {
                    metric_id: "overall".into(),
                    value: 0.42,
                    unit: "ratio".into(),
                }],
                diagnostics: Vec::new(),
                artifacts: Vec::new(),
                execution: None,
            }],
        };

        let rendered = render_autoresearch_target(
            &catalog,
            Some(&results),
            "kg-navigation",
            &[],
            &[],
            AutoResearchRenderFormat::Issue,
        )
        .unwrap();

        assert!(rendered.contains("## Current problem"));
        assert!(rendered.contains("Promoted result inputs:"));
        assert!(rendered.contains("retrieval-regression"));
        assert!(rendered.contains("Evidence: score overall=0.42 ratio"));
        assert!(rendered.contains("## Required change"));
        assert!(rendered.contains("## Acceptance criteria"));
    }

    #[test]
    fn renders_issue_format_with_deferred_only_results() {
        let catalog = sample_catalog();
        let results = BenchmarkResults {
            runs: vec![BenchmarkRun {
                benchmark_id: "swe-qa-pro".into(),
                run_id: "baseline".into(),
                status: "validation_only".into(),
                summary: "Catalog validated".into(),
                scores: vec![BenchmarkScore {
                    metric_id: "overall".into(),
                    value: 1.0,
                    unit: "pass".into(),
                }],
                diagnostics: Vec::new(),
                artifacts: Vec::new(),
                execution: None,
            }],
        };
        let rendered = render_autoresearch_target(
            &catalog,
            Some(&results),
            "kg-navigation",
            &[],
            &[],
            AutoResearchRenderFormat::GitHubIssue,
        )
        .unwrap();
        assert!(rendered.contains("No promotable result inputs are currently available."));
        assert!(rendered.contains("Deferred result inputs:"));
        assert!(rendered.contains("## Test expectations"));
    }

    #[test]
    fn renders_target_as_github_issue_body() {
        let catalog = sample_catalog();
        let results = BenchmarkResults {
            runs: vec![BenchmarkRun {
                benchmark_id: "swe-qa-pro".into(),
                run_id: "baseline".into(),
                status: "validation_only".into(),
                summary: "Catalog validated".into(),
                scores: vec![BenchmarkScore {
                    metric_id: "overall".into(),
                    value: 1.0,
                    unit: "pass".into(),
                }],
                diagnostics: Vec::new(),
                artifacts: Vec::new(),
                execution: None,
            }],
        };
        let rendered = render_autoresearch_target(
            &catalog,
            Some(&results),
            "kg-navigation",
            &[],
            &[],
            AutoResearchRenderFormat::GitHubIssue,
        )
        .unwrap();

        assert!(rendered.contains("# Autoresearch Target: KG navigation"));
        assert!(rendered.contains("## Related issues and local TODOs"));
        assert!(!rendered.contains("ISSUE-0013"));
    }

    #[test]
    fn renders_json_with_promotion_summary() {
        let catalog = sample_catalog();
        let results = BenchmarkResults {
            runs: vec![BenchmarkRun {
                benchmark_id: "swe-qa-pro".into(),
                run_id: "retrieval-regression".into(),
                status: "observed_failure".into(),
                summary: "Hybrid retrieval missed the expected file cluster.".into(),
                scores: vec![BenchmarkScore {
                    metric_id: "overall".into(),
                    value: 0.42,
                    unit: "ratio".into(),
                }],
                diagnostics: Vec::new(),
                artifacts: Vec::new(),
                execution: None,
            }],
        };

        let rendered = render_autoresearch_target(
            &catalog,
            Some(&results),
            "kg-navigation",
            &[],
            &[],
            AutoResearchRenderFormat::Json,
        )
        .unwrap();

        assert!(rendered.contains("\"promotable_count\": 1"));
        assert!(rendered.contains("\"has_promotable_results\": true"));
        assert!(rendered.contains("\"score_evidence\""));
        assert!(!rendered.contains("\"runs\""));
    }

    #[test]
    fn normalizes_status_strings_in_promoted_results() {
        let catalog = sample_catalog();
        let results = BenchmarkResults {
            runs: vec![BenchmarkRun {
                benchmark_id: "swe-qa-pro".into(),
                run_id: "runner-failure".into(),
                status: " FAILED ".into(),
                summary: "Runner failed to complete the benchmark.".into(),
                scores: vec![BenchmarkScore {
                    metric_id: "overall".into(),
                    value: 0.0,
                    unit: "ratio".into(),
                }],
                diagnostics: Vec::new(),
                artifacts: Vec::new(),
                execution: None,
            }],
        };

        let rendered = render_autoresearch_target(
            &catalog,
            Some(&results),
            "kg-navigation",
            &[],
            &[],
            AutoResearchRenderFormat::Issue,
        )
        .unwrap();

        assert!(rendered.contains("[failed]"));
        assert!(!rendered.contains("[ FAILED ]"));
    }

    #[test]
    fn sanitizes_available_results_score_fields() {
        let catalog = sample_catalog();
        let results = BenchmarkResults {
            runs: vec![BenchmarkRun {
                benchmark_id: "swe-qa-pro".into(),
                run_id: "baseline".into(),
                status: "validation_only".into(),
                summary: "Catalog validated".into(),
                scores: vec![BenchmarkScore {
                    metric_id: "overall".into(),
                    value: 1.0,
                    unit: "pass\n# injected".into(),
                }],
                diagnostics: Vec::new(),
                artifacts: Vec::new(),
                execution: None,
            }],
        };

        let rendered = render_autoresearch_target(
            &catalog,
            Some(&results),
            "kg-navigation",
            &[],
            &[],
            AutoResearchRenderFormat::Markdown,
        )
        .unwrap();

        assert!(rendered.contains("overall=1 pass # injected"));
        assert!(!rendered.contains("\n# injected"));
    }

    #[test]
    fn loads_catalog_from_toml() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("catalog.toml");
        let mut file = std::fs::File::create(&path).unwrap();
        writeln!(
            file,
            r#"
[[benchmarks]]
id = "swe-qa-pro"
name = "SWE-QA-Pro"
best_use = "Repository-grounded QA"
task_scale = "260 questions"
summary = "Repo QA benchmark"

  [[benchmarks.metrics]]
  id = "overall"
  label = "Overall"
  notes = "Judge average"

  [[benchmarks.sources]]
  label = "paper"
  url = "https://arxiv.org/abs/2603.16124"

[[autoresearch_components]]
id = "ablation"
title = "Ablation"
prompt_fragment = "Compare graph-only with hybrid retrieval."
benchmark_ids = ["swe-qa-pro"]

[[autoresearch_targets]]
id = "kg-navigation"
title = "KG navigation"
summary = "Improve navigation"
benchmark_ids = ["swe-qa-pro"]
component_ids = ["ablation"]
"#
        )
        .unwrap();

        let catalog = load_benchmark_catalog(&path).unwrap();
        assert_eq!(catalog.benchmarks.len(), 1);
        assert_eq!(catalog.autoresearch_targets.len(), 1);
    }
}
