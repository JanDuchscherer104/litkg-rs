use anyhow::{Context, Result};
use litkg_core::{infer_enriched_edges, ParsedPaper, RepoConfig};
use serde::Serialize;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Serialize)]
struct Neo4jNode {
    id: String,
    labels: Vec<String>,
    properties: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct Neo4jEdge {
    source: String,
    target: String,
    rel_type: String,
    properties: serde_json::Value,
}

pub struct Neo4jSink;

impl Neo4jSink {
    pub fn export(config: &RepoConfig, papers: &[ParsedPaper]) -> Result<Vec<PathBuf>> {
        let out_dir = config.neo4j_export_root();
        fs::create_dir_all(&out_dir)
            .with_context(|| format!("Failed to create {}", out_dir.display()))?;
        let mut nodes = Vec::new();
        let mut edges = Vec::new();

        for paper in papers {
            let paper_id = format!("paper:{}", paper.metadata.paper_id);
            nodes.push(Neo4jNode {
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
            });

            for (index, section) in paper.sections.iter().enumerate() {
                let section_id = format!("paper:{}:section:{}", paper.metadata.paper_id, index);
                nodes.push(Neo4jNode {
                    id: section_id.clone(),
                    labels: vec!["PaperSection".into()],
                    properties: serde_json::json!({
                        "paper_id": paper.metadata.paper_id,
                        "title": section.title,
                        "level": section.level,
                        "content": section.content,
                    }),
                });
                edges.push(Neo4jEdge {
                    source: paper_id.clone(),
                    target: section_id,
                    rel_type: "HAS_SECTION".into(),
                    properties: serde_json::json!({}),
                });
            }

            for citation in &paper.citations {
                let citation_id = format!("citation:{citation}");
                nodes.push(Neo4jNode {
                    id: citation_id.clone(),
                    labels: vec!["Citation".into()],
                    properties: serde_json::json!({ "citation_key": citation }),
                });
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

        let nodes_path = out_dir.join("nodes.jsonl");
        let edges_path = out_dir.join("edges.jsonl");
        fs::write(&nodes_path, jsonl(&nodes)?)?;
        fs::write(&edges_path, jsonl(&edges)?)?;
        Ok(vec![nodes_path, edges_path])
    }
}

fn jsonl<T: Serialize>(items: &[T]) -> Result<String> {
    let mut lines = Vec::with_capacity(items.len());
    for item in items {
        lines.push(serde_json::to_string(item)?);
    }
    Ok(lines.join("\n") + "\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use litkg_core::{DownloadMode, PaperSourceRecord, ParseStatus, SinkMode, SourceKind};

    #[test]
    fn writes_export_bundle() {
        let dir = tempfile::tempdir().unwrap();
        let config = RepoConfig {
            manifest_path: dir.path().join("sources.jsonl"),
            bib_path: dir.path().join("references.bib"),
            tex_root: dir.path().join("tex"),
            pdf_root: dir.path().join("pdf"),
            generated_docs_root: dir.path().join("generated"),
            registry_path: None,
            parsed_root: None,
            neo4j_export_root: None,
            sink: SinkMode::Neo4j,
            graphify_rebuild_command: None,
            download_pdfs: false,
            relevance_tags: vec![],
        };
        let papers = vec![ParsedPaper {
            metadata: PaperSourceRecord {
                paper_id: "vista".into(),
                citation_key: Some("zhang2026vistaslam".into()),
                title: "ViSTA-SLAM".into(),
                authors: vec![],
                year: Some("2026".into()),
                arxiv_id: Some("2509.01584".into()),
                doi: None,
                url: None,
                tex_dir: None,
                pdf_file: None,
                source_kind: SourceKind::Bib,
                download_mode: DownloadMode::MetadataOnly,
                has_local_tex: false,
                has_local_pdf: false,
                parse_status: ParseStatus::MetadataOnly,
            },
            abstract_text: None,
            sections: vec![],
            figures: vec![],
            tables: vec![],
            citations: vec!["foo".into()],
            provenance: vec![],
        }];

        let written = Neo4jSink::export(&config, &papers).unwrap();
        assert_eq!(written.len(), 2);
        assert!(config.neo4j_export_root().join("nodes.jsonl").is_file());
        assert!(config.neo4j_export_root().join("edges.jsonl").is_file());
    }

    #[test]
    fn exports_similar_topic_edges() {
        let dir = tempfile::tempdir().unwrap();
        let config = RepoConfig {
            manifest_path: dir.path().join("sources.jsonl"),
            bib_path: dir.path().join("references.bib"),
            tex_root: dir.path().join("tex"),
            pdf_root: dir.path().join("pdf"),
            generated_docs_root: dir.path().join("generated"),
            registry_path: None,
            parsed_root: None,
            neo4j_export_root: None,
            sink: SinkMode::Neo4j,
            graphify_rebuild_command: None,
            download_pdfs: false,
            relevance_tags: vec![],
        };
        let papers = vec![
            ParsedPaper {
                metadata: PaperSourceRecord {
                    paper_id: "paper-a".into(),
                    citation_key: Some("papera2026".into()),
                    title: "Stereo visual odometry with bundle adjustment".into(),
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
                abstract_text: Some("Pose graph refinement for stereo visual odometry.".into()),
                sections: vec![],
                figures: vec![],
                tables: vec![],
                citations: vec![],
                provenance: vec![],
            },
            ParsedPaper {
                metadata: PaperSourceRecord {
                    paper_id: "paper-b".into(),
                    citation_key: Some("paperb2026".into()),
                    title: "Pose graph refinement for stereo odometry".into(),
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
                abstract_text: Some("Bundle adjustment improves stereo visual tracking.".into()),
                sections: vec![],
                figures: vec![],
                tables: vec![],
                citations: vec![],
                provenance: vec![],
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
        let config = RepoConfig {
            manifest_path: dir.path().join("sources.jsonl"),
            bib_path: dir.path().join("references.bib"),
            tex_root: dir.path().join("tex"),
            pdf_root: dir.path().join("pdf"),
            generated_docs_root: dir.path().join("generated"),
            registry_path: None,
            parsed_root: None,
            neo4j_export_root: None,
            sink: SinkMode::Neo4j,
            graphify_rebuild_command: None,
            download_pdfs: false,
            relevance_tags: vec![],
        };
        let citing = ParsedPaper {
            metadata: PaperSourceRecord {
                paper_id: "paper-a".into(),
                citation_key: Some("papera2026".into()),
                title: "Stereo visual odometry with bundle adjustment".into(),
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
            abstract_text: Some("Pose graph refinement for stereo visual odometry.".into()),
            sections: vec![],
            figures: vec![],
            tables: vec![],
            citations: vec!["paperb2026".into()],
            provenance: vec![],
        };
        let target = ParsedPaper {
            metadata: PaperSourceRecord {
                paper_id: "paper-b".into(),
                citation_key: Some("paperb2026".into()),
                title: "Pose graph refinement for stereo odometry".into(),
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
            abstract_text: Some("Bundle adjustment improves stereo visual tracking.".into()),
            sections: vec![],
            figures: vec![],
            tables: vec![],
            citations: vec![],
            provenance: vec![],
        };

        Neo4jSink::export(&config, &[citing, target]).unwrap();
        let edges = fs::read_to_string(config.neo4j_export_root().join("edges.jsonl")).unwrap();

        assert!(edges.contains("\"rel_type\":\"CITES_PAPER\""));
        assert!(edges.contains("\"strategy\":\"exact_citation_key\""));
        assert!(edges.contains("\"source\":\"paper:paper-a\""));
        assert!(edges.contains("\"target\":\"paper:paper-b\""));
    }
}
