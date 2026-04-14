use anyhow::{Context, Result};
use litkg_core::{
    infer_enriched_edges, load_project_memory, MemoryChunkKind, MemoryNode, MemoryNodeKind,
    MemorySurface, MemorySurfaceKind, ParsedPaper, RepoConfig,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Neo4jNode {
    pub id: String,
    pub labels: Vec<String>,
    pub properties: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Neo4jEdge {
    pub source: String,
    pub target: String,
    pub rel_type: String,
    pub properties: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Neo4jExportBundle {
    pub root: PathBuf,
    pub nodes: Vec<Neo4jNode>,
    pub edges: Vec<Neo4jEdge>,
}

pub struct Neo4jSink;

impl Neo4jSink {
    pub fn export(config: &RepoConfig, papers: &[ParsedPaper]) -> Result<Vec<PathBuf>> {
        let out_dir = config.neo4j_export_root();
        fs::create_dir_all(&out_dir)
            .with_context(|| format!("Failed to create {}", out_dir.display()))?;
        let mut nodes = BTreeMap::new();
        let mut edges = Vec::new();
        let mut seen_citation_ids = BTreeSet::new();

        for paper in papers {
            let paper_id = format!("paper:{}", paper.metadata.paper_id);
            nodes.insert(
                paper_id.clone(),
                Neo4jNode {
                    id: paper_id.clone(),
                    labels: vec!["Paper".into()],
                    properties: serde_json::json!({
                        "paper_id": paper.metadata.paper_id,
                        "citation_key": paper.metadata.citation_key,
                        "title": paper.metadata.title,
                        "year": paper.metadata.year,
                        "arxiv_id": paper.metadata.arxiv_id,
                        "doi": paper.metadata.doi,
                        "url": paper.metadata.url,
                        "parse_status": format!("{:?}", paper.metadata.parse_status),
                    }),
                },
            );

            for (index, section) in paper.sections.iter().enumerate() {
                let section_id = format!("paper:{}:section:{}", paper.metadata.paper_id, index);
                nodes.insert(
                    section_id.clone(),
                    Neo4jNode {
                        id: section_id.clone(),
                        labels: vec!["PaperSection".into()],
                        properties: serde_json::json!({
                            "paper_id": paper.metadata.paper_id,
                            "title": section.title,
                            "level": section.level,
                            "content": section.content,
                        }),
                    },
                );
                edges.push(Neo4jEdge {
                    source: paper_id.clone(),
                    target: section_id,
                    rel_type: "HAS_SECTION".into(),
                    properties: serde_json::json!({}),
                });
            }

            for citation in &paper.citations {
                let citation_id = format!("citation:{citation}");
                if seen_citation_ids.insert(citation_id.clone()) {
                    nodes.insert(
                        citation_id.clone(),
                        Neo4jNode {
                            id: citation_id.clone(),
                            labels: vec!["Citation".into()],
                            properties: serde_json::json!({ "citation_key": citation }),
                        },
                    );
                }
                edges.push(Neo4jEdge {
                    source: paper_id.clone(),
                    target: citation_id,
                    rel_type: "CITES".into(),
                    properties: serde_json::json!({}),
                });
            }
        }

        for edge in infer_enriched_edges(papers) {
            edges.push(Neo4jEdge {
                source: format!("paper:{}", edge.source_paper_id),
                target: format!("paper:{}", edge.target_paper_id),
                rel_type: edge.edge_type.rel_type().into(),
                properties: serde_json::json!({
                    "score": edge.score,
                    "strategy": edge.strategy.as_str(),
                    "evidence": edge.evidence,
                }),
            });
        }

        let memory_bundle = load_project_memory(config, papers)?;
        for memory_node in memory_bundle.nodes {
            nodes.insert(memory_node.id.clone(), neo4j_memory_node(memory_node));
        }
        for surface in memory_bundle.surfaces {
            nodes.insert(surface.id.clone(), neo4j_surface_node(surface));
        }
        for relation in memory_bundle.relations {
            edges.push(Neo4jEdge {
                source: relation.source_id,
                target: relation.target_id,
                rel_type: relation.relation_type.rel_type().into(),
                properties: serde_json::json!({
                    "target_kind": relation.target_kind,
                    "evidence": relation.evidence,
                }),
            });
        }

        let mut nodes = nodes.into_values().collect::<Vec<_>>();
        nodes.sort_by(|left, right| left.id.cmp(&right.id));
        edges.sort_by(|left, right| {
            left.source
                .cmp(&right.source)
                .then_with(|| left.rel_type.cmp(&right.rel_type))
                .then_with(|| left.target.cmp(&right.target))
                .then_with(|| {
                    left.properties
                        .to_string()
                        .cmp(&right.properties.to_string())
                })
        });

        let nodes_path = out_dir.join("nodes.jsonl");
        let edges_path = out_dir.join("edges.jsonl");
        fs::write(&nodes_path, jsonl(&nodes)?)?;
        fs::write(&edges_path, jsonl(&edges)?)?;
        Ok(vec![nodes_path, edges_path])
    }
}

fn neo4j_memory_node(node: MemoryNode) -> Neo4jNode {
    Neo4jNode {
        id: node.id,
        labels: vec!["ProjectMemory".into(), memory_node_label(node.kind).into()],
        properties: serde_json::json!({
            "title": node.title,
            "text": node.text,
            "memory_kind": memory_node_label(node.kind),
            "chunk_kind": memory_chunk_kind_name(node.chunk_kind),
            "source_path": node.source_path,
            "document_id": node.document_id,
            "document_title": node.document_title,
            "section_heading": node.section_heading,
            "section_slug": node.section_slug,
            "chunk_ordinal": node.chunk_ordinal,
            "line_start": node.line_start,
            "line_end": node.line_end,
            "snapshot_kind": node.snapshot_kind,
            "snapshot_value": node.snapshot_value,
            "source_updated": node.source_updated,
            "source_scope": node.source_scope,
            "source_owner": node.source_owner,
            "source_status": node.source_status,
            "tags": node.tags,
        }),
    }
}

fn neo4j_surface_node(surface: MemorySurface) -> Neo4jNode {
    Neo4jNode {
        id: surface.id,
        labels: vec![
            "RepoSurface".into(),
            memory_surface_label(surface.kind).into(),
        ],
        properties: serde_json::json!({
            "surface_kind": memory_surface_kind_name(surface.kind),
            "locator": surface.locator,
            "repo_path": surface.repo_path,
            "symbol": surface.symbol,
            "exists": surface.exists,
        }),
    }
}

fn memory_node_label(kind: MemoryNodeKind) -> &'static str {
    match kind {
        MemoryNodeKind::ProjectState => "ProjectState",
        MemoryNodeKind::Decision => "Decision",
        MemoryNodeKind::OpenQuestion => "OpenQuestion",
        MemoryNodeKind::Gotcha => "Gotcha",
    }
}

fn memory_chunk_kind_name(kind: MemoryChunkKind) -> &'static str {
    match kind {
        MemoryChunkKind::Section => "section",
        MemoryChunkKind::Bullet => "bullet",
    }
}

fn memory_surface_label(kind: MemorySurfaceKind) -> &'static str {
    match kind {
        MemorySurfaceKind::Code => "CodeSurface",
        MemorySurfaceKind::Doc => "DocSurface",
        MemorySurfaceKind::Paper => "PaperSurface",
    }
}

