use crate::benchmark::{BenchmarkCatalog, BenchmarkSource};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct BenchmarkIntegrationCatalog {
    #[serde(default)]
    pub integrations: Vec<BenchmarkIntegration>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BenchmarkIntegration {
    pub benchmark_id: String,
    pub upstream_status: String,
    pub runner_kind: String,
    pub summary: String,
    #[serde(default)]
    pub official_sources: Vec<BenchmarkSource>,
    #[serde(default)]
    pub required_binaries: Vec<String>,
    #[serde(default)]
    pub required_env_vars: Vec<String>,
    #[serde(default)]
    pub bootstrap_steps: Vec<String>,
    #[serde(default)]
    pub example_commands: Vec<String>,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct BenchmarkRunPlan {
    #[serde(default)]
    pub runs: Vec<BenchmarkRunRequest>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BenchmarkRunRequest {
    pub benchmark_id: String,
    pub run_id: String,
    pub command: String,
    #[serde(default)]
    pub workdir: Option<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct BenchmarkSupportStatus {
    pub benchmark_id: String,
    pub benchmark_name: String,
    pub upstream_status: String,
    pub runner_kind: String,
    pub local_status: String,
    pub configured_runs: usize,
    pub missing_binaries: Vec<String>,
    pub missing_env_vars: Vec<String>,
    pub summary: String,
    pub official_sources: Vec<BenchmarkSource>,
    pub notes: Vec<String>,
}

pub fn load_benchmark_integrations(path: impl AsRef<Path>) -> Result<BenchmarkIntegrationCatalog> {
    let path = path.as_ref();
    let raw = fs::read_to_string(path).with_context(|| {
        format!(
            "Failed to read benchmark integration catalog {}",
            path.display()
        )
    })?;
    toml::from_str(&raw).with_context(|| {
        format!(
            "Failed to parse benchmark integration catalog {}",
            path.display()
        )
    })
}

pub fn load_benchmark_run_plan(path: impl AsRef<Path>) -> Result<BenchmarkRunPlan> {
    let path = path.as_ref();
    let raw = fs::read_to_string(path)
        .with_context(|| format!("Failed to read benchmark run plan {}", path.display()))?;
    toml::from_str(&raw)
        .with_context(|| format!("Failed to parse benchmark run plan {}", path.display()))
}

pub fn validate_benchmark_integrations(
    catalog: &BenchmarkCatalog,
    integrations: &BenchmarkIntegrationCatalog,
) -> Result<()> {
    let benchmark_ids: BTreeSet<&str> = catalog
        .benchmarks
        .iter()
        .map(|benchmark| benchmark.id.as_str())
        .collect();
    let mut seen = BTreeSet::new();

    for integration in &integrations.integrations {
        if !seen.insert(integration.benchmark_id.as_str()) {
            bail!(
                "Duplicate benchmark integration `{}`",
                integration.benchmark_id
            );
        }
        if !benchmark_ids.contains(integration.benchmark_id.as_str()) {
            bail!(
                "Benchmark integration `{}` references unknown benchmark",
                integration.benchmark_id
            );
        }
        if integration.upstream_status.trim().is_empty() {
            bail!(
                "Benchmark integration `{}` must have a non-empty upstream_status",
                integration.benchmark_id
            );
        }
        if integration.runner_kind.trim().is_empty() {
            bail!(
                "Benchmark integration `{}` must have a non-empty runner_kind",
                integration.benchmark_id
            );
        }
        if integration.summary.trim().is_empty() {
            bail!(
                "Benchmark integration `{}` must have a non-empty summary",
                integration.benchmark_id
            );
        }
        for source in &integration.official_sources {
            if source.label.trim().is_empty() || source.url.trim().is_empty() {
                bail!(
                    "Benchmark integration `{}` contains an empty source label or url",
                    integration.benchmark_id
                );
            }
        }
        for binary in &integration.required_binaries {
            if binary.trim().is_empty() {
                bail!(
                    "Benchmark integration `{}` contains an empty required binary",
                    integration.benchmark_id
                );
            }
        }
        for env_var in &integration.required_env_vars {
            if env_var.trim().is_empty() {
                bail!(
                    "Benchmark integration `{}` contains an empty required env var",
                    integration.benchmark_id
                );
            }
        }
    }

    for benchmark_id in benchmark_ids {
        if !seen.contains(benchmark_id) {
            bail!(
                "Benchmark `{}` is missing an integration definition",
                benchmark_id
            );
        }
    }

    Ok(())
}

pub fn validate_benchmark_run_plan(
    catalog: &BenchmarkCatalog,
    plan: &BenchmarkRunPlan,
) -> Result<()> {
    let benchmark_ids: BTreeSet<&str> = catalog
        .benchmarks
        .iter()
        .map(|benchmark| benchmark.id.as_str())
        .collect();
    let mut seen_run_ids = BTreeSet::new();

    for request in &plan.runs {
        if !benchmark_ids.contains(request.benchmark_id.as_str()) {
            bail!(
                "Benchmark run `{}` references unknown benchmark `{}`",
                request.run_id,
                request.benchmark_id
            );
        }
        if request.run_id.trim().is_empty() {
            bail!(
                "Benchmark run request for `{}` must have a non-empty run_id",
                request.benchmark_id
            );
        }
        if !seen_run_ids.insert(request.run_id.as_str()) {
            bail!("Duplicate benchmark run request id `{}`", request.run_id);
        }
        if request.command.trim().is_empty() {
            bail!(
                "Benchmark run `{}` must have a non-empty command",
                request.run_id
            );
        }
        if let Some(workdir) = &request.workdir {
            if workdir.trim().is_empty() {
                bail!(
                    "Benchmark run `{}` contains an empty workdir",
                    request.run_id
                );
            }
        }
        for (key, value) in &request.env {
            if key.trim().is_empty() || value.trim().is_empty() {
                bail!(
                    "Benchmark run `{}` contains an empty environment key or value",
                    request.run_id
                );
            }
        }
    }

    Ok(())
}

pub fn inspect_benchmark_support(
    catalog: &BenchmarkCatalog,
    integrations: &BenchmarkIntegrationCatalog,
    plan: Option<&BenchmarkRunPlan>,
    benchmark_ids: &[String],
) -> Result<Vec<BenchmarkSupportStatus>> {
    validate_benchmark_integrations(catalog, integrations)?;
    if let Some(plan) = plan {
        validate_benchmark_run_plan(catalog, plan)?;
    }

    let selected_benchmarks = select_benchmarks(catalog, benchmark_ids)?;
    let integration_by_id = integration_map(integrations);
    let run_plan_by_benchmark = run_plan_map(plan);

    selected_benchmarks
        .into_iter()
        .map(|benchmark| {
            let integration = integration_by_id
                .get(benchmark.id.as_str())
                .expect("validated integration coverage");
            let configured_runs = run_plan_by_benchmark
                .get(benchmark.id.as_str())
                .map_or(0usize, Vec::len);
            let missing_binaries = integration
                .required_binaries
                .iter()
                .filter(|binary| !binary_exists(binary))
                .cloned()
                .collect::<Vec<_>>();
            let missing_env_vars = integration
                .required_env_vars
                .iter()
                .filter(|name| {
                    env::var(name.as_str())
                        .map(|value| value.trim().is_empty())
                        .unwrap_or(true)
                })
                .cloned()
                .collect::<Vec<_>>();
            let local_status = if configured_runs == 0 {
                "not_configured"
            } else if missing_binaries.is_empty() && missing_env_vars.is_empty() {
                "ready"
            } else {
                "missing_prerequisites"
            };

            Ok(BenchmarkSupportStatus {
                benchmark_id: benchmark.id.clone(),
                benchmark_name: benchmark.name.clone(),
                upstream_status: integration.upstream_status.clone(),
                runner_kind: integration.runner_kind.clone(),
                local_status: local_status.to_string(),
                configured_runs,
                missing_binaries,
                missing_env_vars,
                summary: integration.summary.clone(),
                official_sources: integration.official_sources.clone(),
                notes: integration.notes.clone(),
            })
        })
        .collect()
}

fn select_benchmarks<'a>(
    catalog: &'a BenchmarkCatalog,
    benchmark_ids: &[String],
) -> Result<Vec<&'a crate::benchmark::BenchmarkSpec>> {
    if benchmark_ids.is_empty() {
        return Ok(catalog.benchmarks.iter().collect());
    }

    benchmark_ids
        .iter()
        .map(|benchmark_id| {
            catalog
                .benchmarks
                .iter()
                .find(|benchmark| benchmark.id == *benchmark_id)
                .with_context(|| format!("Unknown benchmark `{benchmark_id}`"))
        })
        .collect()
}

fn integration_map(
    integrations: &BenchmarkIntegrationCatalog,
) -> BTreeMap<&str, &BenchmarkIntegration> {
    integrations
        .integrations
        .iter()
        .map(|integration| (integration.benchmark_id.as_str(), integration))
        .collect()
}

fn run_plan_map(plan: Option<&BenchmarkRunPlan>) -> BTreeMap<&str, Vec<&BenchmarkRunRequest>> {
    let mut grouped = BTreeMap::new();
    if let Some(plan) = plan {
        for request in &plan.runs {
            grouped
                .entry(request.benchmark_id.as_str())
                .or_insert_with(Vec::new)
                .push(request);
        }
    }
    grouped
}

fn binary_exists(binary: &str) -> bool {
    let binary = binary.trim();
    if binary.contains(std::path::MAIN_SEPARATOR) {
        return PathBuf::from(binary).exists();
    }

    env::var_os("PATH")
        .map(|paths| {
            env::split_paths(&paths).any(|path| {
                let candidate = path.join(binary);
                candidate.exists()
            })
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::benchmark::{
        AutoResearchTargetTemplate, BenchmarkCatalog, BenchmarkMetric, BenchmarkSpec,
    };

    fn sample_catalog() -> BenchmarkCatalog {
        BenchmarkCatalog {
            benchmarks: vec![BenchmarkSpec {
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
                    url: "https://example.com/paper".into(),
                }],
            }],
            autoresearch_components: vec![],
            autoresearch_targets: vec![AutoResearchTargetTemplate {
                id: "kg-navigation".into(),
                title: "KG navigation".into(),
                summary: "Improve navigation".into(),
                benchmark_ids: vec!["swe-qa-pro".into()],
                component_ids: vec!["noop".into()],
            }],
        }
    }

    fn sample_integrations() -> BenchmarkIntegrationCatalog {
        BenchmarkIntegrationCatalog {
            integrations: vec![BenchmarkIntegration {
                benchmark_id: "swe-qa-pro".into(),
                upstream_status: "dataset_only".into(),
                runner_kind: "external_command".into(),
                summary: "Custom evaluator required.".into(),
                official_sources: vec![BenchmarkSource {
                    label: "github".into(),
                    url: "https://example.com/repo".into(),
                }],
                required_binaries: vec![],
                required_env_vars: vec![],
                bootstrap_steps: vec![],
                example_commands: vec![],
                notes: vec!["custom adapter".into()],
            }],
        }
    }

    #[test]
    fn validates_benchmark_integrations() {
        validate_benchmark_integrations(&sample_catalog(), &sample_integrations()).unwrap();
    }

    #[test]
    fn validates_benchmark_run_plan() {
        validate_benchmark_run_plan(
            &sample_catalog(),
            &BenchmarkRunPlan {
                runs: vec![BenchmarkRunRequest {
                    benchmark_id: "swe-qa-pro".into(),
                    run_id: "baseline".into(),
                    command: "echo {}".into(),
                    workdir: Some(".".into()),
                    env: BTreeMap::from([("TOKEN".into(), "abc".into())]),
                }],
            },
        )
        .unwrap();
    }

    #[test]
    fn inspects_benchmark_support() {
        let statuses = inspect_benchmark_support(
            &sample_catalog(),
            &sample_integrations(),
            Some(&BenchmarkRunPlan {
                runs: vec![BenchmarkRunRequest {
                    benchmark_id: "swe-qa-pro".into(),
                    run_id: "baseline".into(),
                    command: "echo {}".into(),
                    workdir: None,
                    env: BTreeMap::new(),
                }],
            }),
            &[],
        )
        .unwrap();

        assert_eq!(statuses.len(), 1);
        assert_eq!(statuses[0].benchmark_id, "swe-qa-pro");
        assert_eq!(statuses[0].local_status, "ready");
        assert_eq!(statuses[0].configured_runs, 1);
    }
}
