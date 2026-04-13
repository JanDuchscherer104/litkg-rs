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
use std::time::{SystemTime, UNIX_EPOCH};

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

#[derive(Debug, Clone, PartialEq, Eq)]
struct MlflowTrackingConfig {
    python_bin: String,
    tracking_uri: String,
    experiment_name: String,
    run_name_prefix: String,
    payload_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
struct MlflowTrackingPayload {
    benchmark_id: String,
    benchmark_name: String,
    run_id: String,
    run_name: String,
    status: String,
    summary: String,
    runner_kind: Option<String>,
    command: Option<String>,
    workdir: Option<String>,
    scores: Vec<BenchmarkScore>,
    diagnostics: Vec<String>,
    artifacts: Vec<MlflowTrackingArtifact>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct MlflowTrackingArtifact {
    label: String,
    kind: String,
    location: String,
    resolved_path: Option<String>,
}

const MLFLOW_LOG_SCRIPT: &str = r#"
import json
import os
import pathlib
import sys

payload_path = pathlib.Path(sys.argv[1])
payload = json.loads(payload_path.read_text(encoding='utf-8'))

try:
    import mlflow
except Exception as exc:
    print(f'mlflow import failed: {exc}', file=sys.stderr)
    sys.exit(3)

tracking_uri = os.environ.get('LITKG_MLFLOW_TRACKING_URI', '').strip()
if tracking_uri:
    mlflow.set_tracking_uri(tracking_uri)

experiment_name = os.environ.get('LITKG_MLFLOW_EXPERIMENT', 'litkg-benchmarks')
mlflow.set_experiment(experiment_name)

with mlflow.start_run(run_name=payload.get('run_name')):
    mlflow.set_tags({
        'benchmark_id': str(payload.get('benchmark_id', '')),
        'benchmark_name': str(payload.get('benchmark_name', '')),
        'run_id': str(payload.get('run_id', '')),
        'status': str(payload.get('status', '')),
    })

    params = {
        'runner_kind': payload.get('runner_kind', ''),
        'command': payload.get('command', ''),
        'workdir': payload.get('workdir', ''),
    }
    for key, value in params.items():
        if value:
            mlflow.log_param(key, str(value))

    for score in payload.get('scores', []):
        metric_id = score.get('metric_id')
        value = score.get('value')
        if metric_id is None or value is None:
            continue
        try:
            mlflow.log_metric(str(metric_id), float(value))
        except Exception:
            pass

    for artifact in payload.get('artifacts', []):
        resolved_path = artifact.get('resolved_path')
        if not resolved_path:
            continue
        if not os.path.exists(resolved_path):
            continue
        artifact_subdir = f"benchmark_artifacts/{artifact.get('kind', 'misc')}"
        mlflow.log_artifact(resolved_path, artifact_subdir)

    mlflow.log_artifact(str(payload_path), 'benchmark_payload')
"#;

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
    let mlflow_tracking = mlflow_tracking_from_env();
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
                let mut run = BenchmarkRun {
                    benchmark_id: benchmark.id.clone(),
                    run_id: format!("{}-prerequisite-check", benchmark.id),
                    status: "unavailable".to_string(),
                    summary: "Benchmark integration is configured but missing local prerequisites."
                        .to_string(),
                    scores: Vec::new(),
                    diagnostics: missing_prereq_diagnostics(&missing_binaries, &missing_env_vars),
                    artifacts: Vec::new(),
                    execution: None,
                };
                maybe_track_with_mlflow(
                    mlflow_tracking.as_ref(),
                    &benchmark.name,
                    &mut run,
                    None,
                    None,
                    None,
                );
                results.runs.push(run);
                continue;
            }

            for request in requests {
                results.runs.push(execute_run_request(
                    catalog,
                    &benchmark.name,
                    integration,
                    request,
                    mlflow_tracking.as_ref(),
                )?);
            }
        } else {
            let mut run = BenchmarkRun {
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
            };
            maybe_track_with_mlflow(
                mlflow_tracking.as_ref(),
                &benchmark.name,
                &mut run,
                None,
                None,
                None,
            );
            results.runs.push(run);
        }
    }

    validate_benchmark_results(catalog, &results)?;
    Ok(results)
}

