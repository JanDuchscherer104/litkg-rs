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
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PromotedAutoResearchTarget {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub benchmarks: Vec<BenchmarkSpec>,
    pub components: Vec<AutoResearchComponent>,
    pub runs: Vec<BenchmarkRun>,
    pub evidence: Vec<PromotionEvidence>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PromotionEvidence {
    pub benchmark_id: String,
    pub run_id: String,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoResearchRenderFormat {
    Markdown,
    Json,
    GithubIssue,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PromotionComponentSelection {
    #[default]
    TemplateOnly,
    TemplateAndMatched,
    MatchedOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetricThresholdComparison {
    LessThan,
    LessThanOrEqual,
    GreaterThan,
    GreaterThanOrEqual,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MetricThresholdRule {
    pub metric_id: String,
    pub comparison: MetricThresholdComparison,
    pub value: f64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct BenchmarkPromotionRequest {
    pub target_ids: Vec<String>,
    pub benchmark_ids: Vec<String>,
    pub status_filters: Vec<String>,
    pub metric_thresholds: Vec<MetricThresholdRule>,
    pub component_selection: PromotionComponentSelection,
    pub component_ids: Vec<String>,
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
        }
    }

    summary.run_count = results.runs.len();
    Ok(summary)
}

pub fn promote_benchmark_results(
    catalog: &BenchmarkCatalog,
    results: &BenchmarkResults,
    request: &BenchmarkPromotionRequest,
) -> Result<Vec<PromotedAutoResearchTarget>> {
    validate_benchmark_results(catalog, results)?;

    let benchmark_by_id = benchmark_map(catalog);
    let component_by_id = component_map(catalog);
    let target_filter = requested_id_set(&request.target_ids, "autoresearch target")?;
    let benchmark_filter = requested_id_set(&request.benchmark_ids, "benchmark")?;
    let status_filter = requested_status_set(&request.status_filters);

    if let Some(target_filter) = &target_filter {
        for target_id in target_filter {
            if !catalog
                .autoresearch_targets
                .iter()
                .any(|target| target.id == *target_id)
            {
                bail!("Unknown autoresearch target `{target_id}`");
            }
        }
    }
    if let Some(benchmark_filter) = &benchmark_filter {
        for benchmark_id in benchmark_filter {
            if !benchmark_by_id.contains_key(benchmark_id) {
                bail!("Unknown benchmark `{benchmark_id}`");
            }
        }
    }

    let matched_runs = results
        .runs
        .iter()
        .filter_map(|run| {
            promotion_evidence_for_run(
                run,
                benchmark_filter.as_ref(),
                status_filter.as_ref(),
                &request.metric_thresholds,
            )
            .map(|evidence| (run.clone(), evidence))
        })
        .collect::<Vec<_>>();

    let matched_benchmark_ids = matched_runs
        .iter()
        .map(|(run, _)| run.benchmark_id.clone())
        .collect::<BTreeSet<_>>();
    let selected_targets = catalog
        .autoresearch_targets
        .iter()
        .filter(|target| {
            target_filter
                .as_ref()
                .map(|filter| filter.contains(&target.id))
                .unwrap_or(true)
        })
        .collect::<Vec<_>>();
    let mut promoted = Vec::new();

    for target in selected_targets {
        let selected_benchmark_ids = target
            .benchmark_ids
            .iter()
            .filter(|benchmark_id| matched_benchmark_ids.contains(*benchmark_id))
            .cloned()
            .collect::<Vec<_>>();
        if selected_benchmark_ids.is_empty() {
            continue;
        }

        let benchmarks = selected_benchmark_ids
            .iter()
            .map(|benchmark_id| {
                benchmark_by_id
                    .get(benchmark_id)
                    .cloned()
                    .with_context(|| format!("Unknown benchmark `{benchmark_id}`"))
            })
            .collect::<Result<Vec<_>>>()?;
        let component_ids =
            select_component_ids(catalog, target, &selected_benchmark_ids, request)?;
        let components = component_ids
            .iter()
            .map(|component_id| {
                component_by_id
                    .get(component_id)
                    .cloned()
                    .with_context(|| format!("Unknown autoresearch component `{component_id}`"))
            })
            .collect::<Result<Vec<_>>>()?;

        let benchmark_order = selected_benchmark_ids
            .iter()
            .enumerate()
            .map(|(index, benchmark_id)| (benchmark_id.clone(), index))
            .collect::<BTreeMap<_, _>>();
        let mut runs = matched_runs
            .iter()
            .filter(|(run, _)| selected_benchmark_ids.contains(&run.benchmark_id))
            .map(|(run, _)| run.clone())
            .collect::<Vec<_>>();
        runs.sort_by(|left, right| {
            benchmark_order
                .get(&left.benchmark_id)
                .cmp(&benchmark_order.get(&right.benchmark_id))
                .then_with(|| left.run_id.cmp(&right.run_id))
        });

        let evidence = runs
            .iter()
            .filter_map(|run| {
                matched_runs
                    .iter()
                    .find(|(candidate, _)| {
                        candidate.run_id == run.run_id && candidate.benchmark_id == run.benchmark_id
                    })
                    .map(|(_, evidence)| evidence.clone())
            })
            .collect::<Vec<_>>();

        promoted.push(PromotedAutoResearchTarget {
            id: target.id.clone(),
            title: target.title.clone(),
            summary: target.summary.clone(),
            benchmarks,
            components,
            runs,
            evidence,
        });
    }

    Ok(promoted)
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

    let rendered = RenderedAutoResearchTarget {
        id: target.id.clone(),
        title: target.title.clone(),
        summary: target.summary.clone(),
        benchmarks: selected_benchmarks,
        components: selected_components,
        runs: selected_runs,
    };

    match format {
        AutoResearchRenderFormat::Markdown => Ok(render_markdown_target(&rendered)),
        AutoResearchRenderFormat::Json => serde_json::to_string_pretty(&rendered)
            .context("Failed to serialize autoresearch target as JSON"),
        AutoResearchRenderFormat::GithubIssue => Ok(render_target_as_github_issue(&rendered)),
    }
}

pub fn render_promoted_targets(
    targets: &[PromotedAutoResearchTarget],
    format: AutoResearchRenderFormat,
) -> Result<String> {
    if targets.is_empty() {
        bail!("No autoresearch targets matched the supplied promotion filters");
    }

    match format {
        AutoResearchRenderFormat::Markdown => Ok(targets
            .iter()
            .map(render_promoted_target_markdown)
            .collect::<Vec<_>>()
            .join("\n\n---\n\n")),
        AutoResearchRenderFormat::Json => serde_json::to_string_pretty(targets)
            .context("Failed to serialize promoted autoresearch targets as JSON"),
        AutoResearchRenderFormat::GithubIssue => Ok(targets
            .iter()
            .map(render_promoted_target_as_github_issue)
            .collect::<Vec<_>>()
            .join("\n\n---\n\n")),
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

fn render_target_as_github_issue(rendered: &RenderedAutoResearchTarget) -> String {
    let benchmark_list = rendered
        .benchmarks
        .iter()
        .map(|benchmark| format!("- `{}`: {}", benchmark.id, benchmark.summary))
        .collect::<Vec<_>>();
    let component_list = rendered
        .components
        .iter()
        .enumerate()
        .map(|(index, component)| format!("{}. {}", index + 1, component.title))
        .collect::<Vec<_>>();
    let result_list = rendered
        .runs
        .iter()
        .map(|run| {
            let score_summary = if run.scores.is_empty() {
                "no scores recorded".to_string()
            } else {
                run.scores
                    .iter()
                    .map(|score| format!("{}={} {}", score.metric_id, score.value, score.unit))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            format!(
                "- `{}` on `{}` [{}]: {}",
                run.run_id, run.benchmark_id, run.status, score_summary
            )
        })
        .collect::<Vec<_>>();

    let mut lines = vec![
        format!("Title: Auto Research: {}", rendered.title),
        String::new(),
        "## Summary".to_string(),
        rendered.summary.clone(),
        String::new(),
        "## Selected Benchmarks".to_string(),
    ];
    lines.extend(benchmark_list);
    if !result_list.is_empty() {
        lines.push(String::new());
        lines.push("## Available Results".to_string());
        lines.extend(result_list);
    }
    lines.push(String::new());
    lines.push("## Proposed Work".to_string());
    lines.extend(component_list);
    lines.push(String::new());
    lines.push("## Acceptance Criteria".to_string());
    lines.push(
        "- Re-render the target and confirm the selected benchmark set is unchanged.".to_string(),
    );
    lines.push(
        "- Validate the benchmark catalog and results before and after the change.".to_string(),
    );
    lines.join("\n")
}

fn render_promoted_target_markdown(target: &PromotedAutoResearchTarget) -> String {
    let mut lines = vec![
        format!("# Promoted Auto Research Target: {}", target.title),
        String::new(),
        target.summary.clone(),
        String::new(),
        "## Triggering Evidence".to_string(),
        String::new(),
    ];

    for evidence in &target.evidence {
        lines.push(format!(
            "- `{}` on `{}`: {}",
            evidence.run_id,
            evidence.benchmark_id,
            evidence.reasons.join("; ")
        ));
    }

    lines.push(String::new());
    lines.push("## Selected Benchmarks".to_string());
    lines.push(String::new());
    for benchmark in &target.benchmarks {
        lines.push(format!(
            "- `{}`: {}. Task scale: {}",
            benchmark.id, benchmark.best_use, benchmark.task_scale
        ));
    }

    if !target.runs.is_empty() {
        lines.push(String::new());
        lines.push("## Promoted Runs".to_string());
        lines.push(String::new());
        for run in &target.runs {
            append_run_details(&mut lines, run);
        }
    }

    lines.push(String::new());
    lines.push("## Concatenated Components".to_string());
    lines.push(String::new());
    for (index, component) in target.components.iter().enumerate() {
        lines.push(format!("### {}. {}", index + 1, component.title));
        lines.push(String::new());
        lines.push(component.prompt_fragment.clone());
        lines.push(String::new());
    }

    lines.join("\n")
}

fn render_promoted_target_as_github_issue(target: &PromotedAutoResearchTarget) -> String {
    let benchmarks = target
        .benchmarks
        .iter()
        .map(|benchmark| format!("- `{}`: {}", benchmark.id, benchmark.summary))
        .collect::<Vec<_>>();
    let evidence = target
        .evidence
        .iter()
        .map(|evidence| {
            format!(
                "- `{}` on `{}`: {}",
                evidence.run_id,
                evidence.benchmark_id,
                evidence.reasons.join("; ")
            )
        })
        .collect::<Vec<_>>();
    let components = target
        .components
        .iter()
        .enumerate()
        .map(|(index, component)| format!("{}. {}", index + 1, component.title))
        .collect::<Vec<_>>();

    let mut lines = vec![
        format!("Title: Auto Research: {}", target.title),
        String::new(),
        "## Summary".to_string(),
        target.summary.clone(),
        String::new(),
        "## Triggering Evidence".to_string(),
    ];
    lines.extend(evidence);
    lines.push(String::new());
    lines.push("## Benchmarks To Keep Frozen".to_string());
    lines.extend(benchmarks);
    lines.push(String::new());
    lines.push("## Proposed Work".to_string());
    lines.extend(components);
    lines.push(String::new());
    lines.push("## Validation".to_string());
    lines.push(
        "- `cargo run -p litkg-cli -- validate-benchmarks --catalog ... --results ...`".to_string(),
    );
    lines.push("- `cargo run -p litkg-cli -- render-autoresearch-target --catalog ... --results ... --target-id ...`".to_string());
    lines.join("\n")
}

fn append_run_details(lines: &mut Vec<String>, run: &BenchmarkRun) {
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
    if !run.diagnostics.is_empty() {
        lines.push(format!("  diagnostics: {}", run.diagnostics.join(" | ")));
    }
    if !run.artifacts.is_empty() {
        lines.push(format!(
            "  artifacts: {}",
            run.artifacts
                .iter()
                .map(|artifact| format!(
                    "{} ({}) -> {}",
                    artifact.label, artifact.kind, artifact.location
                ))
                .collect::<Vec<_>>()
                .join(" | ")
        ));
    }
    if let Some(execution) = &run.execution {
        lines.push(format!(
            "  execution: {} via `{}` in `{}`",
            execution.runner_kind, execution.command, execution.workdir
        ));
    }
    lines.push(format!("  summary: {}", run.summary));
}

fn requested_id_set(ids: &[String], label: &str) -> Result<Option<BTreeSet<String>>> {
    if ids.is_empty() {
        return Ok(None);
    }
    Ok(Some(ensure_unique_named_items(
        ids.iter().map(|id| id.as_str()),
        label,
    )?))
}

fn requested_status_set(statuses: &[String]) -> Option<BTreeSet<String>> {
    if statuses.is_empty() {
        return None;
    }
    Some(
        statuses
            .iter()
            .map(|status| status.trim().to_string())
            .filter(|status| !status.is_empty())
            .collect(),
    )
}

fn promotion_evidence_for_run(
    run: &BenchmarkRun,
    benchmark_filter: Option<&BTreeSet<String>>,
    status_filter: Option<&BTreeSet<String>>,
    metric_thresholds: &[MetricThresholdRule],
) -> Option<PromotionEvidence> {
    if benchmark_filter
        .map(|filter| !filter.contains(&run.benchmark_id))
        .unwrap_or(false)
    {
        return None;
    }
    if status_filter
        .map(|filter| !filter.contains(&run.status))
        .unwrap_or(false)
    {
        return None;
    }

    let mut reasons = Vec::new();
    if status_filter.is_some() {
        reasons.push(format!("status `{}` matched promotion filter", run.status));
    }

    let threshold_reasons = matched_threshold_reasons(run, metric_thresholds);
    if !metric_thresholds.is_empty() && threshold_reasons.is_empty() {
        return None;
    }
    reasons.extend(threshold_reasons);
    if reasons.is_empty() {
        reasons.push("selected by benchmark filter".to_string());
    }

    Some(PromotionEvidence {
        benchmark_id: run.benchmark_id.clone(),
        run_id: run.run_id.clone(),
        reasons,
    })
}

fn matched_threshold_reasons(run: &BenchmarkRun, rules: &[MetricThresholdRule]) -> Vec<String> {
    let mut reasons = Vec::new();
    for score in &run.scores {
        for rule in rules {
            if score.metric_id != rule.metric_id {
                continue;
            }
            if metric_matches_rule(score.value, rule) {
                reasons.push(format!(
                    "{} {} {} (observed {})",
                    rule.metric_id,
                    rule.comparison.symbol(),
                    rule.value,
                    score.value
                ));
            }
        }
    }
    reasons
}

fn metric_matches_rule(value: f64, rule: &MetricThresholdRule) -> bool {
    match rule.comparison {
        MetricThresholdComparison::LessThan => value < rule.value,
        MetricThresholdComparison::LessThanOrEqual => value <= rule.value,
        MetricThresholdComparison::GreaterThan => value > rule.value,
        MetricThresholdComparison::GreaterThanOrEqual => value >= rule.value,
    }
}

fn select_component_ids(
    catalog: &BenchmarkCatalog,
    target: &AutoResearchTargetTemplate,
    benchmark_ids: &[String],
    request: &BenchmarkPromotionRequest,
) -> Result<Vec<String>> {
    let mut component_ids = Vec::new();
    match request.component_selection {
        PromotionComponentSelection::TemplateOnly => {
            for component_id in &target.component_ids {
                push_unique(&mut component_ids, component_id.clone());
            }
        }
        PromotionComponentSelection::TemplateAndMatched => {
            for component_id in &target.component_ids {
                push_unique(&mut component_ids, component_id.clone());
            }
            for component in &catalog.autoresearch_components {
                if component
                    .benchmark_ids
                    .iter()
                    .any(|benchmark_id| benchmark_ids.contains(benchmark_id))
                {
                    push_unique(&mut component_ids, component.id.clone());
                }
            }
        }
        PromotionComponentSelection::MatchedOnly => {
            for component in &catalog.autoresearch_components {
                if component
                    .benchmark_ids
                    .iter()
                    .any(|benchmark_id| benchmark_ids.contains(benchmark_id))
                {
                    push_unique(&mut component_ids, component.id.clone());
                }
            }
        }
    }
    for component_id in &request.component_ids {
        push_unique(&mut component_ids, component_id.clone());
    }
    if component_ids.is_empty() {
        bail!(
            "Promotion for autoresearch target `{}` selected no components",
            target.id
        );
    }
    Ok(component_ids)
}

fn push_unique(values: &mut Vec<String>, candidate: String) {
    if !values.contains(&candidate) {
        values.push(candidate);
    }
}

impl MetricThresholdComparison {
    fn symbol(self) -> &'static str {
        match self {
            MetricThresholdComparison::LessThan => "<",
            MetricThresholdComparison::LessThanOrEqual => "<=",
            MetricThresholdComparison::GreaterThan => ">",
            MetricThresholdComparison::GreaterThanOrEqual => ">=",
        }
    }
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
            autoresearch_components: vec![
                AutoResearchComponent {
                    id: "ablation".into(),
                    title: "Ablation".into(),
                    prompt_fragment: "Compare graph-only with hybrid retrieval.".into(),
                    benchmark_ids: vec!["swe-qa-pro".into()],
                    tags: vec!["retrieval".into()],
                },
                AutoResearchComponent {
                    id: "docs".into(),
                    title: "Docs".into(),
                    prompt_fragment: "Tighten README and project memory surfaces.".into(),
                    benchmark_ids: vec!["swe-qa-pro".into()],
                    tags: vec!["docs".into()],
                },
            ],
            autoresearch_targets: vec![AutoResearchTargetTemplate {
                id: "kg-navigation".into(),
                title: "KG navigation".into(),
                summary: "Improve repository navigation quality.".into(),
                benchmark_ids: vec!["swe-qa-pro".into()],
                component_ids: vec!["ablation".into()],
            }],
        }
    }

    fn sample_results() -> BenchmarkResults {
        BenchmarkResults {
            runs: vec![BenchmarkRun {
                benchmark_id: "swe-qa-pro".into(),
                run_id: "baseline".into(),
                status: "needs_improvement".into(),
                summary: "Grounding fell off on cross-file questions.".into(),
                scores: vec![BenchmarkScore {
                    metric_id: "overall".into(),
                    value: 0.42,
                    unit: "score".into(),
                }],
                diagnostics: vec![
                    "Missed one supporting file during answer synthesis.".into(),
                    "Reasoning trace lost citation backreferences.".into(),
                ],
                artifacts: vec![BenchmarkArtifact {
                    label: "run-log".into(),
                    kind: "log".into(),
                    location: "artifacts/baseline.log".into(),
                }],
                execution: Some(BenchmarkExecutionRecord {
                    runner_kind: "mock-runner".into(),
                    command: "cargo run --example bench".into(),
                    workdir: "/tmp/litkg".into(),
                }),
            }],
        }
    }

    #[test]
    fn validates_catalog_and_results() {
        let catalog = sample_catalog();
        let summary = validate_benchmark_catalog(&catalog).unwrap();
        assert_eq!(summary.benchmark_count, 1);
        assert_eq!(summary.metric_count, 1);

        let results = sample_results();
        let result_summary = validate_benchmark_results(&catalog, &results).unwrap();
        assert_eq!(result_summary.run_count, 1);
    }

    #[test]
    fn renders_target_markdown() {
        let catalog = sample_catalog();
        let results = sample_results();
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
    }

    #[test]
    fn renders_target_as_github_issue() {
        let catalog = sample_catalog();
        let results = sample_results();
        let rendered = render_autoresearch_target(
            &catalog,
            Some(&results),
            "kg-navigation",
            &[],
            &[],
            AutoResearchRenderFormat::GithubIssue,
        )
        .unwrap();
        assert!(rendered.contains("Title: Auto Research: KG navigation"));
        assert!(rendered.contains("## Proposed Work"));
    }

    #[test]
    fn promotes_results_into_issue_ready_targets() {
        let catalog = sample_catalog();
        let results = sample_results();
        let promoted = promote_benchmark_results(
            &catalog,
            &results,
            &BenchmarkPromotionRequest {
                target_ids: vec!["kg-navigation".into()],
                benchmark_ids: vec![],
                status_filters: vec!["needs_improvement".into()],
                metric_thresholds: vec![MetricThresholdRule {
                    metric_id: "overall".into(),
                    comparison: MetricThresholdComparison::LessThanOrEqual,
                    value: 0.5,
                }],
                component_selection: PromotionComponentSelection::TemplateAndMatched,
                component_ids: vec![],
            },
        )
        .unwrap();
        assert_eq!(promoted.len(), 1);
        assert_eq!(promoted[0].components.len(), 2);
        assert!(promoted[0].evidence[0]
            .reasons
            .iter()
            .any(|reason| reason.contains("overall <= 0.5")));

        let rendered =
            render_promoted_targets(&promoted, AutoResearchRenderFormat::GithubIssue).unwrap();
        assert!(rendered.contains("Title: Auto Research: KG navigation"));
        assert!(rendered.contains("status `needs_improvement` matched promotion filter"));
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
