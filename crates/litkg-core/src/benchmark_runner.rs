use crate::benchmark::{
    validate_benchmark_results, BenchmarkArtifact, BenchmarkCatalog, BenchmarkExecutionRecord,
    BenchmarkResults, BenchmarkRun, BenchmarkScore, BenchmarkSource,
};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct NormalizedBenchmarkRunArtifact {
    pub status: String,
    pub summary: String,
    #[serde(default)]
    pub scores: Vec<BenchmarkScore>,
    #[serde(default)]
    pub diagnostics: Vec<String>,
    #[serde(default)]
    pub artifacts: Vec<BenchmarkArtifact>,
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

pub fn run_benchmarks(
    catalog: &BenchmarkCatalog,
    integrations: &BenchmarkIntegrationCatalog,
    plan: Option<&BenchmarkRunPlan>,
    benchmark_ids: &[String],
) -> Result<BenchmarkResults> {
    validate_benchmark_integrations(catalog, integrations)?;
    if let Some(plan) = plan {
        validate_benchmark_run_plan(catalog, plan)?;
    }

    let selected_benchmarks = select_benchmarks(catalog, benchmark_ids)?;
    let integration_by_id = integration_map(integrations);
    let run_plan_by_benchmark = run_plan_map(plan);
    let mut results = BenchmarkResults::default();

    for benchmark in selected_benchmarks {
        let integration = integration_by_id
            .get(benchmark.id.as_str())
            .expect("validated integration coverage");
        let requests = run_plan_by_benchmark.get(benchmark.id.as_str());
        if let Some(requests) = requests {
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

            if !missing_binaries.is_empty() || !missing_env_vars.is_empty() {
                results.runs.push(BenchmarkRun {
                    benchmark_id: benchmark.id.clone(),
                    run_id: format!("{}-prerequisite-check", benchmark.id),
                    status: "unavailable".to_string(),
                    summary: "Benchmark integration is configured but missing local prerequisites."
                        .to_string(),
                    scores: Vec::new(),
                    diagnostics: missing_prereq_diagnostics(&missing_binaries, &missing_env_vars),
                    artifacts: Vec::new(),
                    execution: None,
                });
                continue;
            }

            for request in requests {
                results
                    .runs
                    .push(execute_run_request(catalog, integration, request)?);
            }
        } else {
            results.runs.push(BenchmarkRun {
                benchmark_id: benchmark.id.clone(),
                run_id: format!("{}-unconfigured", benchmark.id),
                status: "unconfigured".to_string(),
                summary: "No benchmark run request is configured for this benchmark.".to_string(),
                scores: Vec::new(),
                diagnostics: vec![
                    "Add a benchmark run plan entry with a command that writes normalized JSON to $LITKG_BENCHMARK_OUTPUT_PATH."
                        .to_string(),
                ],
                artifacts: Vec::new(),
                execution: None,
            });
        }
    }

    validate_benchmark_results(catalog, &results)?;
    Ok(results)
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

fn execute_run_request(
    catalog: &BenchmarkCatalog,
    integration: &BenchmarkIntegration,
    request: &BenchmarkRunRequest,
) -> Result<BenchmarkRun> {
    let tempdir = tempfile::tempdir().context("Failed to create benchmark runner tempdir")?;
    let output_path = tempdir.path().join("normalized-result.json");
    let artifact_dir = tempdir.path().join("artifacts");
    fs::create_dir_all(&artifact_dir).with_context(|| {
        format!(
            "Failed to create artifact directory {}",
            artifact_dir.display()
        )
    })?;

    let workdir = request
        .workdir
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or_else(|| env::current_dir().expect("current_dir is available"));

    let mut command = Command::new("sh");
    command.arg("-c").arg(&request.command);
    command.current_dir(&workdir);
    command.env("LITKG_BENCHMARK_ID", &request.benchmark_id);
    command.env("LITKG_BENCHMARK_RUN_ID", &request.run_id);
    command.env("LITKG_BENCHMARK_OUTPUT_PATH", &output_path);
    command.env("LITKG_BENCHMARK_ARTIFACT_DIR", &artifact_dir);
    for (key, value) in &request.env {
        command.env(key, value);
    }

    let execution = BenchmarkExecutionRecord {
        runner_kind: integration.runner_kind.clone(),
        command: request.command.clone(),
        workdir: workdir.display().to_string(),
    };

    let output = match command.output() {
        Ok(output) => output,
        Err(error) => {
            return Ok(BenchmarkRun {
                benchmark_id: request.benchmark_id.clone(),
                run_id: request.run_id.clone(),
                status: "runner_failed".to_string(),
                summary: "Failed to spawn benchmark command.".to_string(),
                scores: Vec::new(),
                diagnostics: vec![error.to_string()],
                artifacts: Vec::new(),
                execution: Some(execution),
            });
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

    if output_path.exists() {
        let raw = match fs::read_to_string(&output_path) {
            Ok(raw) => raw,
            Err(error) => {
                return Ok(BenchmarkRun {
                    benchmark_id: request.benchmark_id.clone(),
                    run_id: request.run_id.clone(),
                    status: "normalization_error".to_string(),
                    summary: "Failed to read normalized benchmark artifact.".to_string(),
                    scores: Vec::new(),
                    diagnostics: vec![
                        error.to_string(),
                        format!("Artifact path: {}", output_path.display()),
                    ],
                    artifacts: Vec::new(),
                    execution: Some(execution),
                });
            }
        };

        let normalized: NormalizedBenchmarkRunArtifact = match serde_json::from_str(&raw) {
            Ok(normalized) => normalized,
            Err(error) => {
                return Ok(BenchmarkRun {
                    benchmark_id: request.benchmark_id.clone(),
                    run_id: request.run_id.clone(),
                    status: "normalization_error".to_string(),
                    summary: "Failed to parse normalized benchmark artifact.".to_string(),
                    scores: Vec::new(),
                    diagnostics: vec![
                        error.to_string(),
                        format!("Artifact path: {}", output_path.display()),
                    ],
                    artifacts: Vec::new(),
                    execution: Some(execution),
                });
            }
        };

        let mut run = BenchmarkRun {
            benchmark_id: request.benchmark_id.clone(),
            run_id: request.run_id.clone(),
            status: normalized.status,
            summary: normalized.summary,
            scores: normalized.scores,
            diagnostics: normalized.diagnostics,
            artifacts: normalized.artifacts,
            execution: Some(execution.clone()),
        };
        push_command_streams(
            &mut run.diagnostics,
            &stdout,
            &stderr,
            !output.status.success(),
        );
        if !output.status.success() {
            run.diagnostics
                .push(format!("Command exited with status {}", output.status));
        }

        if let Err(error) = validate_benchmark_results(
            catalog,
            &BenchmarkResults {
                runs: vec![run.clone()],
            },
        ) {
            return Ok(BenchmarkRun {
                benchmark_id: request.benchmark_id.clone(),
                run_id: request.run_id.clone(),
                status: "normalization_error".to_string(),
                summary: "Normalized benchmark artifact failed validation.".to_string(),
                scores: Vec::new(),
                diagnostics: vec![
                    format!(
                        "The normalized artifact for `{}` did not match the benchmark metric schema.",
                        request.benchmark_id
                    ),
                    error.to_string(),
                    format!("Artifact path: {}", output_path.display()),
                ],
                artifacts: Vec::new(),
                execution: Some(execution),
            });
        }

        return Ok(run);
    }

    let mut diagnostics = Vec::new();
    push_command_streams(&mut diagnostics, &stdout, &stderr, true);
    diagnostics.push(format!(
        "Expected normalized JSON artifact at {}",
        output_path.display()
    ));
    diagnostics.push(format!("Command exited with status {}", output.status));

    Ok(BenchmarkRun {
        benchmark_id: request.benchmark_id.clone(),
        run_id: request.run_id.clone(),
        status: if output.status.success() {
            "normalization_error".to_string()
        } else {
            "runner_failed".to_string()
        },
        summary: if output.status.success() {
            "Benchmark command completed without emitting a normalized artifact.".to_string()
        } else {
            "Benchmark command failed before emitting a normalized artifact.".to_string()
        },
        scores: Vec::new(),
        diagnostics,
        artifacts: Vec::new(),
        execution: Some(execution),
    })
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

fn missing_prereq_diagnostics(
    missing_binaries: &[String],
    missing_env_vars: &[String],
) -> Vec<String> {
    let mut diagnostics = Vec::new();
    if !missing_binaries.is_empty() {
        diagnostics.push(format!(
            "Missing required binaries: {}",
            missing_binaries.join(", ")
        ));
    }
    if !missing_env_vars.is_empty() {
        diagnostics.push(format!(
            "Missing required environment variables: {}",
            missing_env_vars.join(", ")
        ));
    }
    diagnostics
}

fn push_command_streams(
    diagnostics: &mut Vec<String>,
    stdout: &str,
    stderr: &str,
    include_stdout: bool,
) {
    if include_stdout && !stdout.is_empty() {
        diagnostics.push(format!("stdout: {}", truncate(stdout)));
    }
    if !stderr.is_empty() {
        diagnostics.push(format!("stderr: {}", truncate(stderr)));
    }
}

fn truncate(value: &str) -> String {
    const MAX_CHARS: usize = 600;
    if value.chars().count() <= MAX_CHARS {
        return value.to_string();
    }
    let mut truncated = value.chars().take(MAX_CHARS).collect::<String>();
    truncated.push_str("...");
    truncated
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::benchmark::{
        AutoResearchComponent, AutoResearchTargetTemplate, BenchmarkCatalog, BenchmarkMetric,
        BenchmarkSpec,
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
            autoresearch_components: vec![AutoResearchComponent {
                id: "noop".into(),
                title: "No-op".into(),
                prompt_fragment: "No-op component.".into(),
                benchmark_ids: vec!["swe-qa-pro".into()],
                tags: vec![],
            }],
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

    #[test]
    fn runs_benchmark_command_and_normalizes_output() {
        let dir = tempfile::tempdir().unwrap();
        let script_path = dir.path().join("emit-result.sh");
        fs::write(
            &script_path,
            r#"#!/bin/sh
cat > "$LITKG_BENCHMARK_OUTPUT_PATH" <<'EOF'
{
  "status": "error",
  "summary": "Terminal benchmark completed.",
  "scores": [
    {
      "metric_id": "overall",
      "value": 0.42,
      "unit": "ratio"
    }
  ],
  "diagnostics": ["fixture-run"],
  "artifacts": [
    {
      "label": "raw-log",
      "kind": "log",
      "location": "artifacts/run.log"
    }
  ]
}
EOF
"#,
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = fs::metadata(&script_path).unwrap().permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&script_path, permissions).unwrap();
        }

        let results = run_benchmarks(
            &sample_catalog(),
            &sample_integrations(),
            Some(&BenchmarkRunPlan {
                runs: vec![BenchmarkRunRequest {
                    benchmark_id: "swe-qa-pro".into(),
                    run_id: "baseline".into(),
                    command: script_path.display().to_string(),
                    workdir: Some(dir.path().display().to_string()),
                    env: BTreeMap::new(),
                }],
            }),
            &["swe-qa-pro".into()],
        )
        .unwrap();

        assert_eq!(results.runs.len(), 1);
        assert_eq!(results.runs[0].status, "error");
        assert_eq!(results.runs[0].scores[0].metric_id, "overall");
        assert_eq!(
            results.runs[0].execution.as_ref().unwrap().runner_kind,
            "external_command"
        );
        assert_eq!(results.runs[0].diagnostics[0], "fixture-run");
    }
}