fn execute_run_request(
    catalog: &BenchmarkCatalog,
    benchmark_name: &str,
    integration: &BenchmarkIntegration,
    request: &BenchmarkRunRequest,
    mlflow_tracking: Option<&MlflowTrackingConfig>,
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
    let workdir_display = workdir.display().to_string();

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
        workdir: workdir_display.clone(),
    };
    let output = match command.output() {
        Ok(output) => output,
        Err(error) => {
            let mut run = BenchmarkRun {
                benchmark_id: request.benchmark_id.clone(),
                run_id: request.run_id.clone(),
                status: "runner_failed".to_string(),
                summary: "Failed to spawn benchmark command.".to_string(),
                scores: Vec::new(),
                diagnostics: vec![error.to_string()],
                artifacts: Vec::new(),
                execution: Some(execution),
            };
            maybe_track_with_mlflow(
                mlflow_tracking,
                benchmark_name,
                &mut run,
                Some(workdir.as_path()),
                Some(artifact_dir.as_path()),
                None,
            );
            return Ok(run);
        }
    };
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

    if output_path.exists() {
        let raw = match fs::read_to_string(&output_path) {
            Ok(raw) => raw,
            Err(error) => {
                let mut run = BenchmarkRun {
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
                };
                maybe_track_with_mlflow(
                    mlflow_tracking,
                    benchmark_name,
                    &mut run,
                    Some(workdir.as_path()),
                    Some(artifact_dir.as_path()),
                    Some(output_path.as_path()),
                );
                return Ok(run);
            }
        };
        let normalized: NormalizedBenchmarkRunArtifact = match serde_json::from_str(&raw) {
            Ok(normalized) => normalized,
            Err(error) => {
                let mut run = BenchmarkRun {
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
                };
                maybe_track_with_mlflow(
                    mlflow_tracking,
                    benchmark_name,
                    &mut run,
                    Some(workdir.as_path()),
                    Some(artifact_dir.as_path()),
                    Some(output_path.as_path()),
                );
                return Ok(run);
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
            let mut run = BenchmarkRun {
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
            };
            maybe_track_with_mlflow(
                mlflow_tracking,
                benchmark_name,
                &mut run,
                Some(workdir.as_path()),
                Some(artifact_dir.as_path()),
                Some(output_path.as_path()),
            );
            return Ok(run);
        }

        maybe_track_with_mlflow(
            mlflow_tracking,
            benchmark_name,
            &mut run,
            Some(workdir.as_path()),
            Some(artifact_dir.as_path()),
            Some(output_path.as_path()),
        );
        return Ok(run);
    }

    let mut diagnostics = Vec::new();
    push_command_streams(&mut diagnostics, &stdout, &stderr, true);
    diagnostics.push(format!(
        "Expected normalized JSON artifact at {}",
        output_path.display()
    ));
    diagnostics.push(format!("Command exited with status {}", output.status));

    let mut run = BenchmarkRun {
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
    };
    maybe_track_with_mlflow(
        mlflow_tracking,
        benchmark_name,
        &mut run,
        Some(workdir.as_path()),
        Some(artifact_dir.as_path()),
        Some(output_path.as_path()),
    );
    Ok(run)
}

fn maybe_track_with_mlflow(
    config: Option<&MlflowTrackingConfig>,
    benchmark_name: &str,
    run: &mut BenchmarkRun,
    workdir: Option<&Path>,
    artifact_dir: Option<&Path>,
    normalized_output_path: Option<&Path>,
) {
    let Some(config) = config else {
        return;
    };
    let payload = build_mlflow_payload(
        config,
        benchmark_name,
        run,
        workdir,
        artifact_dir,
        normalized_output_path,
    );
    match write_mlflow_payload(config, &payload) {
        Ok(payload_path) => {
            attach_mlflow_payload_artifact(run, &payload_path);
            if let Err(error) = log_mlflow_payload(config, &payload_path) {
                run.diagnostics.push(format!(
                    "MLflow tracking failed: {}. Local payload: {}",
                    error,
                    payload_path.display()
                ));
            }
        }
        Err(error) => {
            run.diagnostics.push(format!(
                "MLflow tracking failed before payload logging: {}",
                error
            ));
        }
    }
}

