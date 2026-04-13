use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use litkg_core::{
    build_registry_snapshot, compute_corpus_stats, download_registry_sources, inspect_paper,
    load_parsed_papers, load_registry, parse_registry_papers, search_papers, sync_registry,
    validate_benchmark_catalog, validate_benchmark_results, write_parsed_papers,
    AutoResearchRenderFormat, BenchmarkResults, CorpusStats, DownloadOptions, PaperInspection,
    PaperSourceRecord, RepoConfig, SearchResults, SinkMode,
};
use litkg_graphify::GraphifySink;
use litkg_neo4j::Neo4jSink;
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
    Stats(StatsCommand),
    Search(SearchCommand),
    ShowPaper(ShowPaperCommand),
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

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum OutputFormat {
    Text,
    Json,
}

#[derive(Args, Clone)]
struct StatsCommand {
    #[command(flatten)]
    config: ConfigArg,
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    format: OutputFormat,
}

#[derive(Args, Clone)]
struct SearchCommand {
    #[command(flatten)]
    config: ConfigArg,
    #[arg(long)]
    query: String,
    #[arg(long, default_value_t = 10)]
    limit: usize,
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    format: OutputFormat,
}

#[derive(Args, Clone)]
struct ShowPaperCommand {
    #[command(flatten)]
    config: ConfigArg,
    #[arg(long = "paper")]
    paper_selector: String,
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    format: OutputFormat,
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
    #[arg(long, default_value = "markdown")]
    format: String,
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
            let papers = load_parsed_papers(config.parsed_root())?;
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
        Commands::Stats(args) => {
            let config = RepoConfig::load(&args.config.config)?;
            let registry = load_registry_or_sync(&config)?;
            let papers = load_parsed_papers(config.parsed_root())?;
            let stats = compute_corpus_stats(&registry, &papers);
            print_structured_output(&stats, args.format, render_stats)?;
        }
        Commands::Search(args) => {
            if args.limit == 0 {
                anyhow::bail!("--limit must be at least 1");
            }
            let config = RepoConfig::load(&args.config.config)?;
            let registry = load_registry_or_sync(&config)?;
            let papers = load_parsed_papers(config.parsed_root())?;
            let hits = search_papers(
                &registry,
                &papers,
                &config.relevance_tags,
                &args.query,
                args.limit,
            )?;
            print_structured_output(&hits, args.format, render_search_results)?;
        }
        Commands::ShowPaper(args) => {
            let config = RepoConfig::load(&args.config.config)?;
            let registry = load_registry_or_sync(&config)?;
            let papers = load_parsed_papers(config.parsed_root())?;
            let inspection = inspect_paper(&config, &registry, &papers, &args.paper_selector)?;
            print_structured_output(&inspection, args.format, render_paper_inspection)?;
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
            let format = match args.format.as_str() {
                "markdown" => AutoResearchRenderFormat::Markdown,
                "json" => AutoResearchRenderFormat::Json,
                other => anyhow::bail!("Unsupported autoresearch target format `{other}`"),
            };
            let rendered = litkg_core::render_autoresearch_target(
                &catalog,
                results.as_ref(),
                &args.target_id,
                &args.component_ids,
                &args.benchmark_ids,
                format,
            )?;
            println!("{rendered}");
        }
    }
    Ok(())
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

fn load_registry_or_sync(config: &RepoConfig) -> Result<Vec<PaperSourceRecord>> {
    if config.registry_path().exists() {
        load_registry(config.registry_path())
    } else {
        build_registry_snapshot(config)
    }
}

fn print_structured_output<T>(
    value: &T,
    format: OutputFormat,
    render_text: impl FnOnce(&T) -> String,
) -> Result<()>
where
    T: serde::Serialize,
{
    match format {
        OutputFormat::Text => println!("{}", render_text(value)),
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(value)?),
    }
    Ok(())
}

fn render_stats(stats: &CorpusStats) -> String {
    let mut lines = vec![
        "litkg corpus stats".to_string(),
        String::new(),
        format!("papers: {}", stats.total_papers),
        format!("parsed_papers: {}", stats.papers_with_parsed_content),
        format!("local_tex: {}", stats.papers_with_local_tex),
        format!("local_pdf: {}", stats.papers_with_local_pdf),
        format!("sections: {}", stats.total_sections),
        format!("figures: {}", stats.total_figures),
        format!("tables: {}", stats.total_tables),
        format!("citations: {}", stats.total_citations),
        String::new(),
        "source_kinds:".to_string(),
    ];
    lines.extend(render_count_map(&stats.source_kind_counts));
    lines.push(String::new());
    lines.push("download_modes:".to_string());
    lines.extend(render_count_map(&stats.download_mode_counts));
    lines.push(String::new());
    lines.push("parse_statuses:".to_string());
    lines.extend(render_count_map(&stats.parse_status_counts));
    lines.join("\n")
}

