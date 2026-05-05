use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use colored::Colorize;
use comfy_table::Table;
use indicatif::{ProgressBar, ProgressStyle};
use litkg_core::{
    build_context_pack, build_registry_snapshot, compute_corpus_stats, compute_repo_capabilities,
    download_registry_sources, enrich_registry_with_semantic_scholar, inspect_benchmark_support,
    inspect_paper, load_agent_backlog, load_parsed_papers, load_registry, parse_registry_papers,
    promote_benchmark_results, render_promoted_targets, run_benchmarks, search_papers,
    sync_registry, validate_benchmark_catalog, validate_benchmark_results, write_benchmark_results,
    write_parsed_papers, AgentRecommendation, AutoResearchRenderFormat, BenchmarkPromotionRequest,
    BenchmarkResults, BenchmarkSupportStatus, CapabilityOptions, CapabilityState, ContextPack,
    ContextPackRequest, CorpusStats, DownloadOptions, MetricThresholdComparison,
    MetricThresholdRule, PaperInspection, PaperSourceRecord, ParsedPaper,
    PromotionComponentSelection, RepoCapabilitySnapshot, RepoConfig, RuntimeCheck, SearchResults,
    SemanticScholarClient, SemanticScholarConfig, SemanticScholarPaper,
    SemanticScholarSearchRequest, SinkMode,
};
use litkg_graphify::GraphifySink;
use litkg_neo4j::Neo4jSink;
use litkg_viewer::{
    load_and_search_bundle, run_bundle_with_options as run_viewer_bundle_with_options,
    GraphEntryQuery, GraphFilter, GraphModality, GraphSearchHit, ViewerOptions,
};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime};

#[derive(Parser)]
#[command(name = "litkg")]
#[command(about = "Repo-independent literature download and graph materialization toolkit.")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Ingest documents and sync registry
    Ingest(IngestCommand),
    /// Agent-facing capability contract
    Capabilities(CapabilitiesCommand),
    /// Agent-facing task context/action pack
    ContextPack(ContextPackCommand),
    /// Knowledge graph operations
    #[command(subcommand)]
    Kg(KgCommand),
    /// Literature operations
    #[command(subcommand)]
    Lit(LitCommand),
    /// Semantic Scholar operations
    #[command(subcommand)]
    S2(S2Command),
    /// Benchmark operations
    #[command(subcommand)]
    Benchmark(BenchmarkCommand),
    /// Information and capabilities
    #[command(subcommand)]
    Info(InfoCommand),
}

#[derive(Args, Clone)]
struct IngestCommand {
    #[command(flatten)]
    config: ConfigArg,
    /// Optional directory to ingest documents from
    dir: Option<String>,
    #[arg(long, default_value_t = false)]
    recursive: bool,
    #[arg(long, value_enum, default_value_t = DocKindArg::Documentation)]
    kind: DocKindArg,
}

#[derive(Subcommand)]
enum KgCommand {
    Visualize(VisualizeCommand),
    Find(KgFindCommand),
    Consolidate(KgConsolidateCommand),
    Build(WriteCommand),
    Export(WriteCommand),
}

#[derive(Subcommand)]
enum LitCommand {
    Download(DownloadCommand),
    Parse(ConfigArg),
    Search(SearchCommand),
    Show(ShowPaperCommand),
}

#[derive(Subcommand)]
enum S2Command {
    Enrich(SemanticScholarEnrichCommand),
    Search(SemanticScholarSearchCommand),
    Recommend(SemanticScholarRecommendCommand),
    Paper(SemanticScholarPaperCommand),
}

#[derive(Subcommand)]
enum BenchmarkCommand {
    Validate(BenchmarkCatalogArg),
    Run(BenchmarkRunCommand),
    Support(BenchmarkSupportCommand),
    Promote(PromoteBenchmarkResultsCommand),
    RenderTarget(AutoResearchTargetCommand),
    SyncIssue(AutoResearchIssueSyncCommand),
}

#[derive(Subcommand)]
enum InfoCommand {
    Capabilities(CapabilitiesCommand),
    Stats(StatsCommand),
    ContextPack(ContextPackCommand),
}

