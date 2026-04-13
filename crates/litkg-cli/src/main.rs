use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use litkg_core::{
    download_registry_sources, inspect_benchmark_support, load_registry, parse_registry_papers,
    promote_benchmark_results, render_promoted_targets, run_benchmarks, sync_registry,
    validate_benchmark_catalog, validate_benchmark_results, write_benchmark_results,
    write_parsed_papers, AutoResearchRenderFormat, BenchmarkCatalog, BenchmarkPromotionRequest,
    BenchmarkResults, BenchmarkSupportStatus, DownloadOptions, MetricThresholdComparison,
    MetricThresholdRule, PromotionComponentSelection, RepoConfig, SinkMode,
};
use litkg_graphify::GraphifySink;
use litkg_neo4j::Neo4jSink;
use litkg_viewer::run_bundle as run_viewer_bundle;
use std::process::Command;

#[derive(Parser)]
#[command(name = "litkg")]
#[command(about = "Repo-independent literature download and graph materialization toolkit.")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    SyncRegistry(ConfigArg),
    Download(DownloadCommand),
    Parse(ConfigArg),
    Materialize(ConfigArg),
    RebuildGraph(ConfigArg),
    Pipeline(DownloadCommand),
    ExportNeo4j(ConfigArg),
    InspectGraph(ConfigArg),
    ValidateBenchmarks(BenchmarkCatalogArg),
    BenchmarkSupport(BenchmarkSupportCommand),
    RunBenchmarks(BenchmarkRunCommand),
    RenderAutoresearchTarget(AutoResearchTargetCommand),
    PromoteBenchmarkResults(PromoteBenchmarkResultsCommand),
    SyncAutoresearchTargetIssue(AutoResearchIssueSyncCommand),
}

#[derive(Args, Clone)]
struct ConfigArg {
    #[arg(long)]
    config: String,
}

#[derive(Args, Clone)]
struct DownloadCommand {
    #[command(flatten)]
    config: ConfigArg,
    #[arg(long, default_value_t = false)]
    overwrite: bool,
    #[arg(long, default_value_t = false)]
    download_pdfs: bool,
}

#[derive(Args, Clone)]
struct BenchmarkCatalogArg {
    #[arg(long)]
    catalog: String,
    #[arg(long)]
    results: Option<String>,
}

#[derive(Args, Clone)]
struct AutoResearchTargetCommand {
    #[command(flatten)]
    catalog: BenchmarkCatalogArg,
    #[arg(long = "target-id")]
    target_id: String,
    #[arg(long = "component-id")]
    component_ids: Vec<String>,
    #[arg(long = "benchmark-id")]
    benchmark_ids: Vec<String>,
    #[arg(long, value_enum, default_value_t = AutoResearchTargetFormatArg::Markdown)]
    format: AutoResearchTargetFormatArg,
}

#[derive(Args, Clone)]
struct AutoResearchIssueSyncCommand {
    #[command(flatten)]
    catalog: BenchmarkCatalogArg,
    #[arg(long = "target-id")]
    target_id: String,
    #[arg(long = "component-id")]
    component_ids: Vec<String>,
    #[arg(long = "benchmark-id")]
    benchmark_ids: Vec<String>,
    #[arg(long)]
    repo: Option<String>,
    #[arg(long)]
    title: Option<String>,
    #[arg(long = "label")]
    labels: Vec<String>,
    #[arg(long, default_value_t = false)]
    dry_run: bool,
}

#[derive(Args, Clone)]
struct BenchmarkExecutionArgs {
    #[arg(long)]
    catalog: String,
    #[arg(long)]
    integrations: String,
    #[arg(long)]
    plan: Option<String>,
    #[arg(long = "benchmark-id")]
    benchmark_ids: Vec<String>,
}

#[derive(Args, Clone)]
struct BenchmarkSupportCommand {
    #[command(flatten)]
    execution: BenchmarkExecutionArgs,
    #[arg(long, default_value = "text")]
    format: String,
}