fn render_search_results(results: &SearchResults) -> String {
    if results.hits.is_empty() {
        return format!("No papers matched query `{}`.", results.query);
    }

    let mut lines = vec![
        format!("Search results for `{}`", results.query),
        String::new(),
        format!(
            "showing: {} of {}",
            results.hits.len(),
            results.total_matches
        ),
    ];
    if results.has_more {
        lines.push(format!("limit: {}", results.limit));
    }

    for (index, hit) in results.hits.iter().enumerate() {
        lines.push(String::new());
        lines.push(format!("{}. {} ({})", index + 1, hit.title, hit.paper_id));
        lines.push(format!(
            "   citation_key: {}",
            hit.citation_key.as_deref().unwrap_or("n/a")
        ));
        lines.push(format!(
            "   year: {} | parse_status: {:?} | score: {}",
            hit.year.as_deref().unwrap_or("n/a"),
            hit.parse_status,
            hit.score
        ));
        lines.push(format!(
            "   local_assets: tex={} pdf={}",
            yes_no(hit.has_local_tex),
            yes_no(hit.has_local_pdf)
        ));
        lines.push(format!(
            "   matched_fields: {}",
            hit.matched_fields.join(", ")
        ));
        if !hit.relevance_tags.is_empty() {
            lines.push(format!(
                "   relevance_tags: {}",
                hit.relevance_tags.join(", ")
            ));
        }
        if let Some(snippet) = &hit.snippet {
            lines.push(format!("   snippet: {snippet}"));
        }
    }

    lines.join("\n")
}

fn render_paper_inspection(inspection: &PaperInspection) -> String {
    let mut lines = vec![
        format!(
            "{} ({})",
            inspection.metadata.title, inspection.metadata.paper_id
        ),
        String::new(),
        format!(
            "citation_key: {}",
            inspection.metadata.citation_key.as_deref().unwrap_or("n/a")
        ),
        format!(
            "arxiv_id: {}",
            inspection.metadata.arxiv_id.as_deref().unwrap_or("n/a")
        ),
        format!(
            "year: {}",
            inspection.metadata.year.as_deref().unwrap_or("n/a")
        ),
        format!("source_kind: {:?}", inspection.metadata.source_kind),
        format!("download_mode: {:?}", inspection.metadata.download_mode),
        format!("parse_status: {:?}", inspection.metadata.parse_status),
        format!(
            "authors: {}",
            if inspection.metadata.authors.is_empty() {
                "n/a".to_string()
            } else {
                inspection.metadata.authors.join(", ")
            }
        ),
        String::new(),
        "paths:".to_string(),
        format!("  parsed_json: {}", inspection.parsed_json_path.display()),
        format!(
            "  materialized_markdown: {}",
            inspection
                .materialized_markdown_path
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "n/a".to_string())
        ),
        format!(
            "  local_tex_dir: {}",
            inspection
                .local_tex_dir
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "n/a".to_string())
        ),
        format!(
            "  local_pdf_path: {}",
            inspection
                .local_pdf_path
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "n/a".to_string())
        ),
        String::new(),
        format!(
            "abstract: {}",
            inspection
                .abstract_text
                .as_deref()
                .unwrap_or("No abstract was extracted.")
        ),
        String::new(),
        format!("sections: {}", inspection.sections.len()),
    ];

    if inspection.sections.is_empty() {
        lines.push("  - none".to_string());
    } else {
        for section in &inspection.sections {
            lines.push(format!("  - L{} {}", section.level, section.title));
        }
    }

    lines.push(String::new());
    lines.push(format!("citations: {}", inspection.citations.len()));
    if inspection.citations.is_empty() {
        lines.push("  - none".to_string());
    } else {
        for citation in &inspection.citations {
            lines.push(format!("  - {}", citation));
        }
    }

    lines.push(String::new());
    lines.push(format!("cited_by: {}", inspection.cited_by.len()));
    if inspection.cited_by.is_empty() {
        lines.push("  - none".to_string());
    } else {
        for incoming in &inspection.cited_by {
            lines.push(format!("  - {} ({})", incoming.title, incoming.paper_id));
        }
    }

    lines.push(String::new());
    lines.push(format!(
        "figure_captions: {}",
        inspection.figure_captions.len()
    ));
    if inspection.figure_captions.is_empty() {
        lines.push("  - none".to_string());
    } else {
        for caption in &inspection.figure_captions {
            lines.push(format!("  - {}", caption));
        }
    }

    lines.push(String::new());
    lines.push(format!(
        "table_captions: {}",
        inspection.table_captions.len()
    ));
    if inspection.table_captions.is_empty() {
        lines.push("  - none".to_string());
    } else {
        for caption in &inspection.table_captions {
            lines.push(format!("  - {}", caption));
        }
    }

    lines.push(String::new());
    lines.push(format!(
        "relevance_tags: {}",
        if inspection.relevance_tags.is_empty() {
            "none".to_string()
        } else {
            inspection.relevance_tags.join(", ")
        }
    ));
    if !inspection.provenance.is_empty() {
        lines.push(format!("provenance: {}", inspection.provenance.join(", ")));
    }

    lines.join("\n")
}

fn render_count_map(counts: &std::collections::BTreeMap<String, usize>) -> Vec<String> {
    if counts.is_empty() {
        return vec!["  - none".to_string()];
    }
    counts
        .iter()
        .map(|(name, count)| format!("  - {name}: {count}"))
        .collect()
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
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
