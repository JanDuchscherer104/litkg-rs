pub mod benchmark;
pub mod bibtex;
pub mod config;
pub mod download;
pub mod inspect;
pub mod manifest;
pub mod materialize;
pub mod model;
pub mod registry;
pub mod tex;

pub use benchmark::{
    load_benchmark_catalog, load_benchmark_results, render_autoresearch_target,
    validate_benchmark_catalog, validate_benchmark_results, AutoResearchRenderFormat,
    AutoResearchTargetTemplate, BenchmarkCatalog, BenchmarkResults, BenchmarkRun, BenchmarkScore,
    BenchmarkSource, BenchmarkSpec, ValidationSummary,
};
pub use bibtex::{parse_bibtex, BibEntry};
pub use config::{RepoConfig, SinkMode};
pub use download::{download_registry_sources, DownloadOptions};
pub use inspect::{
    compute_corpus_stats, inspect_paper, search_papers, CorpusStats, PaperInspection,
    PaperReference, SearchHit, SectionHeading,
};
pub use manifest::{load_manifest, ManifestEntry};
pub use materialize::{
    emit_markdown, load_parsed_papers, matched_relevance_tags, write_materialized_doc,
    write_parsed_papers,
};
pub use model::{
    DownloadMode, MaterializedDoc, PaperFigure, PaperSection, PaperSourceRecord, PaperTable,
    ParseStatus, ParsedPaper, SourceKind,
};
pub use registry::{build_registry_snapshot, load_registry, sync_registry, write_registry};
pub use tex::parse_registry_papers;
