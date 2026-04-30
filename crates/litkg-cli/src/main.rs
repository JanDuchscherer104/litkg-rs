use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use litkg_core::{
    build_context_pack, build_registry_snapshot, compute_corpus_stats, compute_repo_capabilities,
    download_registry_sources, enrich_registry_with_semantic_scholar, inspect_benchmark_support,
    inspect_paper, load_parsed_papers, load_registry, parse_registry_papers,
    promote_benchmark_results, render_promoted_targets, run_benchmarks, search_papers,
    sync_registry, validate_benchmark_catalog, validate_benchmark_results, write_benchmark_results,
    write_parsed_papers, AutoResearchRenderFormat, BenchmarkPromotionRequest, BenchmarkResults,
    BenchmarkSupportStatus, CapabilityOptions, CapabilityState, ContextPack, ContextPackRequest,
    CorpusStats, DownloadOptions, MetricThresholdComparison, MetricThresholdRule, PaperInspection,
    PaperSourceRecord, ParsedPaper, PromotionComponentSelection, RepoCapabilitySnapshot,
    RepoConfig, RuntimeCheck, SearchResults, SemanticScholarClient, SemanticScholarConfig,
    SemanticScholarPaper, SemanticScholarSearchRequest, SinkMode,
};
use litkg_graphify::GraphifySink;
use litkg_neo4j::Neo4jSink;
use litkg_viewer::run_bundle as run_viewer_bundle;
use std::collections::BTreeMap;
use std::path::PathBuf;
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
    IngestDocs(IngestDocsCommand),
    IngestConfiguredSources(ConfigArg),
    Download(DownloadCommand),
    Parse(ConfigArg),
    Materialize(WriteCommand),
    RebuildGraph(ConfigArg),
    Pipeline(DownloadCommand),
    ExportNeo4j(WriteCommand),
    InspectGraph(ConfigArg),
    Capabilities(CapabilitiesCommand),
    ContextPack(ContextPackCommand),
    Stats(StatsCommand),
    Search(SearchCommand),
    ShowPaper(ShowPaperCommand),
    EnrichSemanticScholar(SemanticScholarEnrichCommand),
    SemanticScholarSearch(SemanticScholarSearchCommand),
    SemanticScholarPaper(SemanticScholarPaperCommand),
    SemanticScholarRecommend(SemanticScholarRecommendCommand),
    ValidateBenchmarks(BenchmarkCatalogArg),
    BenchmarkSupport(BenchmarkSupportCommand),
    RunBenchmarks(BenchmarkRunCommand),
    RenderAutoresearchTarget(AutoResearchTargetCommand),
    SyncAutoresearchTargetIssue(AutoResearchIssueSyncCommand),
    PromoteBenchmarkResults(PromoteBenchmarkResultsCommand),
}

