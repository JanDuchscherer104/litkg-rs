use crate::config::RepoConfig;
use crate::materialize::matched_relevance_tags;
use crate::memory::load_project_memory;
use crate::model::{PaperSourceRecord, ParseStatus, ParsedPaper};
use crate::registry::{build_registry_snapshot, load_registry};
use crate::{load_parsed_papers, SinkMode};
use anyhow::{bail, Result};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CorpusStats {
    pub total_papers: usize,
    pub papers_with_parsed_content: usize,
    pub papers_with_local_tex: usize,
    pub papers_with_local_pdf: usize,
    pub total_sections: usize,
    pub total_figures: usize,
    pub total_tables: usize,
    pub total_citations: usize,
    pub source_kind_counts: BTreeMap<String, usize>,
    pub download_mode_counts: BTreeMap<String, usize>,
    pub parse_status_counts: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SearchHit {
    pub paper_id: String,
    pub citation_key: Option<String>,
    pub title: String,
    pub year: Option<String>,
    pub parse_status: ParseStatus,
    pub has_local_tex: bool,
    pub has_local_pdf: bool,
    pub score: u32,
    pub matched_fields: Vec<String>,
    pub snippet: Option<String>,
    pub relevance_tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SearchResults {
    pub query: String,
    pub limit: usize,
    pub total_matches: usize,
    pub has_more: bool,
    pub hits: Vec<SearchHit>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PaperReference {
    pub paper_id: String,
    pub citation_key: Option<String>,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SectionHeading {
    pub level: u8,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PaperInspection {
    pub metadata: PaperSourceRecord,
    pub parsed_json_path: Option<PathBuf>,
    pub materialized_markdown_path: Option<PathBuf>,
    pub local_tex_dir: Option<PathBuf>,
    pub local_pdf_path: Option<PathBuf>,
    pub abstract_text: Option<String>,
    pub sections: Vec<SectionHeading>,
    pub figure_captions: Vec<String>,
    pub table_captions: Vec<String>,
    pub citations: Vec<String>,
    pub cited_by: Vec<PaperReference>,
    pub provenance: Vec<String>,
    pub relevance_tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityState {
    Ready,
    Generated,
    Stale,
    Configured,
    Implemented,
    Missing,
    NotChecked,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CapabilityOptions {
    pub config_path: PathBuf,
    pub repo_root: Option<PathBuf>,
    pub benchmark_catalog: Option<PathBuf>,
    pub benchmark_integrations: Option<PathBuf>,
    pub check_runtime: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RepoCapabilitySnapshot {
    pub config_path: PathBuf,
    pub repo_root: Option<PathBuf>,
    pub conformance: ConformanceReport,
    pub literature_registry: LiteratureRegistryCapability,
    pub downloads: DownloadCapability,
    pub parsing: ParsingCapability,
    pub graph_outputs: GraphOutputCapability,
    pub semantic_scholar: SemanticScholarCapability,
    pub project_memory: ProjectMemoryCapability,
    pub runtime: RuntimeCapability,
    pub benchmarks: Option<BenchmarkCapability>,
    pub next_actions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentRecommendation {
    UseNow,
    RefreshFirst,
    MissingLeaf,
    DoNotUse,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SourceDescriptor {
    pub name: String,
    pub kind: String,
    pub configured_paths: Vec<String>,
    pub supported_node_types: Vec<String>,
    pub supported_edge_types: Vec<String>,
    pub freshness_inputs: Vec<String>,
    pub required_env: Vec<String>,
    pub required_tools: Vec<String>,
    pub state: CapabilityState,
    pub agent_recommendation: AgentRecommendation,
    pub repair_command: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct BackendDescriptor {
    pub name: String,
    pub kind: String,
    pub configured: bool,
    pub output_paths: Vec<String>,
    pub supported_node_types: Vec<String>,
    pub supported_edge_types: Vec<String>,
    pub required_env: Vec<String>,
    pub required_tools: Vec<String>,
    pub state: CapabilityState,
    pub agent_recommendation: AgentRecommendation,
    pub repair_command: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CapabilityProbe {
    pub name: String,
    pub target_type: String,
    pub state: CapabilityState,
    pub agent_recommendation: AgentRecommendation,
    pub detail: String,
    pub repair_command: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ConformanceReport {
    pub sources: Vec<SourceDescriptor>,
    pub backends: Vec<BackendDescriptor>,
    pub probes: Vec<CapabilityProbe>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct LiteratureRegistryCapability {
    pub state: CapabilityState,
    pub manifest_present: bool,
    pub bib_present: bool,
    pub registry_generated: bool,
    pub registry_path: PathBuf,
    pub records_total: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DownloadCapability {
    pub state: CapabilityState,
    pub arxiv_records: usize,
    pub records_with_local_tex: usize,
    pub records_with_local_pdf: usize,
    pub download_pdfs_configured: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ParsingCapability {
    pub state: CapabilityState,
    pub parsed_root: PathBuf,
    pub parsed_papers: usize,
    pub papers_with_structured_content: usize,
    pub total_sections: usize,
    pub total_figures: usize,
    pub total_tables: usize,
    pub total_citations: usize,
    pub total_citation_references: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct GraphOutputCapability {
    pub graphify_state: CapabilityState,
    pub graphify_configured: bool,
    pub graphify_index_generated: bool,
    pub graphify_manifest_generated: bool,
    pub neo4j_state: CapabilityState,
    pub neo4j_configured: bool,
    pub neo4j_export_root: PathBuf,
    pub neo4j_nodes_generated: bool,
    pub neo4j_edges_generated: bool,
    pub native_viewer_state: CapabilityState,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SemanticScholarCapability {
    pub state: CapabilityState,
    pub configured: bool,
    pub api_key_env: String,
    pub api_key_present: bool,
    pub enriched_records: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ProjectMemoryCapability {
    pub state: CapabilityState,
    pub configured: bool,
    pub root: Option<PathBuf>,
    pub root_present: bool,
    pub imported_nodes: usize,
    pub imported_surfaces: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RuntimeCapability {
    pub checked: bool,
    pub docker: RuntimeCheck,
    pub neo4j_service: RuntimeCheck,
    pub uv: RuntimeCheck,
    pub codegraphcontext: RuntimeCheck,
    pub graphiti_helpers: RuntimeCheck,
    pub ollama_service: RuntimeCheck,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RuntimeCheck {
    pub state: CapabilityState,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct BenchmarkCapability {
    pub state: CapabilityState,
    pub catalog_present: bool,
    pub integrations_present: bool,
    pub support_entries: usize,
    pub ready_entries: usize,
    pub missing_binaries: Vec<String>,
    pub missing_env_vars: Vec<String>,
}

pub fn compute_repo_capabilities(
    config: &RepoConfig,
    options: CapabilityOptions,
) -> Result<RepoCapabilitySnapshot> {
    let registry_path = config.registry_path();
    let registry_generated = registry_path.is_file();
    let parsed_root = config.parsed_root();
    let neo4j_export_root = config.neo4j_export_root();
    let manifest_present = config.manifest_path.is_file();
    let bib_present = config.bib_path.is_file();

    let registry =
        load_registry_snapshot(config, registry_generated, manifest_present, bib_present)?;
    let parsed_papers = load_parsed_papers(&parsed_root)?;
    let stats = compute_corpus_stats(&registry, &parsed_papers);

    let literature_registry = LiteratureRegistryCapability {
        state: if registry_generated {
            CapabilityState::Generated
        } else if manifest_present && bib_present {
            CapabilityState::Ready
        } else {
            CapabilityState::Missing
        },
        manifest_present,
        bib_present,
        registry_generated,
        registry_path: registry_path.clone(),
        records_total: registry.len(),
    };

    let downloads = DownloadCapability {
        state: if registry.is_empty() {
            CapabilityState::Missing
        } else if stats.papers_with_local_tex == registry.len()
            && (!config.download_pdfs
                || stats.papers_with_local_pdf == expected_pdf_count(&registry))
        {
            CapabilityState::Ready
        } else {
            CapabilityState::Configured
        },
        arxiv_records: registry
            .iter()
            .filter(|record| record.arxiv_id.is_some())
            .count(),
        records_with_local_tex: stats.papers_with_local_tex,
        records_with_local_pdf: stats.papers_with_local_pdf,
        download_pdfs_configured: config.download_pdfs,
    };

    let parsing = ParsingCapability {
        state: if stats.papers_with_parsed_content > 0 {
            CapabilityState::Generated
        } else if stats.papers_with_local_tex > 0 {
            CapabilityState::Configured
        } else {
            CapabilityState::Missing
        },
        parsed_root: parsed_root.clone(),
        parsed_papers: parsed_papers.len(),
        papers_with_structured_content: stats.papers_with_parsed_content,
        total_sections: stats.total_sections,
        total_figures: stats.total_figures,
        total_tables: stats.total_tables,
        total_citations: stats.total_citations,
        total_citation_references: parsed_papers
            .iter()
            .map(|paper| paper.citation_references.len())
            .sum(),
    };

    let graphify_configured = matches!(config.sink, SinkMode::Graphify | SinkMode::Both);
    let graphify_index_generated = config.generated_docs_root.join("index.md").is_file();
    let graphify_manifest_generated = config
        .generated_docs_root
        .join("graphify-manifest.json")
        .is_file();
    let neo4j_configured = matches!(config.sink, SinkMode::Neo4j | SinkMode::Both);
    let neo4j_nodes_generated = neo4j_export_root.join("nodes.jsonl").is_file();
    let neo4j_edges_generated = neo4j_export_root.join("edges.jsonl").is_file();
    let neo4j_generated = neo4j_nodes_generated && neo4j_edges_generated;
    let graph_outputs = GraphOutputCapability {
        graphify_state: if graphify_index_generated && graphify_manifest_generated {
            CapabilityState::Generated
        } else if graphify_configured {
            CapabilityState::Configured
        } else {
            CapabilityState::Unavailable
        },
        graphify_configured,
        graphify_index_generated,
        graphify_manifest_generated,
        neo4j_state: if neo4j_generated {
            CapabilityState::Generated
        } else if neo4j_configured {
            CapabilityState::Configured
        } else {
            CapabilityState::Unavailable
        },
        neo4j_configured,
        neo4j_export_root: neo4j_export_root.clone(),
        neo4j_nodes_generated,
        neo4j_edges_generated,
        native_viewer_state: if neo4j_generated {
            CapabilityState::Ready
        } else {
            CapabilityState::Missing
        },
    };

    let semantic_config = config.semantic_scholar_config();
    let semantic_configured = semantic_config.enabled;
    let api_key_present = env::var(&semantic_config.api_key_env)
        .ok()
        .is_some_and(|value| !value.is_empty());
    let enriched_records = registry
        .iter()
        .filter(|record| record.semantic_scholar.is_some())
        .count();
    let semantic_scholar = SemanticScholarCapability {
        state: if !semantic_configured {
            CapabilityState::Unavailable
        } else if enriched_records > 0 {
            CapabilityState::Generated
        } else if api_key_present {
            CapabilityState::Ready
        } else {
            CapabilityState::Configured
        },
        configured: semantic_configured,
        api_key_env: semantic_config.api_key_env,
        api_key_present,
        enriched_records,
    };

    let memory_root = config.memory_state_root();
    let memory_root_present = memory_root.as_ref().is_some_and(|root| root.is_dir());
    let memory_bundle = if memory_root_present {
        load_project_memory(config, &parsed_papers).ok()
    } else {
        None
    };
    let project_memory = ProjectMemoryCapability {
        state: if memory_bundle.is_some() {
            CapabilityState::Ready
        } else if memory_root.is_some() {
            CapabilityState::Configured
        } else {
            CapabilityState::Unavailable
        },
        configured: memory_root.is_some(),
        root: memory_root,
        root_present: memory_root_present,
        imported_nodes: memory_bundle
            .as_ref()
            .map(|bundle| bundle.nodes.len())
            .unwrap_or_default(),
        imported_surfaces: memory_bundle
            .as_ref()
            .map(|bundle| bundle.surfaces.len())
            .unwrap_or_default(),
    };

    let runtime = inspect_runtime(options.repo_root.as_deref(), options.check_runtime);
    let benchmarks = inspect_benchmark_capability(
        options.benchmark_catalog.as_deref(),
        options.benchmark_integrations.as_deref(),
    );
    let conformance = compute_agent_conformance_report(
        config,
        &options.config_path,
        options.repo_root.as_deref(),
        options.check_runtime,
    );
    let next_actions = next_actions(
        &options.config_path,
        &literature_registry,
        &downloads,
        &parsing,
        &graph_outputs,
        &semantic_scholar,
    );

    Ok(RepoCapabilitySnapshot {
        config_path: options.config_path,
        repo_root: options.repo_root,
        conformance,
        literature_registry,
        downloads,
        parsing,
        graph_outputs,
        semantic_scholar,
        project_memory,
        runtime,
        benchmarks,
        next_actions,
    })
}

pub fn compute_agent_conformance_report(
    config: &RepoConfig,
    config_path: &Path,
    repo_root: Option<&Path>,
    check_runtime: bool,
) -> ConformanceReport {
    let mut sources = configured_source_descriptors(config, repo_root);
    sources.push(literature_source_descriptor(config));
    sources.sort_by(|left, right| left.name.cmp(&right.name));

    let runtime = inspect_runtime(repo_root, check_runtime);
    let mut backends = vec![
        graphify_backend_descriptor(config, config_path),
        neo4j_backend_descriptor(config, config_path),
        code_index_backend_descriptor(config, repo_root, &runtime),
        graphiti_backend_descriptor(config, repo_root, &runtime),
        context7_backend_descriptor(config),
        openai_docs_backend_descriptor(),
        semantic_scholar_backend_descriptor(config, config_path),
        mempalace_backend_descriptor(config, repo_root),
    ];
    backends.sort_by(|left, right| left.name.cmp(&right.name));

    let probes = sources
        .iter()
        .map(|source| CapabilityProbe {
            name: source.name.clone(),
            target_type: "source".into(),
            state: source.state.clone(),
            agent_recommendation: source.agent_recommendation.clone(),
            detail: format!("{} source contract", source.kind),
            repair_command: source.repair_command.clone(),
        })
        .chain(backends.iter().map(|backend| CapabilityProbe {
            name: backend.name.clone(),
            target_type: "backend".into(),
            state: backend.state.clone(),
            agent_recommendation: backend.agent_recommendation.clone(),
            detail: format!("{} backend contract", backend.kind),
            repair_command: backend.repair_command.clone(),
        }))
        .collect();

    ConformanceReport {
        sources,
        backends,
        probes,
    }
}

fn configured_source_descriptors(
    config: &RepoConfig,
    repo_root: Option<&Path>,
) -> Vec<SourceDescriptor> {
    config
        .sources
        .iter()
        .map(|(name, source)| {
            let configured_paths = source_paths(source);
            let missing_required_paths = configured_paths
                .iter()
                .filter(|raw| is_exact_path(raw))
                .filter(|raw| {
                    let path = repo_root
                        .map(|root| root.join(raw))
                        .unwrap_or_else(|| PathBuf::from(raw));
                    !path.exists()
                })
                .cloned()
                .collect::<Vec<_>>();
            let mut required_tools = Vec::new();
            if source.symbols || source.edges.as_deref() == Some("codegraphcontext") {
                required_tools.push("code-index/CodeGraphContext".into());
            }
            if !source.context7_libraries.is_empty() {
                required_tools.push("Context7 MCP".into());
            }
            if source.markitdown {
                required_tools.push("MarkItDown".into());
            }

            let state = if source.required && !missing_required_paths.is_empty() {
                CapabilityState::Missing
            } else if configured_paths.is_empty() && !source.enabled {
                CapabilityState::Configured
            } else {
                CapabilityState::Ready
            };
            let repair_command = if missing_required_paths.is_empty() {
                None
            } else {
                Some(format!(
                    "restore source paths: {}",
                    missing_required_paths.join(", ")
                ))
            };

            SourceDescriptor {
                name: name.clone(),
                kind: source_kind(name, source),
                configured_paths,
                supported_node_types: source_node_types(name, source),
                supported_edge_types: source_edge_types(name, source),
                freshness_inputs: source_freshness_inputs(source),
                required_env: Vec::new(),
                required_tools,
                agent_recommendation: recommendation_for_state(&state),
                state,
                repair_command,
            }
        })
        .collect()
}

fn literature_source_descriptor(config: &RepoConfig) -> SourceDescriptor {
    let configured_paths = vec![
        config.manifest_path.display().to_string(),
        config.bib_path.display().to_string(),
        config.tex_root.display().to_string(),
        config.pdf_root.display().to_string(),
    ];
    let missing = [&config.manifest_path, &config.bib_path]
        .into_iter()
        .filter(|path| !path.is_file())
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>();
    let state = if !missing.is_empty() {
        CapabilityState::Missing
    } else {
        CapabilityState::Ready
    };
    SourceDescriptor {
        name: "literature".into(),
        kind: "manifest_bib_tex_pdf".into(),
        configured_paths,
        supported_node_types: vec!["Paper".into(), "CitationMention".into()],
        supported_edge_types: vec!["cites".into(), "mentions".into()],
        freshness_inputs: vec![
            config.manifest_path.display().to_string(),
            config.bib_path.display().to_string(),
            config.tex_root.display().to_string(),
        ],
        required_env: Vec::new(),
        required_tools: Vec::new(),
        agent_recommendation: recommendation_for_state(&state),
        state,
        repair_command: if missing.is_empty() {
            None
        } else {
            Some(format!("restore literature inputs: {}", missing.join(", ")))
        },
    }
}

fn graphify_backend_descriptor(config: &RepoConfig, config_path: &Path) -> BackendDescriptor {
    let configured = matches!(config.sink, SinkMode::Graphify | SinkMode::Both)
        || config
            .backends
            .as_ref()
            .is_some_and(|backends| backends.graphify);
    let index_path = config.generated_docs_root.join("index.md");
    let manifest_path = config.generated_docs_root.join("graphify-manifest.json");
    let generated = index_path.is_file() && manifest_path.is_file();
    let state = if generated && stale_against_inputs(&[&index_path, &manifest_path], config) {
        CapabilityState::Stale
    } else if generated {
        CapabilityState::Generated
    } else if configured {
        CapabilityState::Configured
    } else {
        CapabilityState::Unavailable
    };
    BackendDescriptor {
        name: "graphify".into(),
        kind: "durable_static_graph_docs".into(),
        configured,
        output_paths: vec![
            index_path.display().to_string(),
            manifest_path.display().to_string(),
        ],
        supported_node_types: vec!["Paper".into(), "DocSection".into(), "MemoryNode".into()],
        supported_edge_types: vec!["cites".into(), "related_to".into()],
        required_env: Vec::new(),
        required_tools: Vec::new(),
        agent_recommendation: recommendation_for_state(&state),
        state,
        repair_command: configured.then(|| {
            format!(
                "cargo run -p litkg-cli -- kg build --config {}",
                config_path.display()
            )
        }),
    }
}

fn neo4j_backend_descriptor(config: &RepoConfig, config_path: &Path) -> BackendDescriptor {
    let configured = matches!(config.sink, SinkMode::Neo4j | SinkMode::Both)
        || config
            .backends
            .as_ref()
            .is_some_and(|backends| backends.neo4j_export);
    let nodes_path = config.neo4j_export_root().join("nodes.jsonl");
    let edges_path = config.neo4j_export_root().join("edges.jsonl");
    let generated = nodes_path.is_file() && edges_path.is_file();
    let state = if generated && stale_against_inputs(&[&nodes_path, &edges_path], config) {
        CapabilityState::Stale
    } else if generated {
        CapabilityState::Generated
    } else if configured {
        CapabilityState::Configured
    } else {
        CapabilityState::Unavailable
    };
    BackendDescriptor {
        name: "neo4j_export".into(),
        kind: "durable_neo4j_jsonl_export".into(),
        configured,
        output_paths: vec![
            nodes_path.display().to_string(),
            edges_path.display().to_string(),
        ],
        supported_node_types: vec!["Paper".into(), "DocSection".into(), "MemoryNode".into()],
        supported_edge_types: vec!["cites".into(), "imports".into(), "related_to".into()],
        required_env: Vec::new(),
        required_tools: Vec::new(),
        agent_recommendation: recommendation_for_state(&state),
        state,
        repair_command: configured.then(|| {
            format!(
                "cargo run -p litkg-cli -- kg export --config {}",
                config_path.display()
            )
        }),
    }
}

fn code_index_backend_descriptor(
    config: &RepoConfig,
    repo_root: Option<&Path>,
    runtime: &RuntimeCapability,
) -> BackendDescriptor {
    let configured = config
        .backends
        .as_ref()
        .is_some_and(|backends| backends.code_index)
        || config
            .sources
            .values()
            .any(|source| source.symbols || source.edges.as_deref() == Some("codegraphcontext"));
    let state = if !configured {
        CapabilityState::Unavailable
    } else if runtime.checked {
        runtime.codegraphcontext.state.clone()
    } else {
        CapabilityState::Configured
    };
    BackendDescriptor {
        name: "codegraphcontext".into(),
        kind: "code_symbol_index_adapter".into(),
        configured,
        output_paths: repo_root
            .map(|root| root.join(".cache/kg/venvs/cgc").display().to_string())
            .into_iter()
            .collect(),
        supported_node_types: vec![
            "CodeFile".into(),
            "CodeSymbol".into(),
            "SymbolSummary".into(),
        ],
        supported_edge_types: vec!["imports".into(), "calls".into()],
        required_env: Vec::new(),
        required_tools: vec!["code-index MCP".into(), "CodeGraphContext".into()],
        agent_recommendation: recommendation_for_state(&state),
        state,
        repair_command: configured.then(|| "make kg-index-code".into()),
    }
}

fn graphiti_backend_descriptor(
    config: &RepoConfig,
    repo_root: Option<&Path>,
    runtime: &RuntimeCapability,
) -> BackendDescriptor {
    let configured = config
        .backends
        .as_ref()
        .is_some_and(|backends| backends.graphiti)
        || config
            .representation
            .as_ref()
            .is_some_and(|repr| repr.optional_runtime.iter().any(|item| item == "graphiti"));
    let state = if !configured {
        CapabilityState::Unavailable
    } else if runtime.checked {
        runtime.graphiti_helpers.state.clone()
    } else {
        CapabilityState::Configured
    };
    BackendDescriptor {
        name: "graphiti".into(),
        kind: "optional_temporal_memory_runtime".into(),
        configured,
        output_paths: repo_root
            .map(|root| root.join(".cache/kg/graphiti").display().to_string())
            .into_iter()
            .collect(),
        supported_node_types: vec!["Decision".into(), "OpenQuestion".into(), "Claim".into()],
        supported_edge_types: vec!["supersedes".into(), "mentions".into()],
        required_env: Vec::new(),
        required_tools: vec!["Graphiti helpers".into()],
        agent_recommendation: recommendation_for_state(&state),
        state,
        repair_command: configured.then(|| "make kg-ingest-docs".into()),
    }
}

fn context7_backend_descriptor(config: &RepoConfig) -> BackendDescriptor {
    let libraries = config
        .sources
        .values()
        .flat_map(|source| source.context7_libraries.clone())
        .collect::<BTreeSet<_>>();
    let configured = !libraries.is_empty();
    let state = if configured {
        CapabilityState::Configured
    } else {
        CapabilityState::Unavailable
    };
    BackendDescriptor {
        name: "context7".into(),
        kind: "external_library_docs_leaf".into(),
        configured,
        output_paths: libraries.into_iter().collect(),
        supported_node_types: vec!["ExternalDocLeaf".into()],
        supported_edge_types: vec!["requires_external_provider".into()],
        required_env: Vec::new(),
        required_tools: vec!["Context7 MCP".into()],
        agent_recommendation: if configured {
            AgentRecommendation::MissingLeaf
        } else {
            AgentRecommendation::DoNotUse
        },
        state,
        repair_command: configured
            .then(|| "resolve configured Context7 libraries, then rerun litkg context-pack".into()),
    }
}

fn openai_docs_backend_descriptor() -> BackendDescriptor {
    BackendDescriptor {
        name: "openai_developer_docs".into(),
        kind: "external_openai_docs_leaf".into(),
        configured: true,
        output_paths: Vec::new(),
        supported_node_types: vec!["ExternalDocLeaf".into()],
        supported_edge_types: vec!["requires_external_provider".into()],
        required_env: Vec::new(),
        required_tools: vec!["openaiDeveloperDocs MCP".into()],
        state: CapabilityState::Configured,
        agent_recommendation: AgentRecommendation::MissingLeaf,
        repair_command: Some(
            "use openaiDeveloperDocs MCP for current OpenAI/Codex/MCP docs when relevant".into(),
        ),
    }
}

fn semantic_scholar_backend_descriptor(
    config: &RepoConfig,
    config_path: &Path,
) -> BackendDescriptor {
    let semantic = config.semantic_scholar_config();
    let key_present = env::var(&semantic.api_key_env)
        .ok()
        .is_some_and(|value| !value.is_empty());
    let state = if !semantic.enabled {
        CapabilityState::Unavailable
    } else if key_present {
        CapabilityState::Ready
    } else {
        CapabilityState::Configured
    };
    BackendDescriptor {
        name: "semantic_scholar".into(),
        kind: "academic_graph_rest_adapter".into(),
        configured: semantic.enabled,
        output_paths: vec![config.registry_path().display().to_string()],
        supported_node_types: vec!["Paper".into(), "Author".into()],
        supported_edge_types: vec!["cites".into(), "references".into()],
        required_env: if semantic.enabled {
            vec![semantic.api_key_env.clone()]
        } else {
            Vec::new()
        },
        required_tools: Vec::new(),
        agent_recommendation: recommendation_for_state(&state),
        state,
        repair_command: semantic.enabled.then(|| {
            format!(
                "export {}=... && cargo run -p litkg-cli -- s2 enrich --config {}",
                semantic.api_key_env,
                config_path.display()
            )
        }),
    }
}

fn mempalace_backend_descriptor(
    config: &RepoConfig,
    repo_root: Option<&Path>,
) -> BackendDescriptor {
    let configured = config
        .backends
        .as_ref()
        .is_some_and(|backends| backends.mempalace)
        || config
            .representation
            .as_ref()
            .and_then(|repr| repr.memory_backend.as_deref())
            .is_some_and(|backend| backend == "mempalace");
    let root = repo_root.map(|root| root.join(".agents/memory"));
    let state = if !configured {
        CapabilityState::Unavailable
    } else if root.as_ref().is_some_and(|path| path.is_dir()) {
        CapabilityState::Ready
    } else {
        CapabilityState::Configured
    };
    BackendDescriptor {
        name: "mempalace".into(),
        kind: "local_first_agent_memory_adapter".into(),
        configured,
        output_paths: root
            .as_ref()
            .map(|path| vec![path.display().to_string()])
            .unwrap_or_default(),
        supported_node_types: vec![
            "Decision".into(),
            "OpenQuestion".into(),
            "ActionItem".into(),
        ],
        supported_edge_types: vec!["mentions".into(), "depends_on".into()],
        required_env: Vec::new(),
        required_tools: vec!["mempalace-rs".into()],
        agent_recommendation: recommendation_for_state(&state),
        state,
        repair_command: configured.then(|| "make memory-mine".into()),
    }
}

fn source_paths(source: &crate::config::SourceConfig) -> Vec<String> {
    let mut paths = Vec::new();
    paths.extend(source.include.iter().cloned());
    paths.extend(
        source
            .entrypoints
            .iter()
            .map(|path| path.display().to_string()),
    );
    if let Some(path) = &source.manifest {
        paths.push(path.display().to_string());
    }
    if let Some(path) = &source.bib {
        paths.push(path.display().to_string());
    }
    if let Some(path) = &source.pdfs {
        paths.push(path.clone());
    }
    if let Some(path) = &source.tex {
        paths.push(path.clone());
    }
    paths.extend(source.urls.iter().cloned());
    paths.extend(source.context7_libraries.iter().cloned());
    paths.sort();
    paths.dedup();
    paths
}

fn source_kind(name: &str, source: &crate::config::SourceConfig) -> String {
    if !source.context7_libraries.is_empty() {
        "external_docs".into()
    } else if source.symbols || source.edges.as_deref() == Some("codegraphcontext") {
        "code".into()
    } else if source.markitdown || !source.urls.is_empty() {
        "remote_docs".into()
    } else if name.contains("agent") || name.contains("skill") {
        "agent_scaffold".into()
    } else if name.contains("doc") || name.contains("paper") {
        "documentation".into()
    } else {
        "local_source".into()
    }
}

fn source_node_types(name: &str, source: &crate::config::SourceConfig) -> Vec<String> {
    match source_kind(name, source).as_str() {
        "agent_scaffold" => vec![
            "AgentInstructionFile".into(),
            "AgentSkill".into(),
            "AgentBacklogIssue".into(),
            "AgentBacklogTodo".into(),
        ],
        "code" => vec![
            "CodeFile".into(),
            "CodeSymbol".into(),
            "SymbolSummary".into(),
        ],
        "external_docs" | "remote_docs" => vec!["ExternalDocLeaf".into(), "DocSection".into()],
        _ => vec!["Document".into(), "DocSection".into(), "Concept".into()],
    }
}

fn source_edge_types(name: &str, source: &crate::config::SourceConfig) -> Vec<String> {
    match source_kind(name, source).as_str() {
        "agent_scaffold" => vec!["routes_to".into(), "handles".into(), "has_todo".into()],
        "code" => vec!["imports".into(), "calls".into(), "defines".into()],
        "external_docs" | "remote_docs" => vec!["requires_external_provider".into()],
        _ => vec!["mentions".into(), "cites".into()],
    }
}

fn source_freshness_inputs(source: &crate::config::SourceConfig) -> Vec<String> {
    source_paths(source)
        .into_iter()
        .filter(|path| !path.starts_with("http://") && !path.starts_with("https://"))
        .collect()
}

fn is_exact_path(raw: &str) -> bool {
    !raw.starts_with("http://")
        && !raw.starts_with("https://")
        && !raw.starts_with('/')
        && !raw.contains('*')
        && !raw.contains('{')
        && !raw.contains('}')
}

fn stale_against_inputs(outputs: &[&Path], config: &RepoConfig) -> bool {
    let registry_path = config.registry_path();
    let parsed_root = config.parsed_root();
    let inputs = [
        config.manifest_path.as_path(),
        config.bib_path.as_path(),
        registry_path.as_path(),
        parsed_root.as_path(),
    ];
    let newest_input = inputs
        .into_iter()
        .filter_map(|path| {
            path.metadata()
                .and_then(|metadata| metadata.modified())
                .ok()
        })
        .max();
    let Some(newest_input) = newest_input else {
        return false;
    };
    outputs.iter().any(|output| {
        output
            .metadata()
            .and_then(|metadata| metadata.modified())
            .map(|modified| modified < newest_input)
            .unwrap_or(false)
    })
}

fn recommendation_for_state(state: &CapabilityState) -> AgentRecommendation {
    match state {
        CapabilityState::Ready | CapabilityState::Generated | CapabilityState::Implemented => {
            AgentRecommendation::UseNow
        }
        CapabilityState::Stale | CapabilityState::Configured | CapabilityState::NotChecked => {
            AgentRecommendation::RefreshFirst
        }
        CapabilityState::Missing => AgentRecommendation::MissingLeaf,
        CapabilityState::Unavailable => AgentRecommendation::DoNotUse,
    }
}

pub fn compute_corpus_stats(
    registry: &[PaperSourceRecord],
    parsed_papers: &[ParsedPaper],
) -> CorpusStats {
    let effective_records = effective_registry_records(registry, parsed_papers);
    let live_parsed = live_parsed_papers(&effective_records, parsed_papers);
    let mut source_kind_counts = BTreeMap::new();
    let mut download_mode_counts = BTreeMap::new();
    let mut parse_status_counts = BTreeMap::new();

    for record in &effective_records {
        *source_kind_counts
            .entry(format!("{:?}", record.source_kind))
            .or_insert(0) += 1;
        *download_mode_counts
            .entry(format!("{:?}", record.download_mode))
            .or_insert(0) += 1;
        *parse_status_counts
            .entry(format!("{:?}", record.parse_status))
            .or_insert(0) += 1;
    }

    CorpusStats {
        total_papers: effective_records.len(),
        papers_with_parsed_content: live_parsed
            .iter()
            .filter(|paper| {
                paper.metadata.parse_status == ParseStatus::Parsed
                    || paper.abstract_text.is_some()
                    || !paper.sections.is_empty()
                    || !paper.figures.is_empty()
                    || !paper.tables.is_empty()
                    || !paper.citations.is_empty()
            })
            .count(),
        papers_with_local_tex: effective_records
            .iter()
            .filter(|record| record.has_local_tex)
            .count(),
        papers_with_local_pdf: effective_records
            .iter()
            .filter(|record| record.has_local_pdf)
            .count(),
        total_sections: live_parsed.iter().map(|paper| paper.sections.len()).sum(),
        total_figures: live_parsed.iter().map(|paper| paper.figures.len()).sum(),
        total_tables: live_parsed.iter().map(|paper| paper.tables.len()).sum(),
        total_citations: live_parsed.iter().map(|paper| paper.citations.len()).sum(),
        source_kind_counts,
        download_mode_counts,
        parse_status_counts,
    }
}

pub fn search_papers(
    registry: &[PaperSourceRecord],
    parsed_papers: &[ParsedPaper],
    relevance_tags: &[String],
    query: &str,
    limit: usize,
) -> Result<SearchResults> {
    let query = query.trim();
    if limit == 0 {
        bail!("Search limit must be at least 1");
    }
    if query.is_empty() {
        bail!("Search query must not be empty");
    }

    let effective_records = effective_registry_records(registry, parsed_papers);
    let parsed_by_record_id = matched_parsed_papers(&effective_records, parsed_papers);
    let query_lower = query.to_lowercase();
    let mut results = Vec::new();

    for record in &effective_records {
        let parsed = parsed_by_record_id.get(&record.paper_id).copied();
        let mut score = 0u32;
        let mut matched_fields = BTreeSet::new();
        let mut snippet = None;

        add_text_match(
            &mut score,
            &mut matched_fields,
            "paper_id",
            110,
            &record.paper_id,
            &query_lower,
            &mut snippet,
        );
        add_text_match(
            &mut score,
            &mut matched_fields,
            "title",
            120,
            &record.title,
            &query_lower,
            &mut snippet,
        );
        if let Some(paper) = parsed {
            add_text_match(
                &mut score,
                &mut matched_fields,
                "parsed_title",
                80,
                &paper.metadata.title,
                &query_lower,
                &mut snippet,
            );
        }
        if let Some(citation_key) = &record.citation_key {
            add_text_match(
                &mut score,
                &mut matched_fields,
                "citation_key",
                100,
                citation_key,
                &query_lower,
                &mut snippet,
            );
        }
        if let Some(arxiv_id) = &record.arxiv_id {
            add_text_match(
                &mut score,
                &mut matched_fields,
                "arxiv_id",
                100,
                arxiv_id,
                &query_lower,
                &mut snippet,
            );
        }
        if let Some(doi) = &record.doi {
            add_text_match(
                &mut score,
                &mut matched_fields,
                "doi",
                90,
                doi,
                &query_lower,
                &mut snippet,
            );
        }

        for author in &record.authors {
            add_text_match(
                &mut score,
                &mut matched_fields,
                "authors",
                70,
                author,
                &query_lower,
                &mut snippet,
            );
        }

        if let Some(paper) = parsed {
            if let Some(abstract_text) = &paper.abstract_text {
                add_text_match(
                    &mut score,
                    &mut matched_fields,
                    "abstract",
                    50,
                    abstract_text,
                    &query_lower,
                    &mut snippet,
                );
            }

            for section in &paper.sections {
                add_text_match(
                    &mut score,
                    &mut matched_fields,
                    "section_title",
                    40,
                    &section.title,
                    &query_lower,
                    &mut snippet,
                );
                add_text_match(
                    &mut score,
                    &mut matched_fields,
                    "section_content",
                    20,
                    &section.content,
                    &query_lower,
                    &mut snippet,
                );
            }

            for citation in &paper.citations {
                add_text_match(
                    &mut score,
                    &mut matched_fields,
                    "citations",
                    30,
                    citation,
                    &query_lower,
                    &mut snippet,
                );
            }

            for figure in &paper.figures {
                add_text_match(
                    &mut score,
                    &mut matched_fields,
                    "figure_captions",
                    20,
                    &figure.caption,
                    &query_lower,
                    &mut snippet,
                );
            }

            for table in &paper.tables {
                add_text_match(
                    &mut score,
                    &mut matched_fields,
                    "table_captions",
                    20,
                    &table.caption,
                    &query_lower,
                    &mut snippet,
                );
            }
        }

        if score == 0 {
            continue;
        }

        results.push(SearchHit {
            paper_id: record.paper_id.clone(),
            citation_key: record.citation_key.clone(),
            title: record.title.clone(),
            year: record.year.clone(),
            parse_status: record.parse_status.clone(),
            has_local_tex: record.has_local_tex,
            has_local_pdf: record.has_local_pdf,
            score,
            matched_fields: matched_fields.into_iter().collect(),
            snippet,
            relevance_tags: parsed
                .map(|paper| matched_relevance_tags(paper, relevance_tags))
                .unwrap_or_default(),
        });
    }

    results.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.paper_id.cmp(&right.paper_id))
    });
    let total_matches = results.len();
    let has_more = total_matches > limit;
    results.truncate(limit);
    Ok(SearchResults {
        query: query.to_string(),
        limit,
        total_matches,
        has_more,
        hits: results,
    })
}

pub fn inspect_paper(
    config: &RepoConfig,
    registry: &[PaperSourceRecord],
    parsed_papers: &[ParsedPaper],
    selector: &str,
) -> Result<PaperInspection> {
    let effective_records = effective_registry_records(registry, parsed_papers);
    let parsed_by_record_id = matched_parsed_papers(&effective_records, parsed_papers);
    let record = resolve_record(&effective_records, &parsed_by_record_id, selector)?;
    let live_parsed = unique_parsed_papers(parsed_by_record_id.values().copied().collect());
    let parsed = parsed_by_record_id.get(&record.paper_id).copied();
    let self_parsed_id = parsed
        .map(|paper| paper.metadata.paper_id.as_str())
        .unwrap_or(record.paper_id.as_str());
    let parsed_json_path = parsed.and_then(|paper| {
        let path = config
            .parsed_root()
            .join(format!("{}.json", paper.metadata.paper_id));
        path.is_file().then_some(path)
    });

    let cited_by = record
        .citation_key
        .as_ref()
        .map(|citation_key| {
            let mut incoming = live_parsed
                .iter()
                .copied()
                .filter(|paper| paper.metadata.paper_id != self_parsed_id)
                .filter(|paper| {
                    paper
                        .citations
                        .iter()
                        .any(|item| item.eq_ignore_ascii_case(citation_key))
                })
                .map(|paper| PaperReference {
                    paper_id: paper.metadata.paper_id.clone(),
                    citation_key: paper.metadata.citation_key.clone(),
                    title: paper.metadata.title.clone(),
                })
                .collect::<Vec<_>>();
            incoming.sort_by(|left, right| left.paper_id.cmp(&right.paper_id));
            incoming
        })
        .unwrap_or_default();

    Ok(PaperInspection {
        metadata: record.clone(),
        parsed_json_path,
        materialized_markdown_path: {
            let path = config
                .generated_docs_root
                .join(format!("{}.md", record.paper_id));
            path.is_file().then_some(path)
        },
        local_tex_dir: record.tex_dir.as_ref().and_then(|dir| {
            let path = config.tex_root.join(dir);
            path.is_dir().then_some(path)
        }),
        local_pdf_path: record.pdf_file.as_ref().and_then(|file| {
            let path = config.pdf_root.join(file);
            path.is_file().then_some(path)
        }),
        abstract_text: parsed.and_then(|paper| paper.abstract_text.clone()),
        sections: parsed
            .map(|paper| {
                paper
                    .sections
                    .iter()
                    .map(|section| SectionHeading {
                        level: section.level,
                        title: section.title.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default(),
        figure_captions: parsed
            .map(|paper| {
                paper
                    .figures
                    .iter()
                    .map(|figure| figure.caption.clone())
                    .collect()
            })
            .unwrap_or_default(),
        table_captions: parsed
            .map(|paper| {
                paper
                    .tables
                    .iter()
                    .map(|table| table.caption.clone())
                    .collect()
            })
            .unwrap_or_default(),
        citations: parsed
            .map(|paper| paper.citations.clone())
            .unwrap_or_default(),
        cited_by,
        provenance: parsed
            .map(|paper| paper.provenance.clone())
            .unwrap_or_default(),
        relevance_tags: parsed
            .map(|paper| matched_relevance_tags(paper, &config.relevance_tags))
            .unwrap_or_default(),
    })
}

fn resolve_record<'a>(
    registry: &'a [PaperSourceRecord],
    parsed_by_record_id: &BTreeMap<String, &'a ParsedPaper>,
    selector: &str,
) -> Result<&'a PaperSourceRecord> {
    let selector = selector.trim();
    if selector.is_empty() {
        bail!("Paper selector must not be empty");
    }

    let exact_matches = registry
        .iter()
        .filter(|record| {
            record.paper_id.eq_ignore_ascii_case(selector)
                || record
                    .citation_key
                    .as_deref()
                    .is_some_and(|key| key.eq_ignore_ascii_case(selector))
                || record
                    .arxiv_id
                    .as_deref()
                    .is_some_and(|arxiv_id| arxiv_id.eq_ignore_ascii_case(selector))
                || record.title.eq_ignore_ascii_case(selector)
                || parsed_by_record_id
                    .get(&record.paper_id)
                    .is_some_and(|paper| paper.metadata.title.eq_ignore_ascii_case(selector))
        })
        .collect::<Vec<_>>();

    match exact_matches.as_slice() {
        [record] => Ok(*record),
        [] => bail!("No paper matched selector `{selector}`. Use `search` to discover available paper ids or citation keys."),
        records => {
            let suggestions = records
                .iter()
                .take(5)
                .map(|record| format!("{} ({})", record.paper_id, record.title))
                .collect::<Vec<_>>()
                .join(", ");
            bail!("Selector `{selector}` matched multiple papers: {suggestions}");
        }
    }
}

fn add_text_match(
    score: &mut u32,
    matched_fields: &mut BTreeSet<String>,
    field: &str,
    field_score: u32,
    haystack: &str,
    query_lower: &str,
    snippet: &mut Option<String>,
) {
    if !contains_case_insensitive(haystack, query_lower) {
        return;
    }

    *score += field_score;
    matched_fields.insert(field.to_string());
    if snippet.is_none() {
        *snippet = Some(build_snippet(haystack, query_lower));
    }
}

fn contains_case_insensitive(haystack: &str, query_lower: &str) -> bool {
    haystack.to_lowercase().contains(query_lower)
}

fn build_snippet(text: &str, query_lower: &str) -> String {
    let candidate = text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && contains_case_insensitive(line, query_lower))
        .unwrap_or(text);
    truncate(&normalize_whitespace(candidate), 160)
}

fn truncate(text: &str, max_chars: usize) -> String {
    let mut out = String::new();
    for (index, ch) in text.chars().enumerate() {
        if index >= max_chars {
            out.push_str("...");
            break;
        }
        out.push(ch);
    }
    out
}

fn normalize_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn has_structured_parsed_content(paper: &ParsedPaper) -> bool {
    paper.metadata.parse_status == ParseStatus::Parsed
        || paper.abstract_text.is_some()
        || !paper.sections.is_empty()
        || !paper.figures.is_empty()
        || !paper.tables.is_empty()
        || !paper.citations.is_empty()
        || !paper.provenance.is_empty()
}

fn effective_registry_records(
    registry: &[PaperSourceRecord],
    parsed_papers: &[ParsedPaper],
) -> Vec<PaperSourceRecord> {
    let matched = matched_parsed_papers(registry, parsed_papers);

    registry
        .iter()
        .map(|record| match matched.get(&record.paper_id) {
            Some(paper) => {
                let mut merged = record.clone();
                if has_structured_parsed_content(paper) {
                    merged.parse_status = ParseStatus::Parsed;
                }
                merged
            }
            None => record.clone(),
        })
        .collect()
}

fn matched_parsed_papers<'a>(
    registry: &[PaperSourceRecord],
    parsed_papers: &'a [ParsedPaper],
) -> BTreeMap<String, &'a ParsedPaper> {
    registry
        .iter()
        .filter_map(|record| {
            let paper = select_best_parsed_match(record, parsed_papers)?;
            Some((record.paper_id.clone(), paper))
        })
        .collect()
}

fn select_best_parsed_match<'a>(
    record: &PaperSourceRecord,
    parsed_papers: &'a [ParsedPaper],
) -> Option<&'a ParsedPaper> {
    let mut best: Option<(&ParsedPaper, ParsedMatchScore)> = None;
    let mut ambiguous = false;

    for paper in parsed_papers {
        let Some(score) = parsed_match_score(record, paper) else {
            continue;
        };

        match best {
            None => {
                best = Some((paper, score));
                ambiguous = false;
            }
            Some((best_paper, best_score)) => {
                if score > best_score {
                    best = Some((paper, score));
                    ambiguous = false;
                } else if score == best_score
                    && paper.metadata.paper_id != best_paper.metadata.paper_id
                {
                    ambiguous = true;
                }
            }
        }
    }

    if ambiguous {
        None
    } else {
        best.map(|(paper, _)| paper)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct ParsedMatchScore {
    structured: u8,
    richness: usize,
    exact_paper_id: u8,
    exact_citation: u8,
    exact_arxiv: u8,
}

fn parsed_match_score(record: &PaperSourceRecord, paper: &ParsedPaper) -> Option<ParsedMatchScore> {
    let exact_paper_id = u8::from(paper.metadata.paper_id == record.paper_id);
    let exact_citation = u8::from(
        record.citation_key.is_some()
            && paper.metadata.citation_key.is_some()
            && record.citation_key == paper.metadata.citation_key,
    );
    let exact_arxiv = u8::from(
        record.arxiv_id.is_some()
            && paper.metadata.arxiv_id.is_some()
            && record.arxiv_id == paper.metadata.arxiv_id,
    );

    if exact_paper_id == 0 && exact_citation == 0 && exact_arxiv == 0 {
        return None;
    }

    Some(ParsedMatchScore {
        structured: u8::from(has_structured_parsed_content(paper)),
        richness: parsed_content_richness(paper),
        exact_paper_id,
        exact_citation,
        exact_arxiv,
    })
}

fn parsed_content_richness(paper: &ParsedPaper) -> usize {
    usize::from(paper.abstract_text.is_some())
        + paper.sections.len()
        + paper.figures.len()
        + paper.tables.len()
        + paper.citations.len()
        + paper.provenance.len()
}

fn unique_parsed_papers(papers: Vec<&ParsedPaper>) -> Vec<&ParsedPaper> {
    let mut seen = BTreeSet::new();
    papers
        .into_iter()
        .filter(|paper| seen.insert(paper.metadata.paper_id.clone()))
        .collect()
}

fn live_parsed_papers<'a>(
    registry: &'a [PaperSourceRecord],
    parsed_papers: &'a [ParsedPaper],
) -> Vec<&'a ParsedPaper> {
    unique_parsed_papers(
        matched_parsed_papers(registry, parsed_papers)
            .into_values()
            .collect(),
    )
}

fn load_registry_snapshot(
    config: &RepoConfig,
    registry_generated: bool,
    manifest_present: bool,
    bib_present: bool,
) -> Result<Vec<PaperSourceRecord>> {
    if registry_generated {
        return load_registry(config.registry_path());
    }
    if manifest_present && bib_present {
        return build_registry_snapshot(config);
    }
    Ok(Vec::new())
}

fn expected_pdf_count(registry: &[PaperSourceRecord]) -> usize {
    registry
        .iter()
        .filter(|record| record.pdf_file.is_some())
        .count()
}

fn inspect_runtime(repo_root: Option<&Path>, check_runtime: bool) -> RuntimeCapability {
    if !check_runtime {
        let not_checked = RuntimeCheck {
            state: CapabilityState::NotChecked,
            detail: "runtime checks disabled; pass --check-runtime".into(),
        };
        return RuntimeCapability {
            checked: false,
            docker: not_checked.clone(),
            neo4j_service: not_checked.clone(),
            uv: not_checked.clone(),
            codegraphcontext: not_checked.clone(),
            graphiti_helpers: not_checked.clone(),
            ollama_service: not_checked,
        };
    }

    let docker = command_check("docker", &["compose", "version"]);
    let uv = command_check("uv", &["--version"]);
    let neo4j_service = tcp_check("127.0.0.1:7687", "Neo4j Bolt port 7687");
    let ollama_service = tcp_check("127.0.0.1:11434", "Ollama HTTP port 11434");
    let codegraphcontext = repo_root
        .map(|root| {
            let python = root.join(".cache/kg/venvs/cgc/bin/python");
            if python.is_file() {
                RuntimeCheck {
                    state: CapabilityState::Ready,
                    detail: format!(
                        "CodeGraphContext virtualenv present at {}",
                        python.display()
                    ),
                }
            } else {
                RuntimeCheck {
                    state: CapabilityState::Missing,
                    detail: format!("No CodeGraphContext virtualenv at {}", python.display()),
                }
            }
        })
        .unwrap_or_else(|| RuntimeCheck {
            state: CapabilityState::NotChecked,
            detail: "repo root not supplied".into(),
        });
    let graphiti_helpers = repo_root
        .map(|root| {
            let start = root.join("scripts/kg/start_graphiti.sh");
            let ingest = root.join("scripts/kg/ingest_docs.sh");
            if start.is_file() && ingest.is_file() {
                RuntimeCheck {
                    state: CapabilityState::Ready,
                    detail: "Graphiti helper scripts are present".into(),
                }
            } else {
                RuntimeCheck {
                    state: CapabilityState::Missing,
                    detail: "Graphiti helper scripts are missing".into(),
                }
            }
        })
        .unwrap_or_else(|| RuntimeCheck {
            state: CapabilityState::NotChecked,
            detail: "repo root not supplied".into(),
        });

    RuntimeCapability {
        checked: true,
        docker,
        neo4j_service,
        uv,
        codegraphcontext,
        graphiti_helpers,
        ollama_service,
    }
}

fn command_check(binary: &str, args: &[&str]) -> RuntimeCheck {
    match Command::new(binary).args(args).output() {
        Ok(output) if output.status.success() => RuntimeCheck {
            state: CapabilityState::Ready,
            detail: format!("`{binary}` is available"),
        },
        Ok(output) => RuntimeCheck {
            state: CapabilityState::Unavailable,
            detail: format!("`{binary}` exited with status {}", output.status),
        },
        Err(error) => RuntimeCheck {
            state: CapabilityState::Missing,
            detail: format!("`{binary}` unavailable: {error}"),
        },
    }
}

fn tcp_check(address: &str, label: &str) -> RuntimeCheck {
    let Ok(socket) = address.parse::<SocketAddr>() else {
        return RuntimeCheck {
            state: CapabilityState::Unavailable,
            detail: format!("invalid runtime address {address}"),
        };
    };
    match TcpStream::connect_timeout(&socket, Duration::from_millis(150)) {
        Ok(_) => RuntimeCheck {
            state: CapabilityState::Ready,
            detail: format!("{label} is reachable"),
        },
        Err(error) => RuntimeCheck {
            state: CapabilityState::Unavailable,
            detail: format!("{label} is not reachable: {error}"),
        },
    }
}

fn inspect_benchmark_capability(
    catalog: Option<&Path>,
    integrations: Option<&Path>,
) -> Option<BenchmarkCapability> {
    let catalog = catalog?;
    let catalog_present = catalog.is_file();
    let integrations_present = integrations.is_some_and(|path| path.is_file());
    let mut support_entries = 0usize;
    let mut ready_entries = 0usize;
    let mut missing_binaries = BTreeSet::new();
    let mut missing_env_vars = BTreeSet::new();

    if catalog_present && integrations_present {
        if let Some(integrations) = integrations {
            if let (Ok(catalog), Ok(integrations)) = (
                crate::benchmark::load_benchmark_catalog(catalog),
                crate::benchmark_runner::load_benchmark_integrations(integrations),
            ) {
                let empty_ids: Vec<String> = Vec::new();
                if let Ok(statuses) = crate::benchmark_runner::inspect_benchmark_support(
                    &catalog,
                    &integrations,
                    None,
                    &empty_ids,
                ) {
                    support_entries = statuses.len();
                    ready_entries = statuses
                        .iter()
                        .filter(|status| {
                            status.missing_binaries.is_empty() && status.missing_env_vars.is_empty()
                        })
                        .count();
                    for status in statuses {
                        missing_binaries.extend(status.missing_binaries);
                        missing_env_vars.extend(status.missing_env_vars);
                    }
                }
            }
        }
    }

    Some(BenchmarkCapability {
        state: if !catalog_present {
            CapabilityState::Missing
        } else if !integrations_present {
            CapabilityState::Configured
        } else if ready_entries > 0 {
            CapabilityState::Ready
        } else {
            CapabilityState::Generated
        },
        catalog_present,
        integrations_present,
        support_entries,
        ready_entries,
        missing_binaries: missing_binaries.into_iter().collect(),
        missing_env_vars: missing_env_vars.into_iter().collect(),
    })
}

fn next_actions(
    config_path: &Path,
    registry: &LiteratureRegistryCapability,
    downloads: &DownloadCapability,
    parsing: &ParsingCapability,
    graph_outputs: &GraphOutputCapability,
    semantic_scholar: &SemanticScholarCapability,
) -> Vec<String> {
    let config = config_path.display();
    let mut actions = Vec::new();
    if !registry.registry_generated && registry.manifest_present && registry.bib_present {
        actions.push(format!(
            "cargo run -p litkg-cli -- ingest --config {config}"
        ));
    }
    if registry.records_total > 0
        && downloads.records_with_local_tex < downloads.arxiv_records
        && downloads.arxiv_records > 0
    {
        actions.push(format!(
            "cargo run -p litkg-cli -- lit download --config {config}"
        ));
    }
    if parsing.parsed_papers < registry.records_total && downloads.records_with_local_tex > 0 {
        actions.push(format!(
            "cargo run -p litkg-cli -- lit parse --config {config}"
        ));
    }
    if graph_outputs.graphify_configured
        && !(graph_outputs.graphify_index_generated && graph_outputs.graphify_manifest_generated)
        && parsing.parsed_papers > 0
    {
        actions.push(format!(
            "cargo run -p litkg-cli -- kg build --config {config}"
        ));
    }
    if semantic_scholar.configured
        && semantic_scholar.api_key_present
        && semantic_scholar.enriched_records < registry.records_total
    {
        actions.push(format!(
            "cargo run -p litkg-cli -- s2 enrich --config {config}"
        ));
    }
    if graph_outputs.neo4j_configured
        && !(graph_outputs.neo4j_nodes_generated && graph_outputs.neo4j_edges_generated)
        && parsing.parsed_papers > 0
    {
        actions.push(format!(
            "cargo run -p litkg-cli -- kg export --config {config}"
        ));
    }
    if graph_outputs.neo4j_nodes_generated && graph_outputs.neo4j_edges_generated {
        actions.push(format!(
            "cargo run -p litkg-cli -- kg visualize --config {config}"
        ));
    }
    actions
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SinkMode;
    use crate::model::{DownloadMode, PaperFigure, PaperSection, PaperTable, SourceKind};
    use std::path::Path;

    fn config(root: &Path) -> RepoConfig {
        RepoConfig {
            project: None,
            sources: std::collections::BTreeMap::new(),
            representation: None,
            backends: None,
            storage: None,
            manifest_path: root.join("sources.jsonl"),
            bib_path: root.join("references.bib"),
            tex_root: root.join("tex"),
            pdf_root: root.join("pdf"),
            generated_docs_root: root.join("generated"),
            registry_path: None,
            parsed_root: None,
            neo4j_export_root: None,
            memory_state_root: None,
            sink: SinkMode::Graphify,
            graphify_rebuild_command: None,
            download_pdfs: true,
            relevance_tags: vec!["loop closure".into(), "ADVIO".into()],
            semantic_scholar: None,
        }
    }

    fn sample_registry() -> Vec<PaperSourceRecord> {
        vec![
            PaperSourceRecord {
                paper_id: "alpha".into(),
                citation_key: Some("smith2025alpha".into()),
                title: "Alpha SLAM".into(),
                authors: vec!["Alice Smith".into()],
                year: Some("2025".into()),
                arxiv_id: Some("2501.00001".into()),
                doi: None,
                url: None,
                tex_dir: Some("alpha".into()),
                pdf_file: Some("alpha.pdf".into()),
                source_kind: crate::model::SourceKind::ManifestAndBib,
                download_mode: DownloadMode::ManifestSourcePlusPdf,
                has_local_tex: true,
                has_local_pdf: true,
                parse_status: ParseStatus::Parsed,
                semantic_scholar: None,
            },
            PaperSourceRecord {
                paper_id: "beta".into(),
                citation_key: Some("jones2024beta".into()),
                title: "Beta Navigation".into(),
                authors: vec!["Bob Jones".into()],
                year: Some("2024".into()),
                arxiv_id: Some("2401.00002".into()),
                doi: None,
                url: None,
                tex_dir: Some("beta".into()),
                pdf_file: None,
                source_kind: SourceKind::ManifestAndBib,
                download_mode: DownloadMode::ManifestSource,
                has_local_tex: true,
                has_local_pdf: false,
                parse_status: ParseStatus::Parsed,
                semantic_scholar: None,
            },
            PaperSourceRecord {
                paper_id: "gamma".into(),
                citation_key: None,
                title: "Gamma Survey".into(),
                authors: vec![],
                year: None,
                arxiv_id: Some("2301.00003".into()),
                doi: None,
                url: None,
                tex_dir: None,
                pdf_file: None,
                source_kind: SourceKind::Manifest,
                download_mode: DownloadMode::MetadataOnly,
                has_local_tex: false,
                has_local_pdf: false,
                parse_status: ParseStatus::MetadataOnly,
                semantic_scholar: None,
            },
        ]
    }

    fn sample_parsed() -> Vec<ParsedPaper> {
        vec![
            ParsedPaper {
                kind: crate::model::DocumentKind::Literature,
                metadata: sample_registry()[0].clone(),
                abstract_text: Some("Alpha SLAM introduces loop closure robustness.".into()),
                sections: vec![PaperSection {
                    level: 1,
                    title: "Method".into(),
                    content: "Loop closure and ADVIO evaluation.".into(),
                }],
                figures: vec![PaperFigure {
                    caption: "Overview figure".into(),
                }],
                tables: vec![PaperTable {
                    caption: "Evaluation table".into(),
                }],
                citations: vec!["jones2024beta".into()],
                citation_references: Vec::new(),
                provenance: vec!["manifest".into(), "bib".into()],
            },
            ParsedPaper {
                kind: crate::model::DocumentKind::Literature,
                metadata: sample_registry()[1].clone(),
                abstract_text: Some("Beta focuses on navigation baselines.".into()),
                sections: vec![PaperSection {
                    level: 1,
                    title: "Experiments".into(),
                    content: "Compares against smith2025alpha.".into(),
                }],
                figures: vec![],
                tables: vec![],
                citations: vec!["smith2025alpha".into()],
                citation_references: Vec::new(),
                provenance: vec!["manifest".into()],
            },
        ]
    }

    #[test]
    fn computes_corpus_stats_from_registry_and_parsed_papers() {
        let mut parsed = sample_parsed();
        parsed.push(ParsedPaper {
            kind: crate::model::DocumentKind::Literature,
            metadata: PaperSourceRecord {
                paper_id: "stale-paper".into(),
                citation_key: Some("stale2024paper".into()),
                title: "Stale Paper".into(),
                authors: vec![],
                year: Some("2024".into()),
                arxiv_id: None,
                doi: None,
                url: None,
                tex_dir: None,
                pdf_file: None,
                source_kind: SourceKind::Bib,
                download_mode: DownloadMode::MetadataOnly,
                has_local_tex: false,
                has_local_pdf: false,
                parse_status: ParseStatus::Parsed,
                semantic_scholar: None,
            },
            abstract_text: Some("stale".into()),
            sections: vec![PaperSection {
                level: 1,
                title: "Stale".into(),
                content: "Should not count.".into(),
            }],
            figures: vec![PaperFigure {
                caption: "stale figure".into(),
            }],
            tables: vec![PaperTable {
                caption: "stale table".into(),
            }],
            citations: vec!["stale-citation".into()],
            citation_references: Vec::new(),
            provenance: vec![],
        });

        let stats = compute_corpus_stats(&sample_registry(), &parsed);
        assert_eq!(stats.total_papers, 3);
        assert_eq!(stats.papers_with_parsed_content, 2);
        assert_eq!(stats.papers_with_local_tex, 2);
        assert_eq!(stats.papers_with_local_pdf, 1);
        assert_eq!(stats.total_sections, 2);
        assert_eq!(stats.total_figures, 1);
        assert_eq!(stats.total_tables, 1);
        assert_eq!(stats.total_citations, 2);
        assert_eq!(stats.parse_status_counts["Parsed"], 2);
        assert_eq!(stats.parse_status_counts["MetadataOnly"], 1);
    }

    #[test]
    fn searches_metadata_and_parsed_content_with_ranking() {
        let hits = search_papers(
            &sample_registry(),
            &sample_parsed(),
            &["ADVIO".into()],
            "alpha",
            10,
        )
        .unwrap();
        assert_eq!(hits.total_matches, 2);
        assert!(!hits.has_more);
        assert_eq!(hits.hits.len(), 2);
        assert_eq!(hits.hits[0].paper_id, "alpha");
        assert!(hits.hits[0]
            .matched_fields
            .iter()
            .any(|field| field == "paper_id"));
        assert!(hits.hits[0]
            .matched_fields
            .iter()
            .any(|field| field == "title"));
        assert!(hits.hits[0].relevance_tags.iter().any(|tag| tag == "ADVIO"));

        let content_hits = search_papers(
            &sample_registry(),
            &sample_parsed(),
            &[],
            "loop closure",
            10,
        )
        .unwrap();
        assert_eq!(content_hits.hits[0].paper_id, "alpha");
        assert!(content_hits.hits[0]
            .snippet
            .as_deref()
            .is_some_and(|snippet| snippet.contains("loop closure")));

        let zero_limit_error = search_papers(&sample_registry(), &sample_parsed(), &[], "alpha", 0)
            .unwrap_err()
            .to_string();
        assert!(zero_limit_error.contains("at least 1"));

        let empty_query_error = search_papers(&sample_registry(), &sample_parsed(), &[], "   ", 10)
            .unwrap_err()
            .to_string();
        assert!(empty_query_error.contains("must not be empty"));

        let caption_hits = search_papers(
            &sample_registry(),
            &sample_parsed(),
            &[],
            "evaluation table",
            10,
        )
        .unwrap();
        assert_eq!(caption_hits.hits[0].paper_id, "alpha");
        assert!(caption_hits.hits[0]
            .matched_fields
            .iter()
            .any(|field| field == "table_captions"));
    }

    #[test]
    fn search_matches_parsed_titles_but_preserves_current_metadata_title() {
        let registry = vec![PaperSourceRecord {
            paper_id: "demo-paper".into(),
            citation_key: Some("demo2025paper".into()),
            title: "Bib Title".into(),
            authors: vec![],
            year: Some("2025".into()),
            arxiv_id: None,
            doi: None,
            url: None,
            tex_dir: None,
            pdf_file: None,
            source_kind: SourceKind::Bib,
            download_mode: DownloadMode::MetadataOnly,
            has_local_tex: false,
            has_local_pdf: false,
            parse_status: ParseStatus::MetadataOnly,
            semantic_scholar: None,
        }];
        let parsed = vec![ParsedPaper {
            kind: crate::model::DocumentKind::Literature,
            metadata: PaperSourceRecord {
                title: "Parsed Title".into(),
                parse_status: ParseStatus::Parsed,
                semantic_scholar: None,
                has_local_tex: true,
                ..registry[0].clone()
            },
            abstract_text: Some("parsed abstract".into()),
            sections: vec![PaperSection {
                level: 1,
                title: "Parsed section".into(),
                content: "content".into(),
            }],
            figures: vec![],
            tables: vec![],
            citations: vec![],
            citation_references: Vec::new(),
            provenance: vec![],
        }];

        let results = search_papers(&registry, &parsed, &[], "Parsed Title", 10).unwrap();
        assert_eq!(results.hits[0].title, "Bib Title");
        assert_eq!(results.hits[0].parse_status, ParseStatus::Parsed);
        assert!(results.hits[0]
            .matched_fields
            .iter()
            .any(|field| field == "parsed_title"));
    }

    #[test]
    fn search_preserves_current_registry_asset_state() {
        let registry = vec![PaperSourceRecord {
            paper_id: "demo-paper".into(),
            citation_key: Some("demo2025paper".into()),
            title: "Bib Title".into(),
            authors: vec![],
            year: Some("2025".into()),
            arxiv_id: None,
            doi: None,
            url: None,
            tex_dir: None,
            pdf_file: Some("demo.pdf".into()),
            source_kind: SourceKind::Bib,
            download_mode: DownloadMode::ManifestSourcePlusPdf,
            has_local_tex: true,
            has_local_pdf: true,
            parse_status: ParseStatus::Downloaded,
            semantic_scholar: None,
        }];
        let parsed = vec![ParsedPaper {
            kind: crate::model::DocumentKind::Literature,
            metadata: PaperSourceRecord {
                title: "Parsed Title".into(),
                has_local_tex: false,
                has_local_pdf: false,
                parse_status: ParseStatus::Parsed,
                semantic_scholar: None,
                ..registry[0].clone()
            },
            abstract_text: Some("parsed abstract".into()),
            sections: vec![PaperSection {
                level: 1,
                title: "Parsed section".into(),
                content: "content".into(),
            }],
            figures: vec![],
            tables: vec![],
            citations: vec![],
            citation_references: Vec::new(),
            provenance: vec![],
        }];

        let results = search_papers(&registry, &parsed, &[], "Parsed Title", 10).unwrap();
        assert_eq!(results.hits[0].parse_status, ParseStatus::Parsed);
        assert!(results.hits[0].has_local_tex);
        assert!(results.hits[0].has_local_pdf);
    }

    #[test]
    fn stable_identifier_match_beats_stale_exact_id_sidecar() {
        let registry = vec![PaperSourceRecord {
            paper_id: "arxiv-2601-00001".into(),
            citation_key: None,
            title: "Current Manifest Paper".into(),
            authors: vec![],
            year: None,
            arxiv_id: Some("2601.00001".into()),
            doi: None,
            url: None,
            tex_dir: Some("paper-tex".into()),
            pdf_file: None,
            source_kind: SourceKind::Manifest,
            download_mode: DownloadMode::ManifestSource,
            has_local_tex: true,
            has_local_pdf: false,
            parse_status: ParseStatus::Downloaded,
            semantic_scholar: None,
        }];
        let parsed = vec![
            ParsedPaper {
                kind: crate::model::DocumentKind::Literature,
                metadata: PaperSourceRecord {
                    paper_id: "arxiv-2601-00001".into(),
                    citation_key: None,
                    title: "Stale Metadata".into(),
                    authors: vec![],
                    year: None,
                    arxiv_id: Some("2601.00001".into()),
                    doi: None,
                    url: None,
                    tex_dir: Some("paper-tex".into()),
                    pdf_file: None,
                    source_kind: SourceKind::Manifest,
                    download_mode: DownloadMode::ManifestSource,
                    has_local_tex: true,
                    has_local_pdf: false,
                    parse_status: ParseStatus::MetadataOnly,
                    semantic_scholar: None,
                },
                abstract_text: None,
                sections: vec![],
                figures: vec![],
                tables: vec![],
                citations: vec![],
                citation_references: Vec::new(),
                provenance: vec![],
            },
            ParsedPaper {
                kind: crate::model::DocumentKind::Literature,
                metadata: PaperSourceRecord {
                    paper_id: "old2024paper".into(),
                    citation_key: Some("old2024paper".into()),
                    title: "Parsed Paper".into(),
                    authors: vec!["Old Author".into()],
                    year: Some("2024".into()),
                    arxiv_id: Some("2601.00001".into()),
                    doi: None,
                    url: None,
                    tex_dir: Some("paper-tex".into()),
                    pdf_file: Some("stale.pdf".into()),
                    source_kind: SourceKind::ManifestAndBib,
                    download_mode: DownloadMode::ManifestSourcePlusPdf,
                    has_local_tex: true,
                    has_local_pdf: true,
                    parse_status: ParseStatus::Parsed,
                    semantic_scholar: None,
                },
                abstract_text: Some("stale parsed abstract".into()),
                sections: vec![PaperSection {
                    level: 1,
                    title: "Parsed Section".into(),
                    content: "content".into(),
                }],
                figures: vec![],
                tables: vec![],
                citations: vec![],
                citation_references: Vec::new(),
                provenance: vec!["paper-tex/main.tex".into()],
            },
        ];

        let results = search_papers(&registry, &parsed, &[], "Parsed Paper", 10).unwrap();
        assert_eq!(results.hits[0].title, "Current Manifest Paper");
        assert_eq!(results.hits[0].parse_status, ParseStatus::Parsed);
        assert!(results.hits[0]
            .matched_fields
            .iter()
            .any(|field| field == "parsed_title"));
    }

    #[test]
    fn ambiguous_stable_identifier_matches_are_ignored() {
        let registry = vec![PaperSourceRecord {
            paper_id: "arxiv-2601-00001".into(),
            citation_key: None,
            title: "Current Manifest Paper".into(),
            authors: vec![],
            year: None,
            arxiv_id: Some("2601.00001".into()),
            doi: None,
            url: None,
            tex_dir: Some("paper-tex".into()),
            pdf_file: None,
            source_kind: SourceKind::Manifest,
            download_mode: DownloadMode::ManifestSource,
            has_local_tex: true,
            has_local_pdf: false,
            parse_status: ParseStatus::Downloaded,
            semantic_scholar: None,
        }];
        let parsed = vec![
            ParsedPaper {
                kind: crate::model::DocumentKind::Literature,
                metadata: PaperSourceRecord {
                    paper_id: "old-a".into(),
                    citation_key: Some("old-a".into()),
                    title: "Parsed A".into(),
                    authors: vec![],
                    year: None,
                    arxiv_id: Some("2601.00001".into()),
                    doi: None,
                    url: None,
                    tex_dir: Some("paper-tex".into()),
                    pdf_file: None,
                    source_kind: SourceKind::Manifest,
                    download_mode: DownloadMode::ManifestSource,
                    has_local_tex: true,
                    has_local_pdf: false,
                    parse_status: ParseStatus::Parsed,
                    semantic_scholar: None,
                },
                abstract_text: Some("A".into()),
                sections: vec![PaperSection {
                    level: 1,
                    title: "Section".into(),
                    content: "A".into(),
                }],
                figures: vec![],
                tables: vec![],
                citations: vec![],
                citation_references: Vec::new(),
                provenance: vec!["a.tex".into()],
            },
            ParsedPaper {
                kind: crate::model::DocumentKind::Literature,
                metadata: PaperSourceRecord {
                    paper_id: "old-b".into(),
                    citation_key: Some("old-b".into()),
                    title: "Parsed B".into(),
                    authors: vec![],
                    year: None,
                    arxiv_id: Some("2601.00001".into()),
                    doi: None,
                    url: None,
                    tex_dir: Some("paper-tex".into()),
                    pdf_file: None,
                    source_kind: SourceKind::Manifest,
                    download_mode: DownloadMode::ManifestSource,
                    has_local_tex: true,
                    has_local_pdf: false,
                    parse_status: ParseStatus::Parsed,
                    semantic_scholar: None,
                },
                abstract_text: Some("B".into()),
                sections: vec![PaperSection {
                    level: 1,
                    title: "Section".into(),
                    content: "B".into(),
                }],
                figures: vec![],
                tables: vec![],
                citations: vec![],
                citation_references: Vec::new(),
                provenance: vec!["b.tex".into()],
            },
        ];

        assert!(matched_parsed_papers(&registry, &parsed).is_empty());
    }

    #[test]
    fn inspect_paper_rejects_ambiguous_exact_matches() {
        let dir = tempfile::tempdir().unwrap();
        let repo_config = config(dir.path());
        let mut registry = sample_registry();
        registry.push(PaperSourceRecord {
            paper_id: "alpha-duplicate".into(),
            citation_key: Some("other2025alpha".into()),
            title: "Alpha SLAM".into(),
            authors: vec!["Another Author".into()],
            year: Some("2025".into()),
            arxiv_id: Some("2501.00099".into()),
            doi: None,
            url: None,
            tex_dir: None,
            pdf_file: None,
            source_kind: SourceKind::Bib,
            download_mode: DownloadMode::MetadataOnly,
            has_local_tex: false,
            has_local_pdf: false,
            parse_status: ParseStatus::MetadataOnly,
            semantic_scholar: None,
        });

        let error = inspect_paper(&repo_config, &registry, &sample_parsed(), "Alpha SLAM")
            .unwrap_err()
            .to_string();
        assert!(error.contains("matched multiple papers"));
    }

    #[test]
    fn inspects_paper_and_builds_citation_neighborhood() {
        let dir = tempfile::tempdir().unwrap();
        let repo_config = config(dir.path());
        std::fs::create_dir_all(dir.path().join("tex").join("alpha")).unwrap();
        std::fs::create_dir_all(dir.path().join("pdf")).unwrap();
        std::fs::create_dir_all(dir.path().join("generated").join("parsed")).unwrap();
        std::fs::write(
            dir.path()
                .join("generated")
                .join("parsed")
                .join("alpha.json"),
            "{}",
        )
        .unwrap();
        std::fs::write(dir.path().join("pdf").join("alpha.pdf"), b"pdf").unwrap();
        let mut parsed = sample_parsed();
        parsed.push(ParsedPaper {
            kind: crate::model::DocumentKind::Literature,
            metadata: PaperSourceRecord {
                paper_id: "stale-citer".into(),
                citation_key: Some("stale2025citer".into()),
                title: "Stale Citer".into(),
                authors: vec![],
                year: Some("2025".into()),
                arxiv_id: None,
                doi: None,
                url: None,
                tex_dir: None,
                pdf_file: None,
                source_kind: SourceKind::Bib,
                download_mode: DownloadMode::MetadataOnly,
                has_local_tex: false,
                has_local_pdf: false,
                parse_status: ParseStatus::Parsed,
                semantic_scholar: None,
            },
            abstract_text: None,
            sections: vec![],
            figures: vec![],
            tables: vec![],
            citations: vec!["smith2025alpha".into()],
            citation_references: Vec::new(),
            provenance: vec![],
        });
        let inspection =
            inspect_paper(&repo_config, &sample_registry(), &parsed, "smith2025alpha").unwrap();

        assert_eq!(inspection.metadata.paper_id, "alpha");
        assert_eq!(inspection.sections.len(), 1);
        assert_eq!(inspection.citations, vec!["jones2024beta".to_string()]);
        assert_eq!(inspection.cited_by.len(), 1);
        assert_eq!(inspection.cited_by[0].paper_id, "beta");
        assert_eq!(
            inspection.local_tex_dir,
            Some(dir.path().join("tex").join("alpha"))
        );
        assert_eq!(
            inspection.local_pdf_path,
            Some(dir.path().join("pdf").join("alpha.pdf"))
        );
        assert!(inspection
            .relevance_tags
            .iter()
            .any(|tag| tag == "loop closure"));
    }

    #[test]
    fn inspect_paper_resolves_by_parsed_title_but_preserves_metadata_title() {
        let dir = tempfile::tempdir().unwrap();
        let repo_config = config(dir.path());
        let registry = vec![PaperSourceRecord {
            paper_id: "demo-paper".into(),
            citation_key: Some("demo2025paper".into()),
            title: "Bib Title".into(),
            authors: vec![],
            year: Some("2025".into()),
            arxiv_id: None,
            doi: None,
            url: None,
            tex_dir: None,
            pdf_file: None,
            source_kind: SourceKind::Bib,
            download_mode: DownloadMode::MetadataOnly,
            has_local_tex: false,
            has_local_pdf: false,
            parse_status: ParseStatus::MetadataOnly,
            semantic_scholar: None,
        }];
        let parsed = vec![ParsedPaper {
            kind: crate::model::DocumentKind::Literature,
            metadata: PaperSourceRecord {
                title: "Parsed Title".into(),
                parse_status: ParseStatus::Parsed,
                semantic_scholar: None,
                has_local_tex: true,
                ..registry[0].clone()
            },
            abstract_text: Some("parsed abstract".into()),
            sections: vec![PaperSection {
                level: 1,
                title: "Parsed section".into(),
                content: "content".into(),
            }],
            figures: vec![],
            tables: vec![],
            citations: vec![],
            citation_references: Vec::new(),
            provenance: vec![],
        }];

        let inspection = inspect_paper(&repo_config, &registry, &parsed, "Parsed Title").unwrap();
        assert_eq!(inspection.metadata.title, "Bib Title");
        assert_eq!(inspection.metadata.parse_status, ParseStatus::Parsed);
    }
}