fn memory_surface_kind_name(kind: MemorySurfaceKind) -> &'static str {
    match kind {
        MemorySurfaceKind::Code => "code_surface",
        MemorySurfaceKind::Doc => "doc_surface",
        MemorySurfaceKind::Paper => "paper_surface",
    }
}

fn jsonl<T: Serialize>(items: &[T]) -> Result<String> {
    let mut lines = Vec::with_capacity(items.len());
    for item in items {
        lines.push(serde_json::to_string(item)?);
    }
    Ok(lines.join("\n") + "\n")
}

pub fn load_export_bundle(root: impl AsRef<Path>) -> Result<Neo4jExportBundle> {
    let root = root.as_ref();
    let nodes_path = root.join("nodes.jsonl");
    let edges_path = root.join("edges.jsonl");
    Ok(Neo4jExportBundle {
        root: root.to_path_buf(),
        nodes: read_jsonl(&nodes_path)?,
        edges: read_jsonl(&edges_path)?,
    })
}

fn read_jsonl<T>(path: &Path) -> Result<Vec<T>>
where
    T: for<'de> Deserialize<'de>,
{
    let raw =
        fs::read_to_string(path).with_context(|| format!("Failed to read {}", path.display()))?;
    let mut items = Vec::new();
    for (line_number, line) in raw.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let item = serde_json::from_str(trimmed).with_context(|| {
            format!(
                "Failed to parse JSONL record {} from {}",
                line_number + 1,
                path.display()
            )
        })?;
        items.push(item);
    }
    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;
    use litkg_core::{DownloadMode, PaperSourceRecord, ParseStatus, SinkMode, SourceKind};
    use std::path::Path;

    fn config(root: &Path) -> RepoConfig {
        RepoConfig {
            manifest_path: root.join("sources.jsonl"),
            bib_path: root.join("references.bib"),
            tex_root: root.join("tex"),
            pdf_root: root.join("pdf"),
            generated_docs_root: root.join("generated"),
            registry_path: None,
            parsed_root: None,
            neo4j_export_root: None,
            memory_state_root: None,
            sink: SinkMode::Neo4j,
            graphify_rebuild_command: None,
            download_pdfs: false,
            relevance_tags: vec![],
        }
    }

    fn sample_paper(paper_id: &str, citation_key: &str, title: &str) -> ParsedPaper {
        ParsedPaper {
            metadata: PaperSourceRecord {
                paper_id: paper_id.into(),
                citation_key: Some(citation_key.into()),
                title: title.into(),
                authors: vec![],
                year: Some("2026".into()),
                arxiv_id: None,
                doi: None,
                url: None,
                tex_dir: None,
                pdf_file: None,
                source_kind: SourceKind::ManifestAndBib,
                download_mode: DownloadMode::ManifestSource,
                has_local_tex: true,
                has_local_pdf: false,
                parse_status: ParseStatus::Parsed,
            },
            abstract_text: None,
            sections: vec![],
            figures: vec![],
            tables: vec![],
            citations: vec![],
            provenance: vec![],
        }
    }

    fn write_memory_fixture(root: &Path) {
        fs::create_dir_all(root.join(".agents/memory/state")).unwrap();
        fs::create_dir_all(root.join("docs/typst/paper")).unwrap();
        fs::create_dir_all(root.join("aria_nbv/aria_nbv/data_handling")).unwrap();
        fs::write(root.join("AGENTS.md"), "# Repo guidance\n").unwrap();
        fs::write(root.join("docs/typst/paper/main.typ"), "= Paper\n").unwrap();
        fs::write(
            root.join("aria_nbv/aria_nbv/data_handling/_legacy_cache_api.py"),
            "class Legacy: ...\n",
        )
        .unwrap();
        fs::write(
            root.join(".agents/memory/state/PROJECT_STATE.md"),
            r#"---
id: project_state
updated: 2026-04-13
scope: repo
owner: jan
status: active
tags: [nbv]
---

# Project State

## Current Architecture
Training diagnostics live in `aria_nbv/aria_nbv`, while the paper source of truth stays in `docs/typst/paper/main.typ`.
"#,
        )
        .unwrap();
        fs::write(
            root.join(".agents/memory/state/DECISIONS.md"),
            r#"---
id: decisions
updated: 2026-04-13
scope: repo
owner: jan
status: active
tags: [workflow]
---

# Decisions

## Durable Repo Decisions
- Keep the repo-root `AGENTS.md` thin and policy-only.
"#,
        )
        .unwrap();
        fs::write(
            root.join(".agents/memory/state/OPEN_QUESTIONS.md"),
            r#"---
id: open_questions
updated: 2026-03-24
scope: repo
owner: jan
status: active
tags: [research]
---

# Open Questions

## Research Questions
- Which findings from @efm3d2024 matter most?
"#,
        )
        .unwrap();
        fs::write(
            root.join(".agents/memory/state/GOTCHAS.md"),
            r#"---
id: gotchas
updated: 2026-03-30
scope: repo
owner: jan
status: active
tags: [frames]
---

# Gotchas

## Frames and Geometry
- Use `PoseTW` and `CameraTW` instead of raw matrices.
"#,
        )
        .unwrap();
    }

    #[test]
    fn writes_export_bundle() {
        let dir = tempfile::tempdir().unwrap();
        let config = config(dir.path());
        let mut paper = sample_paper("vista", "zhang2026vistaslam", "ViSTA-SLAM");
        paper.metadata.source_kind = SourceKind::Bib;
        paper.metadata.download_mode = DownloadMode::MetadataOnly;
        paper.metadata.parse_status = ParseStatus::MetadataOnly;
        paper.metadata.has_local_tex = false;
        paper.citations = vec!["foo".into()];
        let papers = vec![paper];

        let written = Neo4jSink::export(&config, &papers).unwrap();
        assert_eq!(written.len(), 2);
        assert!(config.neo4j_export_root().join("nodes.jsonl").is_file());
        assert!(config.neo4j_export_root().join("edges.jsonl").is_file());
    }

    #[test]
    fn deduplicates_shared_citation_nodes() {
        let dir = tempfile::tempdir().unwrap();
        let config = config(dir.path());
        let papers = vec![
            {
                let mut paper = sample_paper("paper-a", "papera2026", "Paper A");
                paper.citations = vec!["shared2026".into()];
                paper
            },
            {
                let mut paper = sample_paper("paper-b", "paperb2026", "Paper B");
                paper.citations = vec!["shared2026".into()];
                paper
            },
        ];

        Neo4jSink::export(&config, &papers).unwrap();
        let nodes = fs::read_to_string(config.neo4j_export_root().join("nodes.jsonl")).unwrap();
        let edges = fs::read_to_string(config.neo4j_export_root().join("edges.jsonl")).unwrap();

        assert_eq!(nodes.matches("\"id\":\"citation:shared2026\"").count(), 1);
        assert_eq!(
            edges.matches("\"target\":\"citation:shared2026\"").count(),
            2
        );
    }

    #[test]
    fn exports_similar_topic_edges() {
        let dir = tempfile::tempdir().unwrap();
        let config = config(dir.path());
        let papers = vec![
            {
                let mut paper = sample_paper(
                    "paper-a",
                    "papera2026",
                    "Stereo visual odometry with bundle adjustment",
                );
                paper.abstract_text =
                    Some("Pose graph refinement for stereo visual odometry.".into());
                paper
            },
            {
                let mut paper = sample_paper(
                    "paper-b",
                    "paperb2026",
                    "Pose graph refinement for stereo odometry",
                );
                paper.abstract_text =
                    Some("Bundle adjustment improves stereo visual tracking.".into());
                paper
            },
        ];

        Neo4jSink::export(&config, &papers).unwrap();
        let edges = fs::read_to_string(config.neo4j_export_root().join("edges.jsonl")).unwrap();

        assert!(edges.contains("\"rel_type\":\"SIMILAR_TOPIC\""));
        assert!(edges.contains("\"strategy\":\"weighted_token_overlap\""));
        assert!(edges.contains("\"source\":\"paper:paper-a\""));
        assert!(edges.contains("\"target\":\"paper:paper-b\""));
    }

    #[test]
    fn exports_resolved_citation_edges() {
        let dir = tempfile::tempdir().unwrap();
        let config = config(dir.path());
        let mut citing = sample_paper(
            "paper-a",
            "papera2026",
            "Stereo visual odometry with bundle adjustment",
        );
        citing.abstract_text = Some("Pose graph refinement for stereo visual odometry.".into());
        citing.citations = vec!["paperb2026".into()];
        let mut target = sample_paper(
            "paper-b",
            "paperb2026",
            "Pose graph refinement for stereo odometry",
        );
        target.abstract_text = Some("Bundle adjustment improves stereo visual tracking.".into());

        Neo4jSink::export(&config, &[citing, target]).unwrap();
        let edges = fs::read_to_string(config.neo4j_export_root().join("edges.jsonl")).unwrap();

        assert!(edges.contains("\"rel_type\":\"CITES_PAPER\""));
        assert!(edges.contains("\"strategy\":\"exact_citation_key\""));
        assert!(edges.contains("\"source\":\"paper:paper-a\""));
        assert!(edges.contains("\"target\":\"paper:paper-b\""));
    }

    #[test]
    fn exports_typed_project_memory_nodes_and_links() {
        let dir = tempfile::tempdir().unwrap();
        write_memory_fixture(dir.path());
        let mut config = config(dir.path());
        config.memory_state_root = Some(dir.path().join(".agents/memory/state"));

        let mut paper = sample_paper("efm3d-foundation", "efm3d2024", "EFM3D");
        paper.metadata.source_kind = SourceKind::Bib;
        paper.metadata.download_mode = DownloadMode::MetadataOnly;
        paper.metadata.parse_status = ParseStatus::MetadataOnly;
        paper.metadata.has_local_tex = false;

        Neo4jSink::export(&config, &[paper]).unwrap();
        let nodes = fs::read_to_string(config.neo4j_export_root().join("nodes.jsonl")).unwrap();
        let edges = fs::read_to_string(config.neo4j_export_root().join("edges.jsonl")).unwrap();

        assert!(nodes.contains("\"labels\":[\"ProjectMemory\",\"ProjectState\"]"));
        assert!(nodes.contains("\"labels\":[\"ProjectMemory\",\"Decision\"]"));
        assert!(nodes.contains("\"labels\":[\"ProjectMemory\",\"OpenQuestion\"]"));
        assert!(nodes.contains("\"labels\":[\"ProjectMemory\",\"Gotcha\"]"));
        assert!(nodes.contains("\"labels\":[\"RepoSurface\",\"CodeSurface\"]"));
        assert!(nodes.contains("\"labels\":[\"RepoSurface\",\"DocSurface\"]"));
        assert!(edges.contains("\"rel_type\":\"DOCUMENTS_CODE\""));
        assert!(edges.contains("\"rel_type\":\"CONSTRAINS\""));
        assert!(edges.contains("\"rel_type\":\"RELATES_TO\""));
        assert!(edges.contains("\"target\":\"paper:efm3d-foundation\""));
    }

    #[test]
    fn exports_memory_only_when_no_parsed_papers_are_available() {
        let dir = tempfile::tempdir().unwrap();
        write_memory_fixture(dir.path());
        let mut config = config(dir.path());
        config.memory_state_root = Some(dir.path().join(".agents/memory/state"));

        Neo4jSink::export(&config, &[]).unwrap();
        let nodes = fs::read_to_string(config.neo4j_export_root().join("nodes.jsonl")).unwrap();
        let edges = fs::read_to_string(config.neo4j_export_root().join("edges.jsonl")).unwrap();

        assert!(nodes.contains("\"labels\":[\"ProjectMemory\",\"ProjectState\"]"));
        assert!(edges.contains("\"rel_type\":\"DOCUMENTS_CODE\""));
    }
}