#[derive(Args, Clone)]
struct ConfigArg {
    #[arg(long)]
    config: String,
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
        Commands::IngestDocs(args) => {
            let config = RepoConfig::load(&args.config.config)?;
            let docs = litkg_core::ingest_markdown_docs(
                &config,
                &PathBuf::from(&args.dir),
                args.recursive,
                args.kind.into(),
            )?;
            println!("Successfully ingested {} documents.", docs.len());
        }
        Commands::IngestConfiguredSources(args) => {
            let config = RepoConfig::load(&args.config)?;
            let docs = litkg_core::ingest_configured_sources(&config)?;
            println!(
                "Successfully ingested {} documents from configured sources.",
                docs.len()
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
            let config = RepoConfig::load(&args.config.config)?;
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
            let summaries = materialize(&config, &papers)?;
            print_write_summaries(
                "materialize",
                &summaries,
                parse_output_mode(&args.format)?,
                args.verbose_paths,
            )?;
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
            let config = RepoConfig::load(&args.config.config)?;
            let papers = litkg_core::load_parsed_papers(config.parsed_root())?;
            if papers.is_empty() && config.memory_state_root().is_none() {
                anyhow::bail!(
                    "No parsed papers found under {} and no memory_state_root was configured",
                    config.parsed_root().display()
                );
            }
            let summaries = vec![SinkWriteSummary {
                kind: "neo4j",
                root: config.neo4j_export_root(),
                written_paths: Neo4jSink::export(&config, &papers)?,
            }];
            print_write_summaries(
                "export-neo4j",
                &summaries,
                parse_output_mode(&args.format)?,
                args.verbose_paths,
            )?;
        }
        Commands::InspectGraph(args) => {
            let config = RepoConfig::load(&args.config)?;
            inspect_graph(&config)?;
        }
        Commands::Capabilities(args) => {
            let config = RepoConfig::load(&args.config.config)?;
            let snapshot = compute_repo_capabilities(
                &config,
                CapabilityOptions {
                    config_path: PathBuf::from(&args.config.config),
                    repo_root: args.repo_root.map(PathBuf::from),
                    benchmark_catalog: args.benchmark_catalog.map(PathBuf::from),
                    benchmark_integrations: args.benchmark_integrations.map(PathBuf::from),
                    check_runtime: args.check_runtime,
                },
            )?;
            print_structured_output(&snapshot, args.format, render_capabilities)?;
        }
        Commands::ContextPack(args) => {
            let config = RepoConfig::load(&args.config.config)?;
            let repo_root = match args.repo_root {
                Some(path) => PathBuf::from(path),
                None => std::env::current_dir().context("Failed to determine current directory")?,
            };
            let pack = build_context_pack(
                &config,
                ContextPackRequest {
                    repo_root,
                    task: args.task,
                    budget_tokens: args.budget,
                    profile: args.profile,
                },
            )?;
            print_structured_output(&pack, args.format, render_context_pack)?;
        }
        Commands::Stats(args) => {
            let config = RepoConfig::load(&args.config.config)?;
            let papers = load_parsed_papers(config.parsed_root())?;
            let registry = load_registry_or_sync(&config, &papers)?;
            let stats = compute_corpus_stats(&registry, &papers);
            print_structured_output(&stats, args.format, render_stats)?;
        }
        Commands::Search(args) => {
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
        Commands::ShowPaper(args) => {
            let config = RepoConfig::load(&args.config.config)?;
            let papers = load_parsed_papers(config.parsed_root())?;
            let registry = load_registry_or_sync(&config, &papers)?;
            let inspection = inspect_paper(&config, &registry, &papers, &args.paper_selector)?;
            print_structured_output(&inspection, args.format, render_paper_inspection)?;
        }
        Commands::EnrichSemanticScholar(args) => {
            let config = RepoConfig::load(&args.config.config)?;
            let registry = if config.registry_path().exists() {
                load_registry(config.registry_path())?
            } else {
                sync_registry(&config)?
            };
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
                    println!(
                        "{action} {enriched_count} of {} registry records with Semantic Scholar metadata",
                        updated.len()
                    );
                }
                OutputFormat::Json => println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "records": updated.len(),
                        "enriched": enriched_count,
                        "dry_run": args.dry_run,
                        "registry_path": config.registry_path(),
                    }))?
                ),
            }
        }
        Commands::SemanticScholarSearch(args) => {
            let semantic_config = load_semantic_scholar_config(args.config.as_deref())?;
            let fields = semantic_fields(&semantic_config, &args.fields);
            let mut client = SemanticScholarClient::from_config(semantic_config, None)?;
            let mut request = SemanticScholarSearchRequest::new(args.query, args.limit, fields);
            request.year = args.year;
            request.publication_date_or_year = args.publication_date_or_year;
            request.fields_of_study = args.fields_of_study;
            request.venue = args.venue;
            request.sort = args.sort;
            request.min_citation_count = args.min_citation_count;
            request.open_access_pdf = args.open_access_pdf;
            let papers = client.search_papers(&request)?;
            print_structured_output(&papers, args.format, |papers| {
                render_semantic_scholar_papers(papers)
            })?;
        }
        Commands::SemanticScholarPaper(args) => {
            let semantic_config = load_semantic_scholar_config(args.config.as_deref())?;
            let fields = semantic_fields(&semantic_config, &args.fields);
            let mut client = SemanticScholarClient::from_config(semantic_config, None)?;
            let paper = client.get_paper(&args.paper_id, &fields)?;
            print_structured_output(&paper, args.format, render_semantic_scholar_paper)?;
        }
        Commands::SemanticScholarRecommend(args) => {
            if args.positive_paper_ids.is_empty() {
                anyhow::bail!(
                    "semantic-scholar-recommend requires at least one --positive paper id"
                );
            }
            let semantic_config = load_semantic_scholar_config(args.config.as_deref())?;
            let fields = semantic_fields(&semantic_config, &args.fields);
            let mut client = SemanticScholarClient::from_config(semantic_config, None)?;
            let papers = client.recommend_papers(
                &args.positive_paper_ids,
                &args.negative_paper_ids,
                args.limit,
                &fields,
            )?;
            print_structured_output(&papers, args.format, |papers| {
                render_semantic_scholar_papers(papers)
            })?;
        }
        Commands::ValidateBenchmarks(args) => {
            let catalog = litkg_core::load_benchmark_catalog(&args.catalog.path)?;
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
        Commands::RunBenchmarks(args) => {
            let catalog = litkg_core::load_benchmark_catalog(&args.execution.catalog.path)?;
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
        Commands::SyncAutoresearchTargetIssue(args) => {
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
                let issue_url = create_github_issue(&repo, &title, &body, &args.labels)?;
                println!("{issue_url}");
            }
        }
        Commands::PromoteBenchmarkResults(args) => {
            let catalog = litkg_core::load_benchmark_catalog(&args.catalog.catalog.path)?;
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
    }
    Ok(())
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

fn render_capabilities(snapshot: &RepoCapabilitySnapshot) -> String {
    let mut lines = vec![
        "litkg capability snapshot".to_string(),
        format!("config: {}", snapshot.config_path.display()),
    ];
    if let Some(repo_root) = &snapshot.repo_root {
        lines.push(format!("repo_root: {}", repo_root.display()));
    }

    lines.extend([
        String::new(),
        "enabled:".to_string(),
        format!(
            "  literature registry: {}, {} record(s), manifest={}, bib={}, registry_generated={}",
            state_label(&snapshot.literature_registry.state),
            snapshot.literature_registry.records_total,
            yes_no(snapshot.literature_registry.manifest_present),
            yes_no(snapshot.literature_registry.bib_present),
            yes_no(snapshot.literature_registry.registry_generated),
        ),
        format!(
            "  downloads: {}, arxiv_records={}, local_tex={}, local_pdf={}, download_pdfs={}",
            state_label(&snapshot.downloads.state),
            snapshot.downloads.arxiv_records,
            snapshot.downloads.records_with_local_tex,
            snapshot.downloads.records_with_local_pdf,
            yes_no(snapshot.downloads.download_pdfs_configured),
        ),
        format!(
            "  tex parsing: {}, parsed={}, structured={}, sections={}, citations={}, citation_refs={}",
            state_label(&snapshot.parsing.state),
            snapshot.parsing.parsed_papers,
            snapshot.parsing.papers_with_structured_content,
            snapshot.parsing.total_sections,
            snapshot.parsing.total_citations,
            snapshot.parsing.total_citation_references,
        ),
        format!(
            "  graphify: {}, configured={}, index={}, manifest={}",
            state_label(&snapshot.graph_outputs.graphify_state),
            yes_no(snapshot.graph_outputs.graphify_configured),
            yes_no(snapshot.graph_outputs.graphify_index_generated),
            yes_no(snapshot.graph_outputs.graphify_manifest_generated),
        ),
        format!(
            "  neo4j export: {}, configured={}, nodes={}, edges={}",
            state_label(&snapshot.graph_outputs.neo4j_state),
            yes_no(snapshot.graph_outputs.neo4j_configured),
            yes_no(snapshot.graph_outputs.neo4j_nodes_generated),
            yes_no(snapshot.graph_outputs.neo4j_edges_generated),
        ),
        format!(
            "  native viewer: {}",
            state_label(&snapshot.graph_outputs.native_viewer_state)
        ),
        format!(
            "  semantic scholar: {}, configured={}, key_env={}, key_present={}, enriched={}",
            state_label(&snapshot.semantic_scholar.state),
            yes_no(snapshot.semantic_scholar.configured),
            snapshot.semantic_scholar.api_key_env,
            yes_no(snapshot.semantic_scholar.api_key_present),
            snapshot.semantic_scholar.enriched_records,
        ),
        format!(
            "  project memory: {}, configured={}, root_present={}, nodes={}, surfaces={}",
            state_label(&snapshot.project_memory.state),
            yes_no(snapshot.project_memory.configured),
            yes_no(snapshot.project_memory.root_present),
            snapshot.project_memory.imported_nodes,
            snapshot.project_memory.imported_surfaces,
        ),
    ]);

    lines.extend([
        String::new(),
        "runtime:".to_string(),
        format!("  checked: {}", yes_no(snapshot.runtime.checked)),
        render_runtime_check("docker", &snapshot.runtime.docker),
        render_runtime_check("neo4j_service", &snapshot.runtime.neo4j_service),
        render_runtime_check("uv", &snapshot.runtime.uv),
        render_runtime_check("codegraphcontext", &snapshot.runtime.codegraphcontext),
        render_runtime_check("graphiti_helpers", &snapshot.runtime.graphiti_helpers),
        render_runtime_check("ollama_service", &snapshot.runtime.ollama_service),
    ]);

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
        "active issues:".to_string(),
    ];
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
    lines.push("missing context leaves:".to_string());
    for leaf in &pack.missing_context_leaves {
        lines.push(format!(
            "  - {} [{}]: {}",
            leaf.provider, leaf.status, leaf.query
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

fn render_runtime_check(name: &str, check: &RuntimeCheck) -> String {
    format!("  {name}: {}, {}", state_label(&check.state), check.detail)
}

fn state_label(state: &CapabilityState) -> &'static str {
    match state {
        CapabilityState::Ready => "ready",
        CapabilityState::Generated => "generated",
        CapabilityState::Configured => "configured",
        CapabilityState::Implemented => "implemented",
        CapabilityState::Missing => "missing",
        CapabilityState::NotChecked => "not checked",
        CapabilityState::Unavailable => "unavailable",
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

fn render_count_map(counts: &BTreeMap<String, usize>) -> Vec<String> {
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
}