#[derive(Args, Clone)]
struct BenchmarkRunCommand {
    #[command(flatten)]
    execution: BenchmarkExecutionArgs,
    #[arg(long)]
    output: String,
}

#[derive(Args, Clone)]
struct PromoteBenchmarkResultsCommand {
    #[command(flatten)]
    catalog: BenchmarkCatalogArg,
    #[arg(long = "target-id")]
    target_ids: Vec<String>,
    #[arg(long = "benchmark-id")]
    benchmark_ids: Vec<String>,
    #[arg(long = "component-id")]
    component_ids: Vec<String>,
    #[arg(long = "status")]
    status_filters: Vec<String>,
    #[arg(long = "metric-threshold")]
    metric_thresholds: Vec<String>,
    #[arg(long, default_value = "template-only")]
    component_selection: String,
    #[arg(long, default_value = "markdown")]
    format: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum AutoResearchTargetFormatArg {
    Markdown,
    #[value(alias = "github-issue")]
    Issue,
    Json,
}

impl From<AutoResearchTargetFormatArg> for AutoResearchRenderFormat {
    fn from(value: AutoResearchTargetFormatArg) -> Self {
        match value {
            AutoResearchTargetFormatArg::Markdown => AutoResearchRenderFormat::Markdown,
            AutoResearchTargetFormatArg::Issue => AutoResearchRenderFormat::GitHubIssue,
            AutoResearchTargetFormatArg::Json => AutoResearchRenderFormat::Json,
        }
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::SyncRegistry(args) => {
            let config = RepoConfig::load(&args.config)?;
            let registry = sync_registry(&config)?;
            println!(
                "Synced {} registry records into {}",
                registry.len(),
                config.registry_path().display()
            );
        }
        Commands::Download(args) => {
            let config = RepoConfig::load(&args.config.config)?;
            let registry = if config.registry_path().exists() {
                load_registry(config.registry_path())?
            } else {
                sync_registry(&config)?
            };
            let updated = download_registry_sources(
                &config,
                &registry,
                DownloadOptions {
                    overwrite: args.overwrite,
                    download_pdfs: args.download_pdfs,
                },
            )?;
            litkg_core::write_registry(&config.registry_path(), &updated)?;
            println!("Downloaded literature assets for {} records", updated.len());
        }
        Commands::Parse(args) => {
            let config = RepoConfig::load(&args.config)?;
            let registry = if config.registry_path().exists() {
                load_registry(config.registry_path())?
            } else {
                sync_registry(&config)?
            };
            let papers = parse_registry_papers(&config, &registry)?;
            write_parsed_papers(config.parsed_root(), &papers)?;
            let updated_registry = papers
                .iter()
                .map(|paper| paper.metadata.clone())
                .collect::<Vec<_>>();
            litkg_core::write_registry(&config.registry_path(), &updated_registry)?;
            println!(
                "Parsed {} papers into {}",
                papers.len(),
                config.parsed_root().display()
            );
        }
        Commands::Materialize(args) => {
            let config = RepoConfig::load(&args.config)?;
            let papers = litkg_core::load_parsed_papers(config.parsed_root())?;
            if papers.is_empty() {
                anyhow::bail!(
                    "No parsed papers found under {}",
                    config.parsed_root().display()
                );
            }
            materialize(&config, &papers)?;
        }
        Commands::RebuildGraph(args) => {
            let config = RepoConfig::load(&args.config)?;
            rebuild_graph(&config)?;
        }
        Commands::Pipeline(args) => {
            let config = RepoConfig::load(&args.config.config)?;
            let registry = sync_registry(&config)?;
            let updated = download_registry_sources(
                &config,
                &registry,
                DownloadOptions {
                    overwrite: args.overwrite,
                    download_pdfs: args.download_pdfs,
                },
            )?;
            litkg_core::write_registry(&config.registry_path(), &updated)?;
            let papers = parse_registry_papers(&config, &updated)?;
            write_parsed_papers(config.parsed_root(), &papers)?;
            let updated_registry = papers
                .iter()
                .map(|paper| paper.metadata.clone())
                .collect::<Vec<_>>();
            litkg_core::write_registry(&config.registry_path(), &updated_registry)?;
            materialize(&config, &papers)?;
            if matches!(config.sink, SinkMode::Graphify | SinkMode::Both) {
                rebuild_graph(&config)?;
            }
        }
        Commands::ExportNeo4j(args) => {
            let config = RepoConfig::load(&args.config)?;
            let papers = litkg_core::load_parsed_papers(config.parsed_root())?;
            if papers.is_empty() {
                anyhow::bail!(
                    "No parsed papers found under {}",
                    config.parsed_root().display()
                );
            }
            let written = Neo4jSink::export(&config, &papers)?;
            println!(
                "Wrote Neo4j export bundle:\n{}",
                written
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>()
                    .join("\n")
            );
        }
        Commands::InspectGraph(args) => {
            let config = RepoConfig::load(&args.config)?;
            inspect_graph(&config)?;
        }
        Commands::ValidateBenchmarks(args) => {
            let catalog = litkg_core::load_benchmark_catalog(&args.catalog)?;
            let mut summary = validate_benchmark_catalog(&catalog)?;
            if let Some(results_path) = &args.results {
                let results = litkg_core::load_benchmark_results(results_path)?;
                summary = validate_benchmark_results(&catalog, &results)?;
            }
            println!(
                "Validated benchmark catalog: {} benchmarks, {} metrics, {} autoresearch components, {} autoresearch targets, {} benchmark runs",
                summary.benchmark_count,
                summary.metric_count,
                summary.component_count,
                summary.target_count,
                summary.run_count,
            );
        }
        Commands::BenchmarkSupport(args) => {
            let catalog = litkg_core::load_benchmark_catalog(&args.execution.catalog)?;
            let integrations =
                litkg_core::load_benchmark_integrations(&args.execution.integrations)?;
            let plan = match &args.execution.plan {
                Some(path) => Some(litkg_core::load_benchmark_run_plan(path)?),
                None => None,
            };
            let statuses = inspect_benchmark_support(
                &catalog,
                &integrations,
                plan.as_ref(),
                &args.execution.benchmark_ids,
            )?;
            match args.format.as_str() {
                "text" => println!("{}", render_support_statuses(&statuses)),
                "json" => println!("{}", serde_json::to_string_pretty(&statuses)?),
                other => anyhow::bail!("Unsupported benchmark support format `{other}`"),
            }
        }
        Commands::RunBenchmarks(args) => {
            let catalog = litkg_core::load_benchmark_catalog(&args.execution.catalog)?;
            let integrations =
                litkg_core::load_benchmark_integrations(&args.execution.integrations)?;
            let plan = match &args.execution.plan {
                Some(path) => Some(litkg_core::load_benchmark_run_plan(path)?),
                None => None,
            };
            let results = run_benchmarks(
                &catalog,
                &integrations,
                plan.as_ref(),
                &args.execution.benchmark_ids,
            )?;
            write_benchmark_results(&args.output, &results)?;
            let summary = validate_benchmark_results(&catalog, &results)?;
            println!(
                "Ran benchmark integrations: {} benchmarks, {} runs written to {}",
                summary.benchmark_count, summary.run_count, args.output,
            );
        }
        Commands::RenderAutoresearchTarget(args) => {
            let (catalog, results) = load_benchmark_inputs(&args.catalog)?;
            let rendered = litkg_core::render_autoresearch_target(
                &catalog,
                results.as_ref(),
                &args.target_id,
                &args.component_ids,
                &args.benchmark_ids,
                args.format.into(),
            )?;
            println!("{rendered}");
        }
        Commands::PromoteBenchmarkResults(args) => {
            let catalog = litkg_core::load_benchmark_catalog(&args.catalog.catalog)?;
            let results_path = args
                .catalog
                .results
                .as_ref()
                .context("`promote-benchmark-results` requires --results")?;
            let results = litkg_core::load_benchmark_results(results_path)?;
            let request = BenchmarkPromotionRequest {
                target_ids: args.target_ids,
                benchmark_ids: args.benchmark_ids,
                status_filters: args.status_filters,
                metric_thresholds: args
                    .metric_thresholds
                    .iter()
                    .map(|raw| parse_metric_threshold(raw))
                    .collect::<Result<Vec<_>>>()?,
                component_selection: parse_component_selection(&args.component_selection)?,
                component_ids: args.component_ids,
            };
            let promoted = promote_benchmark_results(&catalog, &results, &request)?;
            let rendered = render_promoted_targets(&promoted, parse_render_format(&args.format)?)?;
            println!("{rendered}");
        }
        Commands::SyncAutoresearchTargetIssue(args) => {
            let (catalog, results) = load_benchmark_inputs(&args.catalog)?;
            let body = litkg_core::render_autoresearch_target(
                &catalog,
                results.as_ref(),
                &args.target_id,
                &args.component_ids,
                &args.benchmark_ids,
                AutoResearchRenderFormat::GitHubIssue,
            )?;
            let title = match &args.title {
                Some(title) => title.clone(),
                None => extract_issue_title(&body)?,
            };
            let repo = match &args.repo {
                Some(repo) => repo.clone(),
                None => infer_github_repo_from_origin()?,
            };

            if args.dry_run {
                println!("Repository: {repo}");
                println!("Title: {title}");
                if !args.labels.is_empty() {
                    println!("Labels: {}", args.labels.join(", "));
                }
                println!();
                println!("{body}");
            } else {
                let issue_url = create_github_issue(&repo, &title, &body, &args.labels)?;
                println!("{issue_url}");
            }
        }
    }
    Ok(())
}

fn load_benchmark_inputs(
    args: &BenchmarkCatalogArg,
) -> Result<(BenchmarkCatalog, Option<BenchmarkResults>)> {
    let catalog = litkg_core::load_benchmark_catalog(&args.catalog)?;
    let results = match &args.results {
        Some(path) => Some(litkg_core::load_benchmark_results(path)?),
        None => None,
    };
    Ok((catalog, results))
}

fn parse_render_format(raw: &str) -> Result<AutoResearchRenderFormat> {
    match raw {
        "markdown" => Ok(AutoResearchRenderFormat::Markdown),
        "json" => Ok(AutoResearchRenderFormat::Json),
        "github-issue" | "issue" => Ok(AutoResearchRenderFormat::GitHubIssue),
        other => anyhow::bail!("Unsupported autoresearch target format `{other}`"),
    }
}

fn parse_component_selection(raw: &str) -> Result<PromotionComponentSelection> {
    match raw {
        "template-only" => Ok(PromotionComponentSelection::TemplateOnly),
        "template-and-matched" => Ok(PromotionComponentSelection::TemplateAndMatched),
        "matched-only" => Ok(PromotionComponentSelection::MatchedOnly),
        other => anyhow::bail!("Unsupported component selection policy `{other}`"),
    }
}

fn parse_metric_threshold(raw: &str) -> Result<MetricThresholdRule> {
    let operators = [
        ("<=", MetricThresholdComparison::LessThanOrEqual),
        (">=", MetricThresholdComparison::GreaterThanOrEqual),
        ("<", MetricThresholdComparison::LessThan),
        (">", MetricThresholdComparison::GreaterThan),
    ];
    for (operator, comparison) in operators {
        if let Some((metric_id, value)) = raw.split_once(operator) {
            let metric_id = metric_id.trim();
            let value = value.trim();
            if metric_id.is_empty() {
                anyhow::bail!("Metric threshold `{raw}` is missing a metric id");
            }
            return Ok(MetricThresholdRule {
                metric_id: metric_id.to_string(),
                comparison,
                value: value.parse::<f64>().with_context(|| {
                    format!("Metric threshold `{raw}` has an invalid numeric value")
                })?,
            });
        }
    }
    anyhow::bail!(
        "Metric threshold `{raw}` must use one of `<`, `<=`, `>`, or `>=`, for example `correctness<0.7`"
    )
}

fn render_support_statuses(statuses: &[BenchmarkSupportStatus]) -> String {
    let mut lines = vec!["Benchmark support snapshot:".to_string()];
    for status in statuses {
        let mut detail = format!(
            "- `{}` [{} / {}] local=`{}` configured_runs={}",
            status.benchmark_id,
            status.upstream_status,
            status.runner_kind,
            status.local_status,
            status.configured_runs
        );
        if !status.missing_binaries.is_empty() {
            detail.push_str(&format!(
                " missing_binaries={}",
                status.missing_binaries.join(",")
            ));
        }
        if !status.missing_env_vars.is_empty() {
            detail.push_str(&format!(
                " missing_env_vars={}",
                status.missing_env_vars.join(",")
            ));
        }
        lines.push(detail);
        lines.push(format!("  {}", status.summary));
    }
    lines.join("\n")
}

fn extract_issue_title(body: &str) -> Result<String> {
    let heading = body
        .lines()
        .find(|line| !line.trim().is_empty())
        .context("Rendered issue body did not contain a heading")?;
    let title = heading
        .strip_prefix("# ")
        .unwrap_or(heading)
        .trim()
        .to_string();
    if title.is_empty() {
        anyhow::bail!("Rendered issue heading was empty");
    }
    Ok(title)
}

fn infer_github_repo_from_origin() -> Result<String> {
    let output = Command::new("git")
        .args(["remote", "get-url", "origin"])
        .output()
        .context("Failed to run `git remote get-url origin`")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Failed to read origin remote: {}", stderr.trim());
    }