#[derive(Args, Clone)]
struct ConfigArg {
    #[arg(long)]
    config: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum GraphModalityArg {
    All,
    Code,
    Docs,
    GeneratedContext,
    Literature,
    Memory,
    Backlog,
    ExternalDocs,
}

impl From<GraphModalityArg> for GraphModality {
    fn from(value: GraphModalityArg) -> Self {
        match value {
            GraphModalityArg::All => GraphModality::All,
            GraphModalityArg::Code => GraphModality::Code,
            GraphModalityArg::Docs => GraphModality::Docs,
            GraphModalityArg::GeneratedContext => GraphModality::GeneratedContext,
            GraphModalityArg::Literature => GraphModality::Literature,
            GraphModalityArg::Memory => GraphModality::Memory,
            GraphModalityArg::Backlog => GraphModality::Backlog,
            GraphModalityArg::ExternalDocs => GraphModality::ExternalDocs,
        }
    }
}

#[derive(Args, Clone)]
struct VisualizeCommand {
    #[command(flatten)]
    config: ConfigArg,
    #[arg(long = "modality", value_enum)]
    modalities: Vec<GraphModalityArg>,
    #[arg(long = "exclude-modality", value_enum)]
    exclude_modalities: Vec<GraphModalityArg>,
    #[arg(long)]
    entry: Option<String>,
    #[arg(long = "entry-rg")]
    entry_rg: Option<String>,
    #[arg(long = "focus-depth", default_value_t = 0)]
    focus_depth: usize,
    #[arg(long = "repo-root")]
    repo_root: Option<String>,
}

#[derive(Args, Clone)]
struct KgFindCommand {
    #[command(flatten)]
    config: ConfigArg,
    query: String,
    #[arg(long = "modality", value_enum)]
    modalities: Vec<GraphModalityArg>,
    #[arg(long = "exclude-modality", value_enum)]
    exclude_modalities: Vec<GraphModalityArg>,
    #[arg(long = "repo-root")]
    repo_root: Option<String>,
    #[arg(long, default_value_t = 24)]
    limit: usize,
    #[arg(long = "no-rg", default_value_t = false)]
    no_rg: bool,
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    format: OutputFormat,
}

#[derive(Args, Clone)]
struct KgConsolidateCommand {
    #[command(flatten)]
    config: ConfigArg,
    #[arg(long = "repo-root")]
    repo_root: Option<String>,
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    format: OutputFormat,
}

#[derive(Args, Clone)]
struct IngestDocsCommand {
    #[command(flatten)]
    config: ConfigArg,
    #[arg(long)]
    dir: String,
    #[arg(long, default_value_t = false)]
    recursive: bool,
    #[arg(long, value_enum, default_value_t = DocKindArg::Documentation)]
    kind: DocKindArg,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum DocKindArg {
    Documentation,
    Transcript,
    ResearchNote,
}

impl From<DocKindArg> for litkg_core::DocumentKind {
    fn from(arg: DocKindArg) -> Self {
        match arg {
            DocKindArg::Documentation => litkg_core::DocumentKind::Documentation,
            DocKindArg::Transcript => litkg_core::DocumentKind::Transcript,
            DocKindArg::ResearchNote => litkg_core::DocumentKind::ResearchNote,
        }
    }
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
struct CapabilitiesCommand {
    #[command(flatten)]
    config: ConfigArg,
    #[arg(long = "repo-root")]
    repo_root: Option<String>,
    #[arg(long = "benchmark-catalog")]
    benchmark_catalog: Option<String>,
    #[arg(long = "benchmark-integrations")]
    benchmark_integrations: Option<String>,
    #[arg(long, default_value_t = false)]
    check_runtime: bool,
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    format: OutputFormat,
}

#[derive(Args, Clone)]
struct ContextPackCommand {
    #[command(flatten)]
    config: ConfigArg,
    #[arg(long)]
    task: String,
    #[arg(long, default_value_t = 12_000)]
    budget: usize,
    #[arg(long, default_value = "agents-scaffold")]
    profile: String,
    #[arg(long = "repo-root")]
    repo_root: Option<String>,
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
struct SemanticScholarEnrichCommand {
    #[command(flatten)]
    config: ConfigArg,
    #[arg(long, default_value_t = false)]
    dry_run: bool,
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    format: OutputFormat,
}

#[derive(Args, Clone)]
struct SemanticScholarSearchCommand {
    #[arg(long)]
    config: Option<String>,
    #[arg(long)]
    query: String,
    #[arg(long, default_value_t = 20)]
    limit: usize,
    #[arg(long)]
    year: Option<String>,
    #[arg(long = "publication-date-or-year")]
    publication_date_or_year: Option<String>,
    #[arg(long = "field-of-study")]
    fields_of_study: Vec<String>,
    #[arg(long)]
    venue: Vec<String>,
    #[arg(long)]
    sort: Option<String>,
    #[arg(long = "min-citation-count")]
    min_citation_count: Option<u64>,
    #[arg(long = "open-access-pdf")]
    open_access_pdf: Option<bool>,
    #[arg(long = "field")]
    fields: Vec<String>,
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    format: OutputFormat,
}

#[derive(Args, Clone)]
struct SemanticScholarPaperCommand {
    #[arg(long)]
    config: Option<String>,
    #[arg(long = "paper")]
    paper_id: String,
    #[arg(long = "field")]
    fields: Vec<String>,
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    format: OutputFormat,
}

#[derive(Args, Clone)]
struct SemanticScholarRecommendCommand {
    #[arg(long)]
    config: Option<String>,
    #[arg(long = "positive")]
    positive_paper_ids: Vec<String>,
    #[arg(long = "negative")]
    negative_paper_ids: Vec<String>,
    #[arg(long, default_value_t = 50)]
    limit: usize,
    #[arg(long = "field")]
    fields: Vec<String>,
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    format: OutputFormat,
}

#[derive(Args, Clone)]
struct WriteCommand {
    #[command(flatten)]
    config: ConfigArg,
    #[arg(long, default_value = "text")]
    format: String,
    #[arg(long, default_value_t = false)]
    verbose_paths: bool,
}

#[derive(Args, Clone)]
struct CatalogPathArg {
    #[arg(long = "catalog")]
    path: String,
}

#[derive(Args, Clone)]
struct BenchmarkCatalogArg {
    #[command(flatten)]
    catalog: CatalogPathArg,
    #[arg(long)]
    results: Option<String>,
}

#[derive(Args, Clone)]
struct BenchmarkExecutionArgs {
    #[command(flatten)]
    catalog: CatalogPathArg,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutputMode {
    Text,
    Json,
}

#[derive(Debug, Clone)]
struct SinkWriteSummary {
    kind: &'static str,
    root: PathBuf,
    written_paths: Vec<PathBuf>,
}

#[derive(Debug, serde::Serialize)]
struct ConsolidationProposal {
    summary: String,
    suggestions: Vec<ConsolidationSuggestion>,
    source_refs: Vec<String>,
}

#[derive(Debug, serde::Serialize)]
struct ConsolidationSuggestion {
    target: String,
    action: String,
    rationale: String,
    evidence: Vec<String>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Ingest(args) => {
            let config = RepoConfig::load(&args.config.config)?;
            let pb = ProgressBar::new_spinner();
            pb.set_style(
                ProgressStyle::default_spinner()
                    .template("{spinner:.green} {msg}")
                    .unwrap(),
            );
            pb.enable_steady_tick(Duration::from_millis(100));

            pb.set_message("Syncing registry...");
            let registry = sync_registry(&config)?;
            let mut total_docs = 0;

            if let Some(dir) = &args.dir {
                pb.set_message(format!("Ingesting documents from {}...", dir));
                let docs = litkg_core::ingest_markdown_docs(
                    &config,
                    &PathBuf::from(dir),
                    args.recursive,
                    args.kind.into(),
                )?;
                total_docs += docs.len();
            }

            pb.set_message("Ingesting configured sources...");
            let docs = litkg_core::ingest_configured_sources(&config)?;
            total_docs += docs.len();

            pb.finish_with_message(format!(
                "{}",
                format!(
                    "Successfully synced {} registry records and ingested {} documents.",
                    registry.len(),
                    total_docs
                )
                .green()
                .bold()
            ));
        }
        Commands::Capabilities(args) => run_capabilities(args)?,
        Commands::ContextPack(args) => run_context_pack(args)?,
        Commands::Kg(kg_cmd) => match kg_cmd {
            KgCommand::Visualize(args) => {
                let config = RepoConfig::load(&args.config.config)?;
                inspect_graph(&config, &args)?;
            }
            KgCommand::Find(args) => {
                run_kg_find(args)?;
            }
            KgCommand::Consolidate(args) => {
                run_kg_consolidate(args)?;
            }
            KgCommand::Build(args) => {
                let config = RepoConfig::load(&args.config.config)?;
                let repo_root = resolve_repo_root(&config, &args.config.config, None)?;
                let config = config_with_repo_root(config, &repo_root);
                let pb = ProgressBar::new_spinner();
                pb.set_style(
                    ProgressStyle::default_spinner()
                        .template("{spinner:.green} {msg}")
                        .unwrap(),
                );
                pb.enable_steady_tick(Duration::from_millis(100));

                pb.set_message("Loading parsed papers...");
                let papers = litkg_core::load_parsed_papers(config.parsed_root())?;
                let allow_memory_only_neo4j = papers.is_empty()
                    && matches!(config.sink, SinkMode::Neo4j)
                    && config.memory_state_root().is_some();
                if papers.is_empty() && !allow_memory_only_neo4j {
                    anyhow::bail!(
                        "No parsed papers found under {}",
                        config.parsed_root().display()
                    );
                }

                pb.set_message("Materializing graph...");
                let summaries = materialize(&config, &papers)?;
                pb.set_message("Rebuilding graph...");
                if matches!(config.sink, SinkMode::Graphify | SinkMode::Both) {
                    rebuild_graph(&config)?;
                }
                pb.finish_with_message(format!(
                    "{}",
                    "Graph build completed successfully!".green().bold()
                ));

                print_write_summaries(
                    "materialize",
                    &summaries,
                    parse_output_mode(&args.format)?,
                    args.verbose_paths,
                )?;
            }
            KgCommand::Export(args) => {
                let config = RepoConfig::load(&args.config.config)?;
                let repo_root = resolve_repo_root(&config, &args.config.config, None)?;
                let config = config_with_repo_root(config, &repo_root);
                let pb = ProgressBar::new_spinner();
                pb.set_style(
                    ProgressStyle::default_spinner()
                        .template("{spinner:.green} {msg}")
                        .unwrap(),
                );
                pb.enable_steady_tick(Duration::from_millis(100));

                pb.set_message("Loading parsed papers for export...");
                let papers = litkg_core::load_parsed_papers(config.parsed_root())?;
                if papers.is_empty() && config.memory_state_root().is_none() {
                    anyhow::bail!(
                        "No parsed papers found under {} and no memory_state_root was configured",
                        config.parsed_root().display()
                    );
                }

                pb.set_message("Exporting to Neo4j...");
                let summaries = vec![SinkWriteSummary {
                    kind: "neo4j",
                    root: config.neo4j_export_root(),
                    written_paths: Neo4jSink::export(&config, &papers)?,
                }];
                pb.finish_with_message(format!(
                    "{}",
                    "Neo4j export completed successfully!".green().bold()
                ));
                print_write_summaries(
                    "export-neo4j",
                    &summaries,
                    parse_output_mode(&args.format)?,
                    args.verbose_paths,
                )?;
            }
        },
        Commands::Lit(lit_cmd) => match lit_cmd {
            LitCommand::Download(args) => {
                let config = RepoConfig::load(&args.config.config)?;
                let pb = ProgressBar::new_spinner();
                pb.set_style(
                    ProgressStyle::default_spinner()
                        .template("{spinner:.green} {msg}")
                        .unwrap(),
                );
                pb.enable_steady_tick(Duration::from_millis(100));

                pb.set_message("Loading registry...");
                let registry = if config.registry_path().exists() {
                    load_registry(config.registry_path())?
                } else {
                    sync_registry(&config)?
                };

                pb.set_message("Downloading literature assets...");
                let updated = download_registry_sources(
                    &config,
                    &registry,
                    DownloadOptions {
                        overwrite: args.overwrite,
                        download_pdfs: args.download_pdfs,
                    },
                )?;
                litkg_core::write_registry(&config.registry_path(), &updated)?;
                pb.finish_with_message(format!(
                    "{}",
                    format!("Downloaded literature assets for {} records", updated.len())
                        .green()
                        .bold()
                ));
            }
            LitCommand::Parse(args) => {
                let config = RepoConfig::load(&args.config)?;
                let pb = ProgressBar::new_spinner();
                pb.set_style(
                    ProgressStyle::default_spinner()
                        .template("{spinner:.green} {msg}")
                        .unwrap(),
                );
                pb.enable_steady_tick(Duration::from_millis(100));

                pb.set_message("Loading registry...");
                let registry = if config.registry_path().exists() {
                    load_registry(config.registry_path())?
                } else {
                    sync_registry(&config)?
                };

                pb.set_message("Parsing papers...");
                let papers = parse_registry_papers(&config, &registry)?;
                write_parsed_papers(config.parsed_root(), &papers)?;
                let updated_registry = papers
                    .iter()
                    .map(|paper| paper.metadata.clone())
                    .collect::<Vec<_>>();
                litkg_core::write_registry(&config.registry_path(), &updated_registry)?;
                pb.finish_with_message(format!(
                    "{}",
                    format!(
                        "Parsed {} papers into {}",
                        papers.len(),
                        config.parsed_root().display()
                    )
                    .green()
                    .bold()
                ));
            }
            LitCommand::Search(args) => {
                if args.limit == 0 {
                    anyhow::bail!("--limit must be at least 1");
                }
                let config = RepoConfig::load(&args.config.config)?;
                let papers = load_parsed_papers(config.parsed_root())?;
                let registry = load_registry_or_sync(&config, &papers)?;
                let hits = search_papers(
                    &registry,
                    &papers,
                    &config.relevance_tags,
                    &args.query,
                    args.limit,
                )?;
                print_structured_output(&hits, args.format, render_search_results)?;
            }
            LitCommand::Show(args) => {
                let config = RepoConfig::load(&args.config.config)?;
                let papers = load_parsed_papers(config.parsed_root())?;
                let registry = load_registry_or_sync(&config, &papers)?;
                let inspection = inspect_paper(&config, &registry, &papers, &args.paper_selector)?;
                print_structured_output(&inspection, args.format, render_paper_inspection)?;
            }
        },
        Commands::S2(s2_cmd) => match s2_cmd {
            S2Command::Enrich(args) => {
                let config = RepoConfig::load(&args.config.config)?;
                let pb = ProgressBar::new_spinner();
                pb.set_style(
                    ProgressStyle::default_spinner()
                        .template("{spinner:.green} {msg}")
                        .unwrap(),
                );
                pb.enable_steady_tick(Duration::from_millis(100));

                pb.set_message("Loading registry...");
                let registry = if config.registry_path().exists() {
                    load_registry(config.registry_path())?
                } else {
                    sync_registry(&config)?
                };

                pb.set_message("Enriching with Semantic Scholar...");
                let updated = enrich_registry_with_semantic_scholar(&config, &registry)?;
                let enriched_count = updated
                    .iter()
                    .filter(|record| record.semantic_scholar.is_some())
                    .count();
                if !args.dry_run {
                    litkg_core::write_registry(&config.registry_path(), &updated)?;
                }
                match args.format {
                    OutputFormat::Text => {
                        let action = if args.dry_run {
                            "Would enrich"
                        } else {
                            "Enriched"
                        };
                        pb.finish_with_message(format!(
                            "{}",
                            format!(
                                "{} {} of {} registry records with Semantic Scholar metadata",
                                action,
                                enriched_count,
                                updated.len()
                            )
                            .green()
                            .bold()
                        ));
                    }
                    OutputFormat::Json => {
                        pb.finish_and_clear();
                        println!(
                            "{}",
                            serde_json::to_string_pretty(&serde_json::json!({
                                "records": updated.len(),
                                "enriched": enriched_count,
                                "dry_run": args.dry_run,
                                "registry_path": config.registry_path(),
                            }))?
                        )
                    }
                }
            }
            S2Command::Search(args) => {
                let semantic_config = load_semantic_scholar_config(args.config.as_deref())?;
                let fields = semantic_fields(&semantic_config, &args.fields);
                let pb = ProgressBar::new_spinner();
                pb.set_style(
                    ProgressStyle::default_spinner()
                        .template("{spinner:.green} {msg}")
                        .unwrap(),
                );
                pb.enable_steady_tick(Duration::from_millis(100));
                pb.set_message("Searching Semantic Scholar...");
                let mut client = SemanticScholarClient::from_config(semantic_config, None)?;
                let mut request =
                    SemanticScholarSearchRequest::new(args.query.clone(), args.limit, fields);
                request.year = args.year.clone();
                request.publication_date_or_year = args.publication_date_or_year.clone();
                request.fields_of_study = args.fields_of_study.clone();
                request.venue = args.venue.clone();
                request.sort = args.sort.clone();
                request.min_citation_count = args.min_citation_count;
                request.open_access_pdf = args.open_access_pdf;
                let papers = client.search_papers(&request)?;
                pb.finish_and_clear();
                print_structured_output(&papers, args.format, |papers| {
                    render_semantic_scholar_papers(papers)
                })?;
            }
            S2Command::Paper(args) => {
                let semantic_config = load_semantic_scholar_config(args.config.as_deref())?;
                let fields = semantic_fields(&semantic_config, &args.fields);
                let pb = ProgressBar::new_spinner();
                pb.set_style(
                    ProgressStyle::default_spinner()
                        .template("{spinner:.green} {msg}")
                        .unwrap(),
                );
                pb.enable_steady_tick(Duration::from_millis(100));
                pb.set_message("Fetching paper from Semantic Scholar...");
                let mut client = SemanticScholarClient::from_config(semantic_config, None)?;
                let paper = client.get_paper(&args.paper_id, &fields)?;
                pb.finish_and_clear();
                print_structured_output(&paper, args.format, render_semantic_scholar_paper)?;
            }
            S2Command::Recommend(args) => {
                if args.positive_paper_ids.is_empty() {
                    anyhow::bail!(
                        "semantic-scholar-recommend requires at least one --positive paper id"
                    );
                }
                let semantic_config = load_semantic_scholar_config(args.config.as_deref())?;
                let fields = semantic_fields(&semantic_config, &args.fields);
                let pb = ProgressBar::new_spinner();
                pb.set_style(
                    ProgressStyle::default_spinner()
                        .template("{spinner:.green} {msg}")
                        .unwrap(),
                );
                pb.enable_steady_tick(Duration::from_millis(100));
                pb.set_message("Fetching recommendations from Semantic Scholar...");
                let mut client = SemanticScholarClient::from_config(semantic_config, None)?;
                let papers = client.recommend_papers(
                    &args.positive_paper_ids,
                    &args.negative_paper_ids,
                    args.limit,
                    &fields,
                )?;
                pb.finish_and_clear();
                print_structured_output(&papers, args.format, |papers| {
                    render_semantic_scholar_papers(papers)
                })?;
            }
        },
        Commands::Benchmark(bench_cmd) => match bench_cmd {
            BenchmarkCommand::Validate(args) => {
                let catalog = litkg_core::load_benchmark_catalog(&args.catalog.path)?;
                let mut summary = validate_benchmark_catalog(&catalog)?;
                if let Some(results_path) = &args.results {
                    let results = litkg_core::load_benchmark_results(results_path)?;
                    summary = validate_benchmark_results(&catalog, &results)?;
                }
                println!(
                    "{}",
                    format!("Validated benchmark catalog: {} benchmarks, {} metrics, {} autoresearch components, {} autoresearch targets, {} benchmark runs",
                    summary.benchmark_count,
                    summary.metric_count,
                    summary.component_count,
                    summary.target_count,
                    summary.run_count).green().bold()
                );
            }
            BenchmarkCommand::Support(args) => {
                let catalog = litkg_core::load_benchmark_catalog(&args.execution.catalog.path)?;
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
            BenchmarkCommand::Run(args) => {
                let catalog = litkg_core::load_benchmark_catalog(&args.execution.catalog.path)?;
                let integrations =
                    litkg_core::load_benchmark_integrations(&args.execution.integrations)?;
                let plan = match &args.execution.plan {
                    Some(path) => Some(litkg_core::load_benchmark_run_plan(path)?),
                    None => None,
                };
                let pb = ProgressBar::new_spinner();
                pb.set_style(
                    ProgressStyle::default_spinner()
                        .template("{spinner:.green} {msg}")
                        .unwrap(),
                );
                pb.enable_steady_tick(Duration::from_millis(100));
                pb.set_message("Running benchmarks...");
                let results = run_benchmarks(
                    &catalog,
                    &integrations,
                    plan.as_ref(),
                    &args.execution.benchmark_ids,
                )?;
                write_benchmark_results(&args.output, &results)?;
                let summary = validate_benchmark_results(&catalog, &results)?;
                pb.finish_with_message(format!(
                    "{}",
                    format!(
                        "Ran benchmark integrations: {} benchmarks, {} runs written to {}",
                        summary.benchmark_count, summary.run_count, args.output
                    )
                    .green()
                    .bold()
                ));
            }
            BenchmarkCommand::RenderTarget(args) => {
                let catalog = litkg_core::load_benchmark_catalog(&args.catalog.catalog.path)?;
                let results: Option<BenchmarkResults> = match &args.catalog.results {
                    Some(path) => Some(litkg_core::load_benchmark_results(path)?),
                    None => None,
                };
                let format = parse_render_format(&args.format)?;
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
            BenchmarkCommand::SyncIssue(args) => {
                let catalog = litkg_core::load_benchmark_catalog(&args.catalog.catalog.path)?;
                let results = match &args.catalog.results {
                    Some(path) => Some(litkg_core::load_benchmark_results(path)?),
                    None => None,
                };
                let body = litkg_core::render_autoresearch_target(
                    &catalog,
                    results.as_ref(),
                    &args.target_id,
                    &args.component_ids,
                    &args.benchmark_ids,
                    AutoResearchRenderFormat::GithubIssue,
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
                    let pb = ProgressBar::new_spinner();
                    pb.set_style(
                        ProgressStyle::default_spinner()
                            .template("{spinner:.green} {msg}")
                            .unwrap(),
                    );
                    pb.enable_steady_tick(Duration::from_millis(100));
                    pb.set_message("Syncing issue to GitHub...");
                    let issue_url = create_github_issue(&repo, &title, &body, &args.labels)?;
                    pb.finish_with_message(format!(
                        "{}",
                        format!("Created issue: {}", issue_url).green().bold()
                    ));
                }
            }
            BenchmarkCommand::Promote(args) => {
                let catalog = litkg_core::load_benchmark_catalog(&args.catalog.catalog.path)?;
                let results_path = args
                    .catalog
                    .results
                    .as_ref()
                    .context("`promote-benchmark-results` requires --results")?;
                let results = litkg_core::load_benchmark_results(results_path)?;
                let request = BenchmarkPromotionRequest {
                    target_ids: args.target_ids.clone(),
                    benchmark_ids: args.benchmark_ids.clone(),
                    status_filters: args.status_filters.clone(),
                    metric_thresholds: args
                        .metric_thresholds
                        .iter()
                        .map(|raw| parse_metric_threshold(raw))
                        .collect::<Result<Vec<_>>>()?,
                    component_selection: parse_component_selection(&args.component_selection)?,
                    component_ids: args.component_ids.clone(),
                };
                let promoted = promote_benchmark_results(&catalog, &results, &request)?;
                let rendered =
                    render_promoted_targets(&promoted, parse_render_format(&args.format)?)?;
                println!("{rendered}");
            }
        },
        Commands::Info(info_cmd) => match info_cmd {
            InfoCommand::Capabilities(args) => run_capabilities(args)?,
            InfoCommand::ContextPack(args) => run_context_pack(args)?,
            InfoCommand::Stats(args) => {
                let config = RepoConfig::load(&args.config.config)?;
                let papers = load_parsed_papers(config.parsed_root())?;
                let registry = load_registry_or_sync(&config, &papers)?;
                let stats = compute_corpus_stats(&registry, &papers);
                print_structured_output(&stats, args.format, render_stats)?;
            }
        },
    }
    Ok(())
}

fn run_capabilities(args: CapabilitiesCommand) -> Result<()> {
    let config = RepoConfig::load(&args.config.config)?;
    let snapshot = compute_repo_capabilities(
        &config,
        CapabilityOptions {
            config_path: PathBuf::from(&args.config.config),
            repo_root: args.repo_root.clone().map(PathBuf::from),
            benchmark_catalog: args.benchmark_catalog.clone().map(PathBuf::from),
            benchmark_integrations: args.benchmark_integrations.clone().map(PathBuf::from),
            check_runtime: args.check_runtime,
        },
    )?;
    print_structured_output(&snapshot, args.format, render_capabilities)
}

fn run_context_pack(args: ContextPackCommand) -> Result<()> {
    let config = RepoConfig::load(&args.config.config)?;
    let repo_root = match args.repo_root.clone() {
        Some(path) => PathBuf::from(path),
        None => std::env::current_dir().context("Failed to determine current directory")?,
    };
    let pack = build_context_pack(
        &config,
        ContextPackRequest {
            config_path: Some(PathBuf::from(&args.config.config)),
            repo_root,
            task: args.task.clone(),
            budget_tokens: args.budget,
            profile: args.profile.clone(),
        },
    )?;
    print_structured_output(&pack, args.format, render_context_pack)
}

fn materialize(
    config: &RepoConfig,
    papers: &[litkg_core::ParsedPaper],
) -> Result<Vec<SinkWriteSummary>> {
    let summaries = match config.sink {
        SinkMode::Graphify => vec![SinkWriteSummary {
            kind: "graphify",
            root: config.generated_docs_root.clone(),
            written_paths: GraphifySink::materialize(config, papers)?,
        }],
        SinkMode::Neo4j => vec![SinkWriteSummary {
            kind: "neo4j",
            root: config.neo4j_export_root(),
            written_paths: Neo4jSink::export(config, papers)?,
        }],
        SinkMode::Both => vec![
            SinkWriteSummary {
                kind: "graphify",
                root: config.generated_docs_root.clone(),
                written_paths: GraphifySink::materialize(config, papers)?,
            },
            SinkWriteSummary {
                kind: "neo4j",
                root: config.neo4j_export_root(),
                written_paths: Neo4jSink::export(config, papers)?,
            },
        ],
    };
    Ok(summaries)
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

fn inspect_graph(config: &RepoConfig, args: &VisualizeCommand) -> Result<()> {
    let repo_root = resolve_repo_root(
        config,
        args.config.config.as_str(),
        args.repo_root.as_deref(),
    )?;
    let config = config_with_repo_root(config.clone(), &repo_root);
    let bundle_root = config.neo4j_export_root();
    let nodes_path = bundle_root.join("nodes.jsonl");
    let edges_path = bundle_root.join("edges.jsonl");

    if !(nodes_path.exists() && edges_path.exists()) {
        let papers = litkg_core::load_parsed_papers(config.parsed_root())?;
        Neo4jSink::export(&config, &papers)?;
        println!(
            "Generated Neo4j export bundle under {}",
            bundle_root.display()
        );
    }

    let options = ViewerOptions {
        filter: graph_filter(&args.modalities, &args.exclude_modalities),
        repo_root: Some(repo_root),
        entry: args.entry.clone(),
        entry_rg: args.entry_rg.clone(),
        focus_depth: args.focus_depth,
    };
    run_viewer_bundle_with_options(&bundle_root, options)
}

fn run_kg_find(args: KgFindCommand) -> Result<()> {
    let config = RepoConfig::load(&args.config.config)?;
    let repo_root = resolve_repo_root(
        &config,
        args.config.config.as_str(),
        args.repo_root.as_deref(),
    )?;
    let config = config_with_repo_root(config, &repo_root);
    let bundle_root = config.neo4j_export_root();
    let nodes_path = bundle_root.join("nodes.jsonl");
    let edges_path = bundle_root.join("edges.jsonl");
    if !(nodes_path.exists() && edges_path.exists())
        || export_is_stale(&config, &[nodes_path.as_path(), edges_path.as_path()])?
    {
        let papers = litkg_core::load_parsed_papers(config.parsed_root())?;
        Neo4jSink::export(&config, &papers)?;
    }
    let query = GraphEntryQuery {
        query: args.query.clone(),
        filter: graph_filter(&args.modalities, &args.exclude_modalities),
        repo_root: Some(repo_root),
        use_rg: !args.no_rg,
        limit: args.limit,
        authority_tiers: config.authority_tiers.clone().unwrap_or_default(),
    };
    let hits = load_and_search_bundle(&bundle_root, query)?;
    print_structured_output(&hits, args.format, |hits| render_graph_search_hits(hits))
}

fn run_kg_consolidate(args: KgConsolidateCommand) -> Result<()> {
    let config = RepoConfig::load(&args.config.config)?;
    let repo_root = resolve_repo_root(
        &config,
        args.config.config.as_str(),
        args.repo_root.as_deref(),
    )?;
    let proposal = build_consolidation_proposal(&repo_root)?;
    print_structured_output(&proposal, args.format, render_consolidation_proposal)
}

fn build_consolidation_proposal(repo_root: &Path) -> Result<ConsolidationProposal> {
    let mut suggestions = Vec::new();
    let mut source_refs = Vec::new();
    let history_root = repo_root.join(".agents/memory/history");
    let mut history_files = files_under(&history_root)
        .into_iter()
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("md"))
        .collect::<Vec<_>>();
    history_files.sort();
    history_files.reverse();
    for path in history_files.into_iter().take(8) {
        let rel = relative_display(repo_root, &path);
        let raw = fs::read_to_string(&path).unwrap_or_default();
        if raw.contains("canonical_updates_needed") || raw.contains("canonical updates") {
            suggestions.push(ConsolidationSuggestion {
                target: ".agents/memory/state/".into(),
                action: "review_canonical_update_candidates".into(),
                rationale:
                    "Recent debrief advertises canonical-update information; promote only cited current truth."
                        .into(),
                evidence: vec![format!("repo:{rel}")],
            });
            source_refs.push(format!("repo:{rel}"));
        }
    }

    let backlog = load_agent_backlog(repo_root)?;
    for record in backlog
        .iter()
        .filter(|record| record.title.to_lowercase().contains("litkg"))
        .take(8)
    {
        let target = match record.kind {
            litkg_core::AgentBacklogKind::Issue => ".agents/issues.toml",
            litkg_core::AgentBacklogKind::Todo => ".agents/todos.toml",
        };
        suggestions.push(ConsolidationSuggestion {
            target: target.into(),
            action: "keep_or_update_backlog_record".into(),
            rationale: format!(
                "Active litkg backlog item `{}` should remain visible until its acceptance criteria are satisfied.",
                record.id
            ),
            evidence: vec![format!("repo:{}:{}", record.source_path, record.line_start)],
        });
        source_refs.push(format!("repo:{}:{}", record.source_path, record.line_start));
    }

    suggestions.push(ConsolidationSuggestion {
        target: ".agents/memory/state/PROJECT_STATE.md".into(),
        action: "proposal_only".into(),
        rationale:
            "Do not overwrite canonical memory automatically; apply only after reviewing the evidence above."
                .into(),
        evidence: source_refs.clone(),
    });
    source_refs.sort();
    source_refs.dedup();
    Ok(ConsolidationProposal {
        summary: format!(
            "{} proposal(s); review and apply manually, no files were changed.",
            suggestions.len()
        ),
        suggestions,
        source_refs,
    })
}

fn render_consolidation_proposal(proposal: &ConsolidationProposal) -> String {
    let mut lines = vec![
        "litkg consolidation proposal".to_string(),
        proposal.summary.clone(),
        String::new(),
    ];
    for suggestion in &proposal.suggestions {
        lines.push(format!("- {} -> {}", suggestion.target, suggestion.action));
        lines.push(format!("  {}", suggestion.rationale));
        if !suggestion.evidence.is_empty() {
            lines.push(format!("  evidence: {}", suggestion.evidence.join(", ")));
        }
    }
    lines.join("\n")
}

fn export_is_stale(config: &RepoConfig, outputs: &[&Path]) -> Result<bool> {
    let Some(oldest_output) = oldest_mtime(outputs)? else {
        return Ok(true);
    };
    Ok(freshness_input_paths(config)
        .into_iter()
        .filter_map(|path| fs::metadata(path).ok()?.modified().ok())
        .any(|mtime| mtime > oldest_output))
}

fn relative_display(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn oldest_mtime(paths: &[&Path]) -> Result<Option<SystemTime>> {
    let mut mtimes = Vec::new();
    for path in paths {
        if !path.is_file() {
            return Ok(None);
        }
        mtimes.push(
            fs::metadata(path)
                .with_context(|| format!("Failed to stat {}", path.display()))?
                .modified()
                .with_context(|| format!("Failed to read mtime for {}", path.display()))?,
        );
    }
    Ok(mtimes.into_iter().min())
}

fn freshness_input_paths(config: &RepoConfig) -> Vec<PathBuf> {
    let mut paths = vec![
        config.registry_path(),
        config.manifest_path.clone(),
        config.bib_path.clone(),
    ];
    paths.extend(files_under(config.parsed_root()));
    if let Some(root) = config.memory_state_root() {
        paths.extend(files_under(root));
    }
    for source in config.sources.values() {
        for pattern in &source.include {
            paths.extend(expand_glob(pattern));
        }
        for entrypoint in &source.entrypoints {
            paths.push(entrypoint.clone());
        }
        if let Some(manifest) = &source.manifest {
            paths.push(manifest.clone());
        }
        if let Some(bib) = &source.bib {
            paths.push(bib.clone());
        }
        if let Some(tex) = &source.tex {
            paths.extend(expand_glob(tex));
        }
        if let Some(pdfs) = &source.pdfs {
            paths.extend(expand_glob(pdfs));
        }
    }
    paths.sort();
    paths.dedup();
    paths
}

fn files_under(root: impl AsRef<Path>) -> Vec<PathBuf> {
    let root = root.as_ref();
    if root.is_file() {
        return vec![root.to_path_buf()];
    }
    if !root.is_dir() {
        return Vec::new();
    }
    let mut paths = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.is_file() {
                paths.push(path);
            }
        }
    }
    paths
}

fn expand_glob(pattern: &str) -> Vec<PathBuf> {
    match glob::glob(pattern) {
        Ok(paths) => paths.flatten().filter(|path| path.is_file()).collect(),
        Err(_) => Vec::new(),
    }
}

fn graph_filter(
    modalities: &[GraphModalityArg],
    exclude_modalities: &[GraphModalityArg],
) -> GraphFilter {
    let mut filter = if modalities.is_empty() {
        GraphFilter::all()
    } else {
        GraphFilter::only(modalities.iter().copied().map(GraphModality::from))
    };
    filter
        .exclude
        .extend(exclude_modalities.iter().copied().map(GraphModality::from));
    filter
}

fn resolve_repo_root(
    config: &RepoConfig,
    config_path: &str,
    cli_repo_root: Option<&str>,
) -> Result<PathBuf> {
    if let Some(repo_root) = cli_repo_root {
        return absolutize(PathBuf::from(repo_root));
    }
    let project_root = config
        .project
        .as_ref()
        .map(|project| project.root.clone())
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| PathBuf::from("."));
    if project_root.is_absolute() {
        return Ok(project_root);
    }

