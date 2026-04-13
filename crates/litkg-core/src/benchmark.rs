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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoResearchRenderFormat {
    Markdown,
    Json,
    GitHubIssue,
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
    let mut lines = vec![
        format!("Suggested issue title: [AutoResearch] {}", rendered.title),
        String::new(),
        "## Summary".to_string(),
        String::new(),
        rendered.summary.clone(),
        String::new(),
        "## Benchmarks In Scope".to_string(),
        String::new(),
    ];

    for benchmark in &rendered.benchmarks {
        lines.push(format!(
            "- `{}` (`{}`): {}. Task scale: {}",
            benchmark.id, benchmark.name, benchmark.best_use, benchmark.task_scale
        ));
    }

    lines.push(String::new());
    lines.push("## Current Signals".to_string());
    lines.push(String::new());
    if rendered.runs.is_empty() {
        lines.push("- No benchmark results were provided for this target render.".to_string());
    } else {
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
    lines.push("## Proposed Work".to_string());
    lines.push(String::new());
    for component in &rendered.components {
        lines.push(format!(
            "- [ ] **{}**: {}",
            component.title,
            component.prompt_fragment.trim()
        ));
    }

    lines.push(String::new());
    lines.push("## Acceptance Criteria".to_string());
    lines.push(String::new());
    lines.push("- [ ] `make benchmark-validate` passes.".to_string());
    lines.push(
        "- [ ] The selected autoresearch target renders cleanly in `markdown` format.".to_string(),
    );
    lines.push(
        "- [ ] The selected autoresearch target renders cleanly in `json` format.".to_string(),
    );
    lines.push(
        "- [ ] The resulting issue body stays deterministic under repeated renders.".to_string(),
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
        assert!(rendered.contains("Suggested issue title: [AutoResearch] KG navigation"));
        assert!(rendered.contains("## Proposed Work"));
        assert!(rendered.contains("- [ ] **Ablation**"));
        assert!(rendered.contains("## Acceptance Criteria"));
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
