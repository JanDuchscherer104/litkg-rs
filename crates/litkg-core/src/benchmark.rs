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
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BenchmarkScore {
    pub metric_id: String,
    pub value: f64,
    pub unit: String,
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

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PromotedRunSummary {
    pub benchmark_id: String,
    pub run_id: String,
    pub status: String,
    pub disposition: AutoResearchResultDisposition,
    pub reason: String,
    pub summary: String,
    pub evidence: Vec<String>,
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
        if run.summary.trim().is_empty() {
            bail!(
                "Benchmark run `{}` must have a non-empty summary",
                run.run_id
            );
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
        promotion_summary: build_promotion_summary(&result_summaries),
        result_summaries,
    };

    match format {
        AutoResearchRenderFormat::Markdown => Ok(render_markdown_target(&rendered)),
        AutoResearchRenderFormat::Issue => Ok(render_issue_target(&rendered)),
        AutoResearchRenderFormat::Json => serde_json::to_string_pretty(&rendered)
            .context("Failed to serialize autoresearch target as JSON"),
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
                run.scores
                    .iter()
                    .map(|score| format!("{}={} {}", score.metric_id, score.value, score.unit))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            lines.push(format!(
                "- `{}` on `{}` [{}]: {}",
                run.run_id, run.benchmark_id, run.status, score_summary
            ));
        }

        lines.push(String::new());
        lines.push("## Result Promotion Assessment".to_string());
        lines.push(String::new());
        lines.push(format!(
            "Promotion summary: {} promotable, {} deferred.",
            rendered.promotion_summary.promotable_count, rendered.promotion_summary.deferred_count
        ));
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
            if !summary.evidence.is_empty() {
                lines.push(format!("  Evidence: {}", summary.evidence.join("; ")));
            }
        }

        if !rendered.result_summaries.is_empty()
            && rendered
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
        rendered.summary.clone(),
        String::new(),
        "## Benchmark Context".to_string(),
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
    lines.push("## Promoted Result Inputs".to_string());
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
                render_evidence_suffix(summary)
            ));
        }
    }

    if !deferred.is_empty() {
        lines.push(String::new());
        lines.push("## Deferred Result Inputs".to_string());
        lines.push(String::new());
        for summary in deferred {
            lines.push(format!(
                "- `{}` on `{}` [{}]: {} Summary: {}{}",
                summary.run_id,
                summary.benchmark_id,
                summary.status,
                summary.reason,
                summary.summary,
                render_evidence_suffix(summary)
            ));
        }
    }

    lines.push(String::new());
    lines.push("## Proposed Research Work".to_string());
    lines.push(String::new());
    for (index, component) in rendered.components.iter().enumerate() {
        lines.push(format!("### {}. {}", index + 1, component.title));
        lines.push(String::new());
        lines.push(component.prompt_fragment.clone());
        lines.push(String::new());
    }
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

    let mut selected_runs = results
        .runs
        .iter()
        .filter(|run| selected_benchmark_ids.contains(&run.benchmark_id))
        .map(summarize_run_for_promotion)
        .collect::<Vec<_>>();
    selected_runs.sort_by(|left, right| {
        left.benchmark_id
            .cmp(&right.benchmark_id)
            .then(left.run_id.cmp(&right.run_id))
    });
    selected_runs
}

fn summarize_run_for_promotion(run: &BenchmarkRun) -> PromotedRunSummary {
    let normalized_status = run.status.trim().to_ascii_lowercase();
    let sanitized_summary = sanitize_inline_markdown(&run.summary);
    let (disposition, reason) = if normalized_status == "validation_only" {
        (
            AutoResearchResultDisposition::Defer,
            "validation-only run; keep it as a schema check, not as evidence for the next target"
                .to_string(),
        )
    } else if is_success_status(&normalized_status) {
        (
            AutoResearchResultDisposition::Defer,
            "successful execution run; keep it for release tracking rather than the next target"
                .to_string(),
        )
    } else if is_promotable_failure_status(&normalized_status) {
        (
            AutoResearchResultDisposition::Promote,
            "recognized execution failure run; eligible to shape the next deterministic research target"
                .to_string(),
        )
    } else {
        (
            AutoResearchResultDisposition::Defer,
            "non-promotable run status; keep it out of benchmark-deficit targeting until the status is classified explicitly"
                .to_string(),
        )
    };

    PromotedRunSummary {
        benchmark_id: run.benchmark_id.clone(),
        run_id: run.run_id.clone(),
        status: run.status.clone(),
        disposition,
        reason,
        summary: sanitized_summary,
        evidence: collect_run_evidence(run),
    }
}

fn is_success_status(status: &str) -> bool {
    matches!(
        status,
        "success" | "successful" | "passed" | "pass" | "completed" | "ok"
    )
}

fn is_promotable_failure_status(status: &str) -> bool {
    matches!(
        status,
        "observed_failure" | "failure" | "failed" | "regression" | "degraded" | "partial_failure"
    )
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

fn collect_run_evidence(run: &BenchmarkRun) -> Vec<String> {
    let mut evidence = Vec::new();

    for score in &run.scores {
        evidence.push(format!(
            "score {}={} {}",
            score.metric_id, score.value, score.unit
        ));
    }

    evidence
}

fn sanitize_inline_markdown(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn render_evidence_suffix(summary: &PromotedRunSummary) -> String {
    if summary.evidence.is_empty() {
        String::new()
    } else {
        format!(" Evidence: {}", summary.evidence.join("; "))
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
        assert!(rendered.contains("non-promotable run status"));
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

        assert!(rendered.contains("## Promoted Result Inputs"));
        assert!(rendered.contains("retrieval-regression"));
        assert!(rendered.contains("Evidence: score overall=0.42 ratio"));
        assert!(rendered.contains("## Proposed Research Work"));
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
        assert!(rendered.contains("## Deferred Result Inputs"));
        assert!(rendered.contains("## Proposed Research Work"));
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
        assert!(rendered.contains("## Deferred Result Inputs"));
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
        assert!(rendered.contains("\"evidence\""));
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