    let cwd_candidate = std::env::current_dir()?.join(&project_root);
    if cwd_candidate.join(&config.manifest_path).exists() {
        return Ok(cwd_candidate.canonicalize().unwrap_or(cwd_candidate));
    }

    let config_path = PathBuf::from(config_path);
    let config_path = absolutize(config_path)?;
    let config_parent = config_path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."));
    let anchored_parent = if config_parent
        .file_name()
        .is_some_and(|name| name == ".configs")
    {
        config_parent.parent().unwrap_or(config_parent)
    } else {
        config_parent
    };
    let anchored = anchored_parent.join(project_root);
    Ok(anchored.canonicalize().unwrap_or(anchored))
}

fn absolutize(path: PathBuf) -> Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path
    } else {
        std::env::current_dir()?.join(path)
    };
    Ok(absolute.canonicalize().unwrap_or(absolute))
}

fn config_with_repo_root(mut config: RepoConfig, repo_root: &Path) -> RepoConfig {
    absolutize_config_path(repo_root, &mut config.manifest_path);
    absolutize_config_path(repo_root, &mut config.bib_path);
    absolutize_config_path(repo_root, &mut config.tex_root);
    absolutize_config_path(repo_root, &mut config.pdf_root);
    absolutize_config_path(repo_root, &mut config.generated_docs_root);
    if let Some(path) = &mut config.registry_path {
        absolutize_config_path(repo_root, path);
    }
    if let Some(path) = &mut config.parsed_root {
        absolutize_config_path(repo_root, path);
    }
    if let Some(path) = &mut config.neo4j_export_root {
        absolutize_config_path(repo_root, path);
    }
    if let Some(path) = &mut config.memory_state_root {
        absolutize_config_path(repo_root, path);
    }
    if let Some(project) = &mut config.project {
        project.root = repo_root.to_path_buf();
    }
    if let Some(storage) = &mut config.storage {
        absolutize_config_path(repo_root, &mut storage.generated_root);
        absolutize_config_path(repo_root, &mut storage.db_root);
        absolutize_config_path(repo_root, &mut storage.runtime_cache_root);
    }
    config
}

fn absolutize_config_path(repo_root: &Path, path: &mut PathBuf) {
    if !path.is_absolute() {
        *path = repo_root.join(&path);
    }
}

fn render_graph_search_hits(hits: &[GraphSearchHit]) -> String {
    if hits.is_empty() {
        return "No graph entries matched.".into();
    }
    hits.iter()
        .map(|hit| {
            let location = hit
                .repo_path
                .as_ref()
                .map(|path| {
                    hit.line_start
                        .map(|line| format!("{path}:{line}"))
                        .unwrap_or_else(|| path.clone())
                })
                .unwrap_or_else(|| hit.node_id.clone());
            let snippet = hit
                .snippet
                .as_ref()
                .map(|snippet| format!("\n  {}", snippet.replace('\n', "\n  ")))
                .unwrap_or_default();
            let rank = hit
                .rank
                .as_ref()
                .map(|rank| {
                    format!(
                        " source_type={} authority={} final={:.1}",
                        rank.source_type, rank.authority, rank.score_final
                    )
                })
                .unwrap_or_default();
            format!(
                "- {} [{}] score={} via={} at {}{}{}",
                hit.title,
                hit.modality.as_str(),
                hit.score,
                hit.matched_field,
                location,
                rank,
                snippet
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn load_registry_or_sync(
    config: &RepoConfig,
    _parsed_papers: &[ParsedPaper],
) -> Result<Vec<PaperSourceRecord>> {
    if config.registry_path().exists() {
        load_registry(config.registry_path())
    } else {
        build_registry_snapshot(config)
    }
}

fn load_semantic_scholar_config(config_path: Option<&str>) -> Result<SemanticScholarConfig> {
    Ok(match config_path {
        Some(path) => RepoConfig::load(path)?.semantic_scholar_config(),
        None => SemanticScholarConfig::default(),
    })
}

fn semantic_fields(config: &SemanticScholarConfig, cli_fields: &[String]) -> Vec<String> {
    let expanded_cli_fields = cli_fields
        .iter()
        .flat_map(|field| field.split(','))
        .map(str::trim)
        .filter(|field| !field.is_empty())
        .map(String::from)
        .collect::<Vec<_>>();
    if !expanded_cli_fields.is_empty() {
        expanded_cli_fields
    } else if !config.fields.is_empty() {
        config.fields.clone()
    } else {
        litkg_core::default_semantic_scholar_fields()
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

fn render_semantic_scholar_papers(papers: &[SemanticScholarPaper]) -> String {
    if papers.is_empty() {
        return "No Semantic Scholar papers returned.".into();
    }
    papers
        .iter()
        .enumerate()
        .map(|(index, paper)| {
            format!(
                "{}. {} ({})\n   year: {} | citations: {} | venue: {}\n   authors: {}",
                index + 1,
                paper.title.as_deref().unwrap_or("untitled"),
                paper.paper_id.as_deref().unwrap_or("no paperId"),
                paper
                    .year
                    .map(|year| year.to_string())
                    .unwrap_or_else(|| "n/a".into()),
                paper
                    .citation_count
                    .map(|count| count.to_string())
                    .unwrap_or_else(|| "n/a".into()),
                paper.venue.as_deref().unwrap_or("n/a"),
                render_semantic_authors(&paper.authors),
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn render_semantic_scholar_paper(paper: &SemanticScholarPaper) -> String {
    let mut lines = vec![
        format!(
            "{} ({})",
            paper.title.as_deref().unwrap_or("untitled"),
            paper.paper_id.as_deref().unwrap_or("no paperId")
        ),
        format!(
            "year: {} | citations: {} | references: {}",
            paper
                .year
                .map(|year| year.to_string())
                .unwrap_or_else(|| "n/a".into()),
            paper
                .citation_count
                .map(|count| count.to_string())
                .unwrap_or_else(|| "n/a".into()),
            paper
                .reference_count
                .map(|count| count.to_string())
                .unwrap_or_else(|| "n/a".into()),
        ),
        format!("venue: {}", paper.venue.as_deref().unwrap_or("n/a")),
        format!("authors: {}", render_semantic_authors(&paper.authors)),
    ];
    if let Some(tldr) = paper.tldr.as_ref().and_then(|tldr| tldr.text.as_deref()) {
        lines.push(format!("tldr: {tldr}"));
    }
    if let Some(abstract_text) = &paper.abstract_text {
        lines.push(format!("abstract: {abstract_text}"));
    }
    lines.join("\n")
}

fn render_semantic_authors(authors: &[litkg_core::SemanticScholarAuthor]) -> String {
    if authors.is_empty() {
        "n/a".into()
    } else {
        authors
            .iter()
            .map(|author| author.name.clone())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

fn render_stats(stats: &CorpusStats) -> String {
    let mut lines = vec!["litkg corpus stats".to_string(), String::new()];

    let mut table = Table::new();
    table.set_header(vec!["Metric", "Value"]);
    table.add_row(vec!["papers", &stats.total_papers.to_string()]);
    table.add_row(vec![
        "parsed_papers",
        &stats.papers_with_parsed_content.to_string(),
    ]);
    table.add_row(vec!["local_tex", &stats.papers_with_local_tex.to_string()]);
    table.add_row(vec!["local_pdf", &stats.papers_with_local_pdf.to_string()]);
    table.add_row(vec!["sections", &stats.total_sections.to_string()]);
    table.add_row(vec!["figures", &stats.total_figures.to_string()]);
    table.add_row(vec!["tables", &stats.total_tables.to_string()]);
    table.add_row(vec!["citations", &stats.total_citations.to_string()]);
    lines.push(table.to_string());

    lines.push(String::new());
    lines.push("source_kinds:".to_string());
    let mut table2 = Table::new();
    table2.set_header(vec!["Source Kind", "Count"]);
    if stats.source_kind_counts.is_empty() {
        table2.add_row(vec!["none", ""]);
    } else {
        for (k, v) in &stats.source_kind_counts {
            table2.add_row(vec![k.as_str(), &v.to_string()]);
        }
    }
    lines.push(table2.to_string());

    lines.push(String::new());
    lines.push("download_modes:".to_string());
    let mut table3 = Table::new();
    table3.set_header(vec!["Download Mode", "Count"]);
    if stats.download_mode_counts.is_empty() {
        table3.add_row(vec!["none", ""]);
    } else {
        for (k, v) in &stats.download_mode_counts {
            table3.add_row(vec![k.as_str(), &v.to_string()]);
        }
    }
    lines.push(table3.to_string());

    lines.push(String::new());
    lines.push("parse_statuses:".to_string());
    let mut table4 = Table::new();
    table4.set_header(vec!["Parse Status", "Count"]);
    if stats.parse_status_counts.is_empty() {
        table4.add_row(vec!["none", ""]);
    } else {
        for (k, v) in &stats.parse_status_counts {
            table4.add_row(vec![k.as_str(), &v.to_string()]);
        }
    }
    lines.push(table4.to_string());

    lines.join("\n")
}

fn render_capabilities(snapshot: &RepoCapabilitySnapshot) -> String {
    let mut lines = vec![
        "litkg capability snapshot".to_string(),
        format!("config: {}", snapshot.config_path.display()),
    ];
    if let Some(repo_root) = &snapshot.repo_root {
        lines.push(format!("repo_root: {}", repo_root.display()));
    }

    lines.push(String::new());
    lines.push("enabled:".to_string());

    let mut table = Table::new();
    table.set_header(vec!["Capability", "State", "Details"]);

    table.add_row(vec![
        "Literature Registry",
        state_label(&snapshot.literature_registry.state),
        &format!(
            "records={}, manifest={}, bib={}, registry_generated={}",
            snapshot.literature_registry.records_total,
            yes_no(snapshot.literature_registry.manifest_present),
            yes_no(snapshot.literature_registry.bib_present),
            yes_no(snapshot.literature_registry.registry_generated)
        ),
    ]);

    table.add_row(vec![
        "Downloads",
        state_label(&snapshot.downloads.state),
        &format!(
            "arxiv_records={}, local_tex={}, local_pdf={}, download_pdfs={}",
            snapshot.downloads.arxiv_records,
            snapshot.downloads.records_with_local_tex,
            snapshot.downloads.records_with_local_pdf,
            yes_no(snapshot.downloads.download_pdfs_configured)
        ),
    ]);

    table.add_row(vec![
        "Tex Parsing",
        state_label(&snapshot.parsing.state),
        &format!(
            "parsed={}, structured={}, sections={}, citations={}, citation_refs={}",
            snapshot.parsing.parsed_papers,
            snapshot.parsing.papers_with_structured_content,
            snapshot.parsing.total_sections,
            snapshot.parsing.total_citations,
            snapshot.parsing.total_citation_references
        ),
    ]);

    table.add_row(vec![
        "Graphify",
        state_label(&snapshot.graph_outputs.graphify_state),
        &format!(
            "configured={}, index={}, manifest={}",
            yes_no(snapshot.graph_outputs.graphify_configured),
            yes_no(snapshot.graph_outputs.graphify_index_generated),
            yes_no(snapshot.graph_outputs.graphify_manifest_generated)
        ),
    ]);

    table.add_row(vec![
        "Neo4j Export",
        state_label(&snapshot.graph_outputs.neo4j_state),
        &format!(
            "configured={}, nodes={}, edges={}",
            yes_no(snapshot.graph_outputs.neo4j_configured),
            yes_no(snapshot.graph_outputs.neo4j_nodes_generated),
            yes_no(snapshot.graph_outputs.neo4j_edges_generated)
        ),
    ]);

    table.add_row(vec![
        "Native Viewer",
        state_label(&snapshot.graph_outputs.native_viewer_state),
        "",
    ]);

    table.add_row(vec![
        "Semantic Scholar",
        state_label(&snapshot.semantic_scholar.state),
        &format!(
            "configured={}, key_env={}, key_present={}, enriched={}",
            yes_no(snapshot.semantic_scholar.configured),
            snapshot.semantic_scholar.api_key_env,
            yes_no(snapshot.semantic_scholar.api_key_present),
            snapshot.semantic_scholar.enriched_records
        ),
    ]);

    table.add_row(vec![
        "Project Memory",
        state_label(&snapshot.project_memory.state),
        &format!(
            "configured={}, root_present={}, nodes={}, surfaces={}",
            yes_no(snapshot.project_memory.configured),
            yes_no(snapshot.project_memory.root_present),
            snapshot.project_memory.imported_nodes,
            snapshot.project_memory.imported_surfaces
        ),
    ]);

    lines.push(table.to_string());

    lines.push(String::new());
    lines.push("agent backend contract:".to_string());
    let mut backend_table = Table::new();
    backend_table.set_header(vec!["Backend", "State", "Recommendation", "Repair"]);
    for backend in &snapshot.conformance.backends {
        backend_table.add_row(vec![
            backend.name.as_str(),
            state_label(&backend.state),
            recommendation_label(&backend.agent_recommendation),
            backend.repair_command.as_deref().unwrap_or("n/a"),
        ]);
    }
    lines.push(backend_table.to_string());

    lines.push(String::new());
    lines.push("runtime:".to_string());
    lines.push(format!("  checked: {}", yes_no(snapshot.runtime.checked)));

    let mut runtime_table = Table::new();
    runtime_table.set_header(vec!["Dependency", "State", "Detail"]);

    let add_runtime_row = |table: &mut Table, name: &str, check: &RuntimeCheck| {
        table.add_row(vec![name, state_label(&check.state), &check.detail]);
    };

    add_runtime_row(&mut runtime_table, "docker", &snapshot.runtime.docker);
    add_runtime_row(
        &mut runtime_table,
        "neo4j_service",
        &snapshot.runtime.neo4j_service,
    );
    add_runtime_row(&mut runtime_table, "uv", &snapshot.runtime.uv);
    add_runtime_row(
        &mut runtime_table,
        "codegraphcontext",
        &snapshot.runtime.codegraphcontext,
    );
    add_runtime_row(
        &mut runtime_table,
        "graphiti_helpers",
        &snapshot.runtime.graphiti_helpers,
    );
    add_runtime_row(
        &mut runtime_table,
        "ollama_service",
        &snapshot.runtime.ollama_service,
    );

    lines.push(runtime_table.to_string());

    if let Some(benchmarks) = &snapshot.benchmarks {
        lines.extend([
            String::new(),
            "benchmarks:".to_string(),
            format!("  state: {}", state_label(&benchmarks.state)),
            format!("  catalog_present: {}", yes_no(benchmarks.catalog_present)),
            format!(
                "  integrations_present: {}",
                yes_no(benchmarks.integrations_present)
            ),
            format!("  support_entries: {}", benchmarks.support_entries),
            format!("  ready_entries: {}", benchmarks.ready_entries),
        ]);
        if !benchmarks.missing_binaries.is_empty() {
            lines.push(format!(
                "  missing_binaries: {}",
                benchmarks.missing_binaries.join(", ")
            ));
        }
        if !benchmarks.missing_env_vars.is_empty() {
            lines.push(format!(
                "  missing_env_vars: {}",
                benchmarks.missing_env_vars.join(", ")
            ));
        }
    }

    lines.push(String::new());
    lines.push("next actions:".to_string());
    if snapshot.next_actions.is_empty() {
        lines.push("  - none".to_string());
    } else {
        lines.extend(
            snapshot
                .next_actions
                .iter()
                .map(|action| format!("  - {action}")),
        );
    }

    lines.join("\n")
}

fn render_context_pack(pack: &ContextPack) -> String {
    let mut lines = vec![
        "litkg context pack".to_string(),
        format!("task: {}", pack.task),
        format!("profile: {}", pack.profile),
        format!("budget_tokens: {}", pack.budget_tokens),
        format!("truncated: {}", yes_no(pack.truncated)),
        String::new(),
        "action plan:".to_string(),
    ];
    for action in &pack.action_plan {
        lines.push(format!("  - {action}"));
    }
    lines.extend([String::new(), "active backlog:".to_string()]);
    for item in &pack.active_backlog {
        let issue_suffix = if item.issue_ids.is_empty() {
            String::new()
        } else {
            format!(" ({})", item.issue_ids.join(", "))
        };
        lines.push(format!(
            "  - {} [{}/{}] {}{}",
            item.id, item.priority, item.status, item.title, issue_suffix
        ));
        if !item.summary.trim().is_empty() {
            lines.push(format!("    summary: {}", item.summary));
        }
        if !item.context.is_empty() {
            lines.push(format!("    context: {}", item.context.join(" | ")));
        }
        if !item.references.is_empty() {
            lines.push(format!("    refs: {}", item.references.join(", ")));
        }
    }
    lines.extend([String::new(), "active issues:".to_string()]);
    for issue in &pack.active_issues {
        lines.push(format!(
            "  - {} [{}/{}] {}",
            issue.id, issue.priority, issue.status, issue.title
        ));
    }
    lines.push(String::new());
    lines.push("active todos:".to_string());
    for todo in &pack.active_todos {
        let issue_suffix = if todo.issue_ids.is_empty() {
            String::new()
        } else {
            format!(" ({})", todo.issue_ids.join(", "))
        };
        lines.push(format!(
            "  - {} [{}/{}] {}{}",
            todo.id, todo.priority, todo.status, todo.title, issue_suffix
        ));
    }
    lines.push(String::new());
    lines.push("evidence spans:".to_string());
    for span in &pack.evidence_spans {
        lines.push(format!(
            "  - {}:{}-{} [{}]",
            span.source_path, span.line_start, span.line_end, span.kind
        ));
        lines.push(indent_block(&span.text, "    "));
    }
    lines.push(String::new());
    lines.push("relevant symbols:".to_string());
    for symbol in &pack.relevant_symbols {
        lines.push(format!(
            "  - {} {} at {} ({})",
            symbol.kind, symbol.name, symbol.path, symbol.reason
        ));
    }
    lines.push(String::new());
    lines.push("relevant papers:".to_string());
    for paper in &pack.relevant_papers {
        lines.push(format!(
            "  - {} {} ({})",
            paper.paper_id,
            paper.title,
            paper.year.as_deref().unwrap_or("n/a")
        ));
    }
    lines.push(String::new());
    lines.push("missing leaves:".to_string());
    for leaf in &pack.missing_leaves {
        lines.push(format!(
            "  - {} [{}]: {} -> {}",
            leaf.provider, leaf.status, leaf.query, leaf.resolution_command
        ));
    }
    lines.push(String::new());
    lines.push("risk flags:".to_string());
    for flag in &pack.risk_flags {
        lines.push(format!("  - {flag}"));
    }
    lines.push(String::new());
    lines.push("backend status:".to_string());
    for backend in &pack.backend_status {
        lines.push(format!(
            "  - {} [{} / {}]: {}",
            backend.name,
            state_label(&backend.state),
            recommendation_label(&backend.agent_recommendation),
            backend.repair_command.as_deref().unwrap_or("n/a")
        ));
    }
    lines.push(String::new());
    lines.push("verification commands:".to_string());
    for command in &pack.verification_commands {
        lines.push(format!("  - {command}"));
    }
    lines.join("\n")
}

fn indent_block(text: &str, prefix: &str) -> String {
    text.lines()
        .map(|line| format!("{prefix}{line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn state_label(state: &CapabilityState) -> &'static str {
    match state {
        CapabilityState::Ready => "ready",
        CapabilityState::Generated => "generated",
        CapabilityState::Stale => "stale",
        CapabilityState::Configured => "configured",
        CapabilityState::Implemented => "implemented",
        CapabilityState::Missing => "missing",
        CapabilityState::NotChecked => "not checked",
        CapabilityState::Unavailable => "unavailable",
    }
}

fn recommendation_label(recommendation: &AgentRecommendation) -> &'static str {
    match recommendation {
        AgentRecommendation::UseNow => "use_now",
        AgentRecommendation::RefreshFirst => "refresh_first",
        AgentRecommendation::MissingLeaf => "missing_leaf",
        AgentRecommendation::DoNotUse => "do_not_use",
    }
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

    let mut table = Table::new();
    table.set_header(vec![
        "Index", "Title", "ID", "Year", "Status", "Score", "Assets", "Matches",
    ]);

    for (index, hit) in results.hits.iter().enumerate() {
        let assets = format!(
            "tex={} pdf={}",
            yes_no(hit.has_local_tex),
            yes_no(hit.has_local_pdf)
        );
        table.add_row(vec![
            (index + 1).to_string(),
            hit.title.clone(),
            hit.paper_id.clone(),
            hit.year.clone().unwrap_or_else(|| "n/a".to_string()),
            format!("{:?}", hit.parse_status),
            hit.score.to_string(),
            assets,
            hit.matched_fields.join(", "),
        ]);

        // Add optional detail rows if they exist
        if !hit.relevance_tags.is_empty() {
            table.add_row(vec![
                "".to_string(),
                format!("Tags: {}", hit.relevance_tags.join(", ")),
                "".to_string(),
                "".to_string(),
                "".to_string(),
                "".to_string(),
                "".to_string(),
                "".to_string(),
            ]);
        }
        if let Some(snippet) = &hit.snippet {
            table.add_row(vec![
                "".to_string(),
                format!("Snippet: {}", snippet),
                "".to_string(),
                "".to_string(),
                "".to_string(),
                "".to_string(),
                "".to_string(),
                "".to_string(),
            ]);
        }
    }

    lines.push(table.to_string());
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
        format!(
            "  parsed_json: {}",
            inspection
                .parsed_json_path
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "n/a".to_string())
        ),
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

fn parse_render_format(raw: &str) -> Result<AutoResearchRenderFormat> {
    match raw {
        "markdown" => Ok(AutoResearchRenderFormat::Markdown),
        "json" => Ok(AutoResearchRenderFormat::Json),
        "github-issue" => Ok(AutoResearchRenderFormat::GithubIssue),
        other => anyhow::bail!("Unsupported autoresearch target format `{other}`"),
    }
}

fn parse_output_mode(raw: &str) -> Result<OutputMode> {
    match raw {
        "text" => Ok(OutputMode::Text),
        "json" => Ok(OutputMode::Json),
        other => anyhow::bail!("Unsupported output format `{other}`"),
    }
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
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

fn print_write_summaries(
    action: &str,
    summaries: &[SinkWriteSummary],
    mode: OutputMode,
    verbose_paths: bool,
) -> Result<()> {
    match mode {
        OutputMode::Text => println!(
            "{}",
            render_write_summaries(action, summaries, verbose_paths)
        ),
        OutputMode::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&render_write_summaries_json(
                    action,
                    summaries,
                    verbose_paths,
                ))?
            );
        }
    }
    Ok(())
}

fn render_write_summaries(
    action: &str,
    summaries: &[SinkWriteSummary],
    verbose_paths: bool,
) -> String {
    let mut lines = vec![format!(
        "Completed `{action}` with {} output set(s):",
        summaries.len()
    )];
    for summary in summaries {
        let extension_counts = extension_counts(&summary.written_paths)
            .into_iter()
            .map(|(extension, count)| format!("{extension}={count}"))
            .collect::<Vec<_>>()
            .join(", ");
        lines.push(format!(
            "- {}: {} file(s) under {}{}",
            summary.kind,
            summary.written_paths.len(),
            summary.root.display(),
            if extension_counts.is_empty() {
                String::new()
            } else {
                format!(" ({extension_counts})")
            }
        ));
        if verbose_paths {
            for path in &summary.written_paths {
                lines.push(format!("  {}", path.display()));
            }
        }
    }
    lines.join("\n")
}

fn render_write_summaries_json(
    action: &str,
    summaries: &[SinkWriteSummary],
    verbose_paths: bool,
) -> serde_json::Value {
    serde_json::json!({
        "action": action,
        "outputs": summaries
            .iter()
            .map(|summary| {
                let extension_counts = extension_counts(&summary.written_paths);
                serde_json::json!({
                    "kind": summary.kind,
                    "root": summary.root,
                    "file_count": summary.written_paths.len(),
                    "extension_counts": extension_counts,
                    "written_paths": if verbose_paths {
                        serde_json::Value::Array(
                            summary
                                .written_paths
                                .iter()
                                .map(|path| serde_json::Value::String(path.display().to_string()))
                                .collect()
                        )
                    } else {
                        serde_json::Value::Null
                    },
                })
            })
            .collect::<Vec<_>>(),
    })
}

fn extension_counts(paths: &[PathBuf]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for path in paths {
        let key = path
            .extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.to_ascii_lowercase())
            .unwrap_or_else(|| "[none]".to_string());
        *counts.entry(key).or_default() += 1;
    }
    counts
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_text_write_summary_without_paths() {
        let summaries = vec![SinkWriteSummary {
            kind: "graphify",
            root: PathBuf::from("/tmp/generated"),
            written_paths: vec![
                PathBuf::from("/tmp/generated/a.md"),
                PathBuf::from("/tmp/generated/index.md"),
                PathBuf::from("/tmp/generated/graphify-manifest.json"),
            ],
        }];

        let rendered = render_write_summaries("materialize", &summaries, false);

        assert!(rendered.contains("Completed `materialize`"));
        assert!(rendered.contains("graphify: 3 file(s) under /tmp/generated (json=1, md=2)"));
        assert!(!rendered.contains("/tmp/generated/a.md"));
    }

    #[test]
    fn renders_json_write_summary_with_paths() {
        let summaries = vec![SinkWriteSummary {
            kind: "neo4j",
            root: PathBuf::from("/tmp/export"),
            written_paths: vec![
                PathBuf::from("/tmp/export/nodes.jsonl"),
                PathBuf::from("/tmp/export/edges.jsonl"),
            ],
        }];

        let rendered = render_write_summaries_json("export-neo4j", &summaries, true);

        assert_eq!(rendered["action"], "export-neo4j");
        assert_eq!(rendered["outputs"][0]["kind"], "neo4j");
        assert_eq!(rendered["outputs"][0]["file_count"], 2);
        assert_eq!(rendered["outputs"][0]["extension_counts"]["jsonl"], 2);
        assert_eq!(
            rendered["outputs"][0]["written_paths"][0],
            "/tmp/export/nodes.jsonl"
        );
    }

    #[test]
    fn parses_kg_visualize_filter_options() {
        let cli = Cli::try_parse_from([
            "litkg",
            "kg",
            "visualize",
            "--config",
            "repo.toml",
            "--modality",
            "code",
            "--modality",
            "generated-context",
            "--exclude-modality",
            "literature",
            "--entry",
            "VinPrediction",
            "--focus-depth",
            "1",
        ])
        .unwrap();

        let Commands::Kg(KgCommand::Visualize(args)) = cli.command else {
            panic!("expected kg visualize command");
        };
        assert_eq!(args.config.config, "repo.toml");
        assert_eq!(
            args.modalities,
            vec![GraphModalityArg::Code, GraphModalityArg::GeneratedContext]
        );
        assert_eq!(args.exclude_modalities, vec![GraphModalityArg::Literature]);
        assert_eq!(args.entry.as_deref(), Some("VinPrediction"));
        assert_eq!(args.focus_depth, 1);
    }
}
