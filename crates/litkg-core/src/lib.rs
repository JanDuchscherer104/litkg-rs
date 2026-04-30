pub mod benchmark;
pub mod benchmark_runner;
pub mod bibtex;
pub mod config;
pub mod download;
pub mod enrich;
pub mod identity;
pub mod inspect;
pub mod manifest;
pub mod markdown;
pub mod materialize;
pub mod memory;
pub mod model;
pub mod notebook;
pub mod registry;
pub mod schema;
pub mod semantic_scholar;
pub mod tabular;
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
pub use config::{default_semantic_scholar_fields, RepoConfig, SemanticScholarConfig, SinkMode};
pub use download::{download_registry_sources, DownloadOptions};
pub use enrich::{infer_enriched_edges, EnrichedEdge, EnrichedEdgeType, EnrichmentStrategy};
pub use identity::{
    normalize_arxiv_id, normalize_author_name, normalize_doi, normalize_title, IdentityResolver,
    ResolutionCandidate,
};
pub use inspect::{
    compute_corpus_stats, compute_repo_capabilities, inspect_paper, search_papers,
    BenchmarkCapability, CapabilityOptions, CapabilityState, CorpusStats, DownloadCapability,
    GraphOutputCapability, LiteratureRegistryCapability, PaperInspection, PaperReference,
    ParsingCapability, ProjectMemoryCapability, RepoCapabilitySnapshot, RuntimeCapability,
    RuntimeCheck, SearchHit, SearchResults, SectionHeading, SemanticScholarCapability,
};
pub use manifest::{load_manifest, ManifestEntry};
pub use markdown::parse_markdown_document;
pub use materialize::{
    emit_markdown, ingest_configured_sources, ingest_markdown_docs, load_parsed_papers,
    matched_relevance_tags, write_materialized_doc, write_parsed_papers,
};
pub use memory::{
    load_project_memory, MemoryChunkKind, MemoryImportBundle, MemoryNode, MemoryNodeKind,
    MemoryRelation, MemoryRelationType, MemorySurface, MemorySurfaceKind,
};
pub use model::{
    CitationReference, DocumentKind, DownloadMode, MaterializedDoc, NotebookCell, NotebookCellKind,
    NotebookDocument, PaperFigure, PaperSection, PaperSourceRecord, PaperTable, ParseStatus,
    ParsedPaper, ResearchMetadata, ResearchPaper, SemanticScholarAuthor,
    SemanticScholarFieldOfStudy, SemanticScholarOpenAccessPdf, SemanticScholarPaper,
    SemanticScholarTldr, SourceKind,
};
pub use notebook::{
    ingest_notebooks_for_research_papers, load_notebook_documents, NotebookIngestStats,
};
pub use registry::{build_registry_snapshot, load_registry, sync_registry, write_registry};
pub use schema::{
    Alias, CanonicalEdge, CanonicalNode, Conflict, ConflictKind, EdgeKind, MergeDecision,
    MergeReason, NodeKind, Provenance, ProvenanceSpan, StableId,
};
pub use semantic_scholar::{
    enrich_registry_with_semantic_scholar, enrich_registry_with_semantic_scholar_client,
    semantic_scholar_identifier, SemanticScholarBatchResponse, SemanticScholarClient,
    SemanticScholarMethod, SemanticScholarRecommendationResponse, SemanticScholarSearchRequest,
    SemanticScholarSearchResponse, SemanticScholarTransport, UreqSemanticScholarTransport,
};
pub use tabular::{
    build_tabular_bundle, build_tabular_bundle_from_parsed,
    build_tabular_bundle_from_parsed_with_notebooks, research_papers_from_parsed,
    write_tabular_exports, CitationTableRow, EdgeTableRow, NotebookCellTableRow, NotebookTableRow,
    PaperTableRow, SectionTableRow, TabularBundle, TabularExportPaths,
};
pub use tex::parse_registry_papers;
