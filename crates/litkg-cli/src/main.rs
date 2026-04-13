use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use litkg_core::{
    download_registry_sources, load_registry, parse_registry_papers, sync_registry,
    validate_benchmark_catalog, validate_benchmark_results, write_parsed_papers,
    AutoResearchRenderFormat, BenchmarkResults, DownloadOptions, RepoConfig, SinkMode,
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
    RenderAutoresearchTarget(AutoResearchTargetCommand),
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
        Commands::RenderAutoresearchTarget(args) => {
            let catalog = litkg_core::load_benchmark_catalog(&args.catalog.catalog)?;
            let results: Option<BenchmarkResults> = match &args.catalog.results {
                Some(path) => Some(litkg_core::load_benchmark_results(path)?),
                None => None,
            };
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
    }
    Ok(())
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