fn build_mlflow_payload(
    config: &MlflowTrackingConfig,
    benchmark_name: &str,
    run: &BenchmarkRun,
    workdir: Option<&Path>,
    artifact_dir: Option<&Path>,
    normalized_output_path: Option<&Path>,
) -> MlflowTrackingPayload {
    let run_name = format!(
        "{}-{}-{}",
        config.run_name_prefix, run.benchmark_id, run.run_id
    );
    MlflowTrackingPayload {
        benchmark_id: run.benchmark_id.clone(),
        benchmark_name: benchmark_name.to_string(),
        run_id: run.run_id.clone(),
        run_name,
        status: run.status.clone(),
        summary: run.summary.clone(),
        runner_kind: run
            .execution
            .as_ref()
            .map(|execution| execution.runner_kind.clone()),
        command: run
            .execution
            .as_ref()
            .map(|execution| execution.command.clone()),
        workdir: run
            .execution
            .as_ref()
            .map(|execution| execution.workdir.clone()),
        scores: run.scores.clone(),
        diagnostics: run.diagnostics.clone(),
        artifacts: resolve_mlflow_artifacts(run, workdir, artifact_dir, normalized_output_path),
    }
}

fn resolve_mlflow_artifacts(
    run: &BenchmarkRun,
    workdir: Option<&Path>,
    artifact_dir: Option<&Path>,
    normalized_output_path: Option<&Path>,
) -> Vec<MlflowTrackingArtifact> {
    let mut artifacts = run
        .artifacts
        .iter()
        .map(|artifact| MlflowTrackingArtifact {
            label: artifact.label.clone(),
            kind: artifact.kind.clone(),
            location: artifact.location.clone(),
            resolved_path: resolve_artifact_path(&artifact.location, workdir, artifact_dir)
                .map(|path| path.display().to_string()),
        })
        .collect::<Vec<_>>();
    if let Some(output_path) = normalized_output_path {
        if output_path.exists() {
            artifacts.push(MlflowTrackingArtifact {
                label: "normalized-result".to_string(),
                kind: "normalized_json".to_string(),
                location: output_path.display().to_string(),
                resolved_path: Some(output_path.display().to_string()),
            });
        }
    }
    artifacts
}

fn resolve_artifact_path(
    location: &str,
    workdir: Option<&Path>,
    artifact_dir: Option<&Path>,
) -> Option<PathBuf> {
    let location_path = PathBuf::from(location);
    let mut candidates = Vec::new();
    if location_path.is_absolute() {
        candidates.push(location_path.clone());
    }
    if let Some(artifact_dir) = artifact_dir {
        candidates.push(artifact_dir.join(location));
        if let Some(stripped) = location.strip_prefix("artifacts/") {
            candidates.push(artifact_dir.join(stripped));
        }
    }
    if let Some(workdir) = workdir {
        candidates.push(workdir.join(location));
    }
    candidates.into_iter().find(|candidate| candidate.exists())
}

fn attach_mlflow_payload_artifact(run: &mut BenchmarkRun, payload_path: &Path) {
    let payload_location = payload_path.display().to_string();
    if run
        .artifacts
        .iter()
        .any(|artifact| artifact.location == payload_location)
    {
        return;
    }
    run.artifacts.push(BenchmarkArtifact {
        label: "mlflow-payload".to_string(),
        kind: "mlflow_payload".to_string(),
        location: payload_location,
    });
}

fn write_mlflow_payload(
    config: &MlflowTrackingConfig,
    payload: &MlflowTrackingPayload,
) -> Result<PathBuf> {
    fs::create_dir_all(&config.payload_dir).with_context(|| {
        format!(
            "Failed to create MLflow payload directory {}",
            config.payload_dir.display()
        )
    })?;
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let filename = format!(
        "{}-{}-{}-{}.json",
        slug(&payload.benchmark_id),
        slug(&payload.run_id),
        timestamp,
        std::process::id()
    );
    let path = config.payload_dir.join(filename);
    let raw =
        serde_json::to_string_pretty(payload).context("Failed to serialize MLflow payload")?;
    fs::write(&path, raw)
        .with_context(|| format!("Failed to write MLflow payload {}", path.display()))?;
    Ok(path)
}