    let remote = String::from_utf8(output.stdout)
        .context("Origin remote URL was not valid UTF-8")?
        .trim()
        .to_string();
    parse_github_repo_from_remote_url(&remote).with_context(|| {
        format!(
            "Origin remote `{remote}` is not a supported GitHub remote shape; pass `--repo [HOST/]owner/repo` explicitly"
        )
    })
}

fn parse_github_repo_from_remote_url(remote: &str) -> Option<String> {
    let normalized = remote.trim().trim_end_matches('/').trim_end_matches(".git");

    if let Some((_, remainder)) = normalized.split_once("://") {
        let without_user = remainder
            .rsplit_once('@')
            .map(|(_, value)| value)
            .unwrap_or(remainder);
        let (host, path) = without_user.split_once('/')?;
        return format_gh_repo_locator(host, path);
    }

    let (host_part, path) = normalized.split_once(':')?;
    let host = host_part
        .rsplit_once('@')
        .map(|(_, value)| value)
        .unwrap_or(host_part);
    format_gh_repo_locator(host, path)
}

fn format_gh_repo_locator(host: &str, path: &str) -> Option<String> {
    let host = host.trim();
    let mut segments = path
        .trim_start_matches('/')
        .split('/')
        .filter(|segment| !segment.is_empty());
    let owner = segments.next()?;
    let repo = segments.next()?;
    if segments.next().is_some() {
        return None;
    }

    if host.eq_ignore_ascii_case("github.com") {
        Some(format!("{owner}/{repo}"))
    } else {
        Some(format!("{host}/{owner}/{repo}"))
    }
}

