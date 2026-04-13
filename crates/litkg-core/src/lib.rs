pub mod benchmark;
pub mod benchmark_runner;
pub mod bibtex;
pub mod config;
pub mod download;
pub mod enrich;
pub mod manifest;
pub mod materialize;
pub mod model;
pub mod registry;
pub mod tex;

pub use benchmark::{
    load_benchmark_catalog, load_benchmark_results, promote_benchmark_results,
    render_autoresearch_target, render_promoted_targets, validate_benchmark_catalog,
    validate_benchmark_results, write_benchmark_results, AutoResearchRenderFormat,
    AutoResearchTargetTemplate, BenchmarkArtifact, BenchmarkCatalog, BenchmarkExecutionRecord,
    BenchmarkPromotionRequest, BenchmarkResults, BenchmarkRun, BenchmarkScore, BenchmarkSource,
    BenchmarkSpec, MetricThresholdComparison, MetricThresholdRule, PromotedAutoResearchTarget,
    PromotionComponentSelection, PromotionEvidence, ValidationSummary,
};
pub use benchmark_runner::{
    inspect_benchmark_support, load_benchmark_integrations, load_benchmark_run_plan,
    run_benchmarks, validate_benchmark_integrations, validate_benchmark_run_plan,
    BenchmarkIntegration, BenchmarkIntegrationCatalog, BenchmarkRunPlan, BenchmarkRunRequest,
    BenchmarkSupportStatus,
};
pub use bibtex::{parse_bibtex, BibEntry};
pub use config::{RepoConfig, SinkMode};
pub use download::{download_registry_sources, DownloadOptions};
pub use enrich::{infer_enriched_edges, EnrichedEdge, EnrichedEdgeType, EnrichmentStrategy};
pub use manifest::{load_manifest, ManifestEntry};
pub use materialize::{
    emit_markdown, load_parsed_papers, matched_relevance_tags, write_materialized_doc,
    write_parsed_papers,
};
pub use model::{
    DownloadMode, MaterializedDoc, PaperFigure, PaperSection, PaperSourceRecord, PaperTable,
    ParseStatus, ParsedPaper, SourceKind,
};
pub use registry::{load_registry, sync_registry, write_registry};
pub use tex::parse_registry_papers;