fn log_mlflow_payload(config: &MlflowTrackingConfig, payload_path: &Path) -> Result<()> {
    let output = Command::new(&config.python_bin)
        .arg("-c")
        .arg(MLFLOW_LOG_SCRIPT)
        .arg(payload_path)
        .env("LITKG_MLFLOW_TRACKING_URI", &config.tracking_uri)
        .env("LITKG_MLFLOW_EXPERIMENT", &config.experiment_name)
        .output()
        .with_context(|| {
            format!(
                "Failed to launch `{}` for MLflow tracking",
                config.python_bin
            )
        })?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let mut details = Vec::new();
    if !stderr.is_empty() {
        details.push(format!("stderr: {}", truncate(&stderr)));
    }
    if !stdout.is_empty() {
        details.push(format!("stdout: {}", truncate(&stdout)));
    }
    if details.is_empty() {
        details.push("No output from MLflow logging process.".to_string());
    }
    bail!(
        "MLflow logger exited with status {} ({})",
        output.status,
        details.join("; ")
    )
}

fn mlflow_tracking_from_env() -> Option<MlflowTrackingConfig> {
    let explicit_opt_in = env_flag("LITKG_BENCHMARK_ENABLE_MLFLOW");
    if matches!(explicit_opt_in, Some(false)) {
        return None;
    }
    let env_opt_in = [
        "LITKG_BENCHMARK_MLFLOW_TRACKING_URI",
        "LITKG_BENCHMARK_MLFLOW_EXPERIMENT",
        "LITKG_BENCHMARK_MLFLOW_PAYLOAD_DIR",
        "LITKG_BENCHMARK_MLFLOW_PYTHON",
    ]
    .iter()
    .any(|name| env_nonempty(name).is_some());
    if !explicit_opt_in.unwrap_or(false) && !env_opt_in {
        return None;
    }

    let tracking_uri = env_nonempty("LITKG_BENCHMARK_MLFLOW_TRACKING_URI").unwrap_or_else(|| {
        let root = env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(".data/mlflow");
        format!("file://{}", root.display())
    });
    let experiment_name = env_nonempty("LITKG_BENCHMARK_MLFLOW_EXPERIMENT")
        .unwrap_or_else(|| "litkg-benchmarks".to_string());
    let run_name_prefix = env_nonempty("LITKG_BENCHMARK_MLFLOW_RUN_PREFIX")
        .unwrap_or_else(|| "litkg-benchmark".to_string());
    let payload_dir = env_nonempty("LITKG_BENCHMARK_MLFLOW_PAYLOAD_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(".data/benchmark-tracking/mlflow")
        });
    let python_bin =
        env_nonempty("LITKG_BENCHMARK_MLFLOW_PYTHON").unwrap_or_else(|| "python3".to_string());
    Some(MlflowTrackingConfig {
        python_bin,
        tracking_uri,
        experiment_name,
        run_name_prefix,
        payload_dir,
    })
}

fn env_nonempty(name: &str) -> Option<String> {
    env::var(name).ok().and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn env_flag(name: &str) -> Option<bool> {
    env_nonempty(name).and_then(|value| match value.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    })
}

fn slug(value: &str) -> String {
    let slug = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    if slug.is_empty() {
        "run".to_string()
    } else {
        slug
    }
}