fn create_github_issue(repo: &str, title: &str, body: &str, labels: &[String]) -> Result<String> {
    let mut command = Command::new("gh");
    command
        .arg("issue")
        .arg("create")
        .arg("--repo")
        .arg(repo)
        .arg("--title")
        .arg(title)
        .arg("--body")
        .arg(body);

    for label in labels {
        command.arg("--label").arg(label);
    }

    let output = command
        .output()
        .context("Failed to run `gh issue create`")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("`gh issue create` failed: {}", stderr.trim());
    }

    let stdout =
        String::from_utf8(output.stdout).context("`gh issue create` output was not valid UTF-8")?;
    Ok(stdout.trim().to_string())
}

fn materialize(config: &RepoConfig, papers: &[litkg_core::ParsedPaper]) -> Result<()> {
    match config.sink {
        SinkMode::Graphify => {
            let written = GraphifySink::materialize(config, papers)?;
            println!(
                "Materialized graphify corpus:\n{}",
                written
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>()
                    .join("\n")
            );
        }
        SinkMode::Neo4j => {
            let written = Neo4jSink::export(config, papers)?;
            println!(
                "Materialized Neo4j export bundle:\n{}",
                written
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>()
                    .join("\n")
            );
        }
        SinkMode::Both => {
            let graphify = GraphifySink::materialize(config, papers)?;
            let neo4j = Neo4jSink::export(config, papers)?;
            println!(
                "Materialized graphify corpus and Neo4j export bundle:\n{}",
                graphify
                    .into_iter()
                    .chain(neo4j)
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>()
                    .join("\n")
            );
        }
    }
    Ok(())
}

fn rebuild_graph(config: &RepoConfig) -> Result<()> {
    let command = config
        .graphify_rebuild_command
        .as_ref()
        .context("No graphify rebuild command configured")?;
    let status = Command::new("sh")
        .arg("-c")
        .arg(command)
        .status()
        .with_context(|| format!("Failed to spawn graph rebuild command `{command}`"))?;
    if !status.success() {
        println!("Graph rebuild skipped because `{command}` returned a non-zero status.");
        return Ok(());
    }
    println!("Rebuilt graph with `{command}`");
    Ok(())
}

fn inspect_graph(config: &RepoConfig) -> Result<()> {
    let bundle_root = config.neo4j_export_root();
    let nodes_path = bundle_root.join("nodes.jsonl");
    let edges_path = bundle_root.join("edges.jsonl");

    if !(nodes_path.exists() && edges_path.exists()) {
        let papers = litkg_core::load_parsed_papers(config.parsed_root())?;
        if papers.is_empty() {
            anyhow::bail!(
                "No parsed papers found under {}; run parse/materialize first or export the Neo4j bundle before inspecting.",
                config.parsed_root().display()
            );
        }
        Neo4jSink::export(config, &papers)?;
        println!(
            "Generated Neo4j export bundle under {}",
            bundle_root.display()
        );
    }

    run_viewer_bundle(&bundle_root)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_accepts_issue_render_format() {
        let cli = Cli::try_parse_from([
            "litkg",
            "render-autoresearch-target",
            "--catalog",
            "examples/benchmarks/kg.toml",
            "--results",
            "examples/benchmarks/sample-results.toml",
            "--target-id",
            "kg_navigation_improvement",
            "--format",
            "issue",
        ])
        .unwrap();

        match cli.command {
            Commands::RenderAutoresearchTarget(command) => {
                assert_eq!(command.format, AutoResearchTargetFormatArg::Issue);
            }
            other => panic!(
                "unexpected command parsed: {:?}",
                std::mem::discriminant(&other)
            ),
        }
    }

    #[test]
    fn cli_accepts_github_issue_render_alias() {
        let cli = Cli::try_parse_from([
            "litkg",
            "render-autoresearch-target",
            "--catalog",
            "examples/benchmarks/kg.toml",
            "--results",
            "examples/benchmarks/sample-results.toml",
            "--target-id",
            "kg_navigation_improvement",
            "--format",
            "github-issue",
        ])
        .unwrap();

        match cli.command {
            Commands::RenderAutoresearchTarget(command) => {
                assert_eq!(command.format, AutoResearchTargetFormatArg::Issue);
            }
            other => panic!(
                "unexpected command parsed: {:?}",
                std::mem::discriminant(&other)
            ),
        }
    }

    #[test]
    fn cli_parses_autoresearch_issue_sync_command() {
        let cli = Cli::try_parse_from([
            "litkg",
            "sync-autoresearch-target-issue",
            "--catalog",
            "examples/benchmarks/kg.toml",
            "--results",
            "examples/benchmarks/sample-results.toml",
            "--target-id",
            "kg_navigation_improvement",
            "--label",
            "autoresearch",
            "--dry-run",
        ])
        .unwrap();

        match cli.command {
            Commands::SyncAutoresearchTargetIssue(command) => {
                assert_eq!(command.labels, vec!["autoresearch"]);
                assert!(command.dry_run);
                assert_eq!(command.target_id, "kg_navigation_improvement");
            }
            other => panic!(
                "unexpected command parsed: {:?}",
                std::mem::discriminant(&other)
            ),
        }
    }

    #[test]
    fn cli_parses_benchmark_support_command() {
        let cli = Cli::try_parse_from([
            "litkg",
            "benchmark-support",
            "--catalog",
            "examples/benchmarks/kg.toml",
            "--integrations",
            "examples/benchmarks/integrations.toml",
            "--benchmark-id",
            "swe-qa-pro",
            "--format",
            "json",
        ])
        .unwrap();

        match cli.command {
            Commands::BenchmarkSupport(command) => {
                assert_eq!(command.execution.benchmark_ids, vec!["swe-qa-pro"]);
                assert_eq!(command.format, "json");
            }
            other => panic!(
                "unexpected command parsed: {:?}",
                std::mem::discriminant(&other)
            ),
        }
    }

    #[test]
    fn cli_parses_promote_benchmark_results_command() {
        let cli = Cli::try_parse_from([
            "litkg",
            "promote-benchmark-results",
            "--catalog",
            "examples/benchmarks/kg.toml",
            "--results",
            "examples/benchmarks/sample-results.toml",
            "--target-id",
            "kg_navigation_improvement",
            "--status",
            "error",
            "--metric-threshold",
            "correctness<=0.7",
            "--format",
            "github-issue",
        ])
        .unwrap();

        match cli.command {
            Commands::PromoteBenchmarkResults(command) => {
                assert_eq!(command.target_ids, vec!["kg_navigation_improvement"]);
                assert_eq!(command.status_filters, vec!["error"]);
                assert_eq!(command.metric_thresholds, vec!["correctness<=0.7"]);
                assert_eq!(command.format, "github-issue");
            }
            other => panic!(
                "unexpected command parsed: {:?}",
                std::mem::discriminant(&other)
            ),
        }
    }

    #[test]
    fn extracts_issue_title_from_heading() {
        let title = extract_issue_title("# Autoresearch Target: KG navigation\n\nBody").unwrap();
        assert_eq!(title, "Autoresearch Target: KG navigation");
    }

    #[test]
    fn parses_github_repo_from_origin_urls() {
        assert_eq!(
            parse_github_repo_from_remote_url("git@github.com:owner/repo.git"),
            Some("owner/repo".to_string())
        );
        assert_eq!(
            parse_github_repo_from_remote_url("https://github.com/owner/repo.git"),
            Some("owner/repo".to_string())
        );
        assert_eq!(
            parse_github_repo_from_remote_url("ssh://git@github.com/owner/repo.git"),
            Some("owner/repo".to_string())
        );
        assert_eq!(
            parse_github_repo_from_remote_url("git@github.example.com:owner/repo.git"),
            Some("github.example.com/owner/repo".to_string())
        );
        assert_eq!(
            parse_github_repo_from_remote_url("https://github.example.com/owner/repo.git"),
            Some("github.example.com/owner/repo".to_string())
        );
    }
}