fn select_benchmarks<'a>(
    catalog: &'a BenchmarkCatalog,
    benchmark_ids: &[String],
) -> Result<Vec<&'a crate::benchmark::BenchmarkSpec>> {
    if benchmark_ids.is_empty() {
        return Ok(catalog.benchmarks.iter().collect());
    }

    let selected = benchmark_ids
        .iter()
        .map(|benchmark_id| {
            catalog
                .benchmarks
                .iter()
                .find(|benchmark| benchmark.id == *benchmark_id)
                .with_context(|| format!("Unknown benchmark `{benchmark_id}`"))
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(selected)
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
        return Path::new(binary).exists();
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
    use crate::benchmark::{BenchmarkCatalog, BenchmarkMetric, BenchmarkSource, BenchmarkSpec};
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn set_env_var(name: &str, value: Option<&str>) -> Option<String> {
        let previous = env::var(name).ok();
        match value {
            Some(value) => env::set_var(name, value),
            None => env::remove_var(name),
        }
        previous
    }

    fn restore_env_var(name: &str, value: Option<String>) {
        match value {
            Some(value) => env::set_var(name, value),
            None => env::remove_var(name),
        }
    }

    fn sample_catalog() -> BenchmarkCatalog {
        BenchmarkCatalog {
            benchmarks: vec![
                BenchmarkSpec {
                    id: "terminal-bench".into(),
                    name: "Terminal-Bench".into(),
                    best_use: "CLI agents".into(),
                    task_scale: "89 tasks".into(),
                    summary: "Terminal evaluation".into(),
                    dataset_notes: Vec::new(),
                    metrics: vec![BenchmarkMetric {
                        id: "task_resolution_rate".into(),
                        label: "Task Resolution Rate".into(),
                        notes: "Primary benchmark metric".into(),
                    }],
                    tags: vec!["terminal".into()],
                    sources: vec![BenchmarkSource {
                        label: "github".into(),
                        url: "https://github.com/harbor-framework/terminal-bench".into(),
                    }],
                },
                BenchmarkSpec {
                    id: "swe-qa-pro".into(),
                    name: "SWE-QA-Pro".into(),
                    best_use: "Repo QA".into(),
                    task_scale: "260 questions".into(),
                    summary: "Repository QA".into(),
                    dataset_notes: Vec::new(),
                    metrics: vec![BenchmarkMetric {
                        id: "correctness".into(),
                        label: "Correctness".into(),
                        notes: "Judge score".into(),
                    }],
                    tags: vec!["qa".into()],
                    sources: vec![BenchmarkSource {
                        label: "github".into(),
                        url: "https://github.com/TIGER-AI-Lab/SWE-QA-Pro".into(),
                    }],
                },
            ],
            autoresearch_components: Vec::new(),
            autoresearch_targets: Vec::new(),
        }
    }

    fn sample_integrations() -> BenchmarkIntegrationCatalog {
        BenchmarkIntegrationCatalog {
            integrations: vec![
                BenchmarkIntegration {
                    benchmark_id: "terminal-bench".into(),
                    upstream_status: "official_harness".into(),
                    runner_kind: "external_command".into(),
                    summary: "Public pip package and CLI".into(),
                    official_sources: vec![BenchmarkSource {
                        label: "github".into(),
                        url: "https://github.com/harbor-framework/terminal-bench".into(),
                    }],
                    required_binaries: vec!["sh".into()],
                    required_env_vars: Vec::new(),
                    bootstrap_steps: Vec::new(),
                    example_commands: Vec::new(),
                    notes: Vec::new(),
                },
                BenchmarkIntegration {
                    benchmark_id: "swe-qa-pro".into(),
                    upstream_status: "dataset_only".into(),
                    runner_kind: "external_command".into(),
                    summary: "Dataset available; evaluation code pending".into(),
                    official_sources: vec![BenchmarkSource {
                        label: "github".into(),
                        url: "https://github.com/TIGER-AI-Lab/SWE-QA-Pro".into(),
                    }],
                    required_binaries: vec!["sh".into()],
                    required_env_vars: Vec::new(),
                    bootstrap_steps: Vec::new(),
                    example_commands: Vec::new(),
                    notes: Vec::new(),
                },
            ],
        }
    }

    #[test]
    fn validates_integration_coverage() {
        validate_benchmark_integrations(&sample_catalog(), &sample_integrations()).unwrap();
    }

    #[test]
    fn inspects_local_support_state() {
        let statuses = inspect_benchmark_support(
            &sample_catalog(),
            &sample_integrations(),
            Some(&BenchmarkRunPlan {
                runs: vec![BenchmarkRunRequest {
                    benchmark_id: "terminal-bench".into(),
                    run_id: "terminal-local".into(),
                    command: "printf ok".into(),
                    workdir: None,
                    env: BTreeMap::new(),
                }],
            }),
            &[],
        )
        .unwrap();

        assert_eq!(statuses.len(), 2);
        assert_eq!(statuses[0].benchmark_id, "terminal-bench");
        assert_eq!(statuses[0].local_status, "ready");
        assert_eq!(statuses[1].benchmark_id, "swe-qa-pro");
        assert_eq!(statuses[1].local_status, "not_configured");
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
  "status": "completed",
  "summary": "Terminal benchmark completed.",
  "scores": [
    {
      "metric_id": "task_resolution_rate",
      "value": 0.42,
      "unit": "rate"
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
                    benchmark_id: "terminal-bench".into(),
                    run_id: "terminal-local".into(),
                    command: script_path.display().to_string(),
                    workdir: Some(dir.path().display().to_string()),
                    env: BTreeMap::new(),
                }],
            }),
            &["terminal-bench".into()],
        )
        .unwrap();

        assert_eq!(results.runs.len(), 1);
        assert_eq!(results.runs[0].status, "completed");
        assert_eq!(results.runs[0].scores[0].metric_id, "task_resolution_rate");
        assert_eq!(
            results.runs[0].execution.as_ref().unwrap().runner_kind,
            "external_command"
        );
    }

    #[test]
    fn mlflow_tracking_is_disabled_without_opt_in() {
        let _guard = env_lock().lock().unwrap();
        let previous_enable = set_env_var("LITKG_BENCHMARK_ENABLE_MLFLOW", Some("0"));
        let previous_uri = set_env_var("LITKG_BENCHMARK_MLFLOW_TRACKING_URI", None);
        let previous_experiment = set_env_var("LITKG_BENCHMARK_MLFLOW_EXPERIMENT", None);
        let previous_payload_dir = set_env_var("LITKG_BENCHMARK_MLFLOW_PAYLOAD_DIR", None);
        let previous_python = set_env_var("LITKG_BENCHMARK_MLFLOW_PYTHON", None);

        let config = mlflow_tracking_from_env();

        restore_env_var("LITKG_BENCHMARK_MLFLOW_PYTHON", previous_python);
        restore_env_var("LITKG_BENCHMARK_MLFLOW_PAYLOAD_DIR", previous_payload_dir);
        restore_env_var("LITKG_BENCHMARK_MLFLOW_EXPERIMENT", previous_experiment);
        restore_env_var("LITKG_BENCHMARK_MLFLOW_TRACKING_URI", previous_uri);
        restore_env_var("LITKG_BENCHMARK_ENABLE_MLFLOW", previous_enable);

        assert!(config.is_none());
    }

    #[test]
    fn mlflow_tracking_failure_does_not_fail_run() {
        let _guard = env_lock().lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let script_path = dir.path().join("emit-result.sh");
        fs::write(
            &script_path,
            r#"#!/bin/sh
cat > "$LITKG_BENCHMARK_OUTPUT_PATH" <<'EOF'
{
  "status": "completed",
  "summary": "Terminal benchmark completed.",
  "scores": [
    {
      "metric_id": "task_resolution_rate",
      "value": 0.55,
      "unit": "rate"
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

        let payload_dir = dir.path().join("mlflow-payloads");
        let previous_enable = set_env_var("LITKG_BENCHMARK_ENABLE_MLFLOW", Some("1"));
        let previous_python = set_env_var(
            "LITKG_BENCHMARK_MLFLOW_PYTHON",
            Some("missing-python-for-mlflow"),
        );
        let previous_payload_dir = set_env_var(
            "LITKG_BENCHMARK_MLFLOW_PAYLOAD_DIR",
            Some(payload_dir.display().to_string().as_str()),
        );
        let previous_uri = set_env_var(
            "LITKG_BENCHMARK_MLFLOW_TRACKING_URI",
            Some("file:///tmp/litkg-mlflow-tests"),
        );
        let previous_experiment =
            set_env_var("LITKG_BENCHMARK_MLFLOW_EXPERIMENT", Some("litkg-tests"));

        let results = run_benchmarks(
            &sample_catalog(),
            &sample_integrations(),
            Some(&BenchmarkRunPlan {
                runs: vec![BenchmarkRunRequest {
                    benchmark_id: "terminal-bench".into(),
                    run_id: "terminal-mlflow".into(),
                    command: script_path.display().to_string(),
                    workdir: Some(dir.path().display().to_string()),
                    env: BTreeMap::new(),
                }],
            }),
            &["terminal-bench".into()],
        )
        .unwrap();

        restore_env_var("LITKG_BENCHMARK_MLFLOW_EXPERIMENT", previous_experiment);
        restore_env_var("LITKG_BENCHMARK_MLFLOW_TRACKING_URI", previous_uri);
        restore_env_var("LITKG_BENCHMARK_MLFLOW_PAYLOAD_DIR", previous_payload_dir);
        restore_env_var("LITKG_BENCHMARK_MLFLOW_PYTHON", previous_python);
        restore_env_var("LITKG_BENCHMARK_ENABLE_MLFLOW", previous_enable);

        assert_eq!(results.runs.len(), 1);
        assert_eq!(results.runs[0].status, "completed");
        assert!(results.runs[0]
            .diagnostics
            .iter()
            .any(|line| line.contains("MLflow tracking failed")));
        assert!(results.runs[0]
            .artifacts
            .iter()
            .any(|artifact| artifact.kind == "mlflow_payload"));
        assert!(payload_dir.exists());
        assert!(fs::read_dir(&payload_dir).unwrap().next().is_some());
    }
}
