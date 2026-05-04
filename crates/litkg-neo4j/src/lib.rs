use anyhow::{Context, Result};
use litkg_core::{
    build_python_code_graph, infer_enriched_edges, load_project_memory, CodeCall, CodeImport,
    MemoryChunkKind, MemoryNode, MemoryNodeKind, MemorySurface, MemorySurfaceKind, ParsedPaper,
    RepoConfig,
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
                        "semantic_scholar_paper_id": paper.metadata.semantic_scholar.as_ref().and_then(|item| item.paper_id.clone()),
                        "semantic_scholar_corpus_id": paper.metadata.semantic_scholar.as_ref().and_then(|item| item.corpus_id),
                        "citation_count": paper.metadata.semantic_scholar.as_ref().and_then(|item| item.citation_count),
                        "influential_citation_count": paper.metadata.semantic_scholar.as_ref().and_then(|item| item.influential_citation_count),
                        "reference_count": paper.metadata.semantic_scholar.as_ref().and_then(|item| item.reference_count),
                        "semantic_scholar_fields": paper.metadata.semantic_scholar.as_ref().map(|item| item.fields_of_study.clone()).unwrap_or_default(),
                    }),
                },
            );

            if let Some(semantic_paper) = &paper.metadata.semantic_scholar {
                for author in &semantic_paper.authors {
                    let author_id = author
                        .author_id
                        .as_ref()
                        .map(|id| format!("author:{id}"))
                        .unwrap_or_else(|| format!("author:name:{}", slugify(&author.name)));
                    nodes.insert(
                        author_id.clone(),
                        Neo4jNode {
                            id: author_id.clone(),
                            labels: vec!["Author".into()],
                            properties: serde_json::json!({
                                "author_id": author.author_id,
                                "name": author.name,
                                "url": author.url,
                                "paper_count": author.paper_count,
                                "citation_count": author.citation_count,
                                "h_index": author.h_index,
                                "affiliations": author.affiliations,
                            }),
                        },
                    );
                    edges.push(Neo4jEdge {
                        source: paper_id.clone(),
                        target: author_id,
                        rel_type: "AUTHORED_BY".into(),
                        properties: serde_json::json!({"source": "semantic_scholar"}),
                    });
                }

                for field in &semantic_paper.fields_of_study {
                    let field_id = format!("field:{}", slugify(field));
                    nodes.insert(
                        field_id.clone(),
                        Neo4jNode {
                            id: field_id.clone(),
                            labels: vec!["FieldOfStudy".into()],
                            properties: serde_json::json!({"name": field}),
                        },
                    );
                    edges.push(Neo4jEdge {
                        source: paper_id.clone(),
                        target: field_id,
                        rel_type: "HAS_FIELD".into(),
                        properties: serde_json::json!({"source": "semantic_scholar"}),
                    });
                }

                for (kind, value) in &semantic_paper.external_ids {
                    let external_id = format!("external_id:{}:{}", slugify(kind), slugify(value));
                    nodes.insert(
                        external_id.clone(),
                        Neo4jNode {
                            id: external_id.clone(),
                            labels: vec!["ExternalId".into()],
                            properties: serde_json::json!({
                                "kind": kind,
                                "value": value,
                            }),
                        },
                    );
                    edges.push(Neo4jEdge {
                        source: paper_id.clone(),
                        target: external_id,
                        rel_type: "HAS_EXTERNAL_ID".into(),
                        properties: serde_json::json!({"source": "semantic_scholar"}),
                    });
                }
            }

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

        add_generated_context_nodes(config, &mut nodes)?;

        let code_graph = build_python_code_graph(config)?;
        for file in code_graph.files {
            nodes.insert(
                file.id.clone(),
                Neo4jNode {
                    id: file.id,
                    labels: vec!["CodeFile".into()],
                    properties: serde_json::json!({
                        "repo_path": file.repo_path,
                        "module": file.module,
                        "line_count": file.line_count,
                        "source": "python_ast",
                    }),
                },
            );
        }
        for module in code_graph.modules {
            nodes.insert(
                module.id.clone(),
                Neo4jNode {
                    id: module.id,
                    labels: vec!["CodeModule".into()],
                    properties: serde_json::json!({
                        "name": module.name,
                        "file_id": module.file_id,
                        "repo_path": module.repo_path,
                        "source": "python_ast",
                    }),
                },
            );
        }
        for symbol in code_graph.symbols {
            nodes.insert(
                symbol.id.clone(),
                Neo4jNode {
                    id: symbol.id,
                    labels: vec!["CodeSymbol".into()],
                    properties: serde_json::json!({
                        "qualified_name": symbol.qualified_name,
                        "name": symbol.name,
                        "symbol_kind": symbol.kind.as_str(),
                        "module": symbol.module,
                        "file_id": symbol.file_id,
                        "repo_path": symbol.repo_path,
                        "parent_id": symbol.parent_id,
                        "line_start": symbol.line_start,
                        "line_end": symbol.line_end,
                        "signature": symbol.signature,
                        "doc_summary": symbol.doc_summary,
                        "source": "python_ast",
                    }),
                },
            );
        }
        for containment in code_graph.contains {
            edges.push(Neo4jEdge {
                source: containment.source_id,
                target: containment.target_id,
                rel_type: containment.rel_type,
                properties: serde_json::json!({"source": "python_ast"}),
            });
        }
        for import in code_graph.imports {
            let target = code_import_target(&import, &mut nodes);
            edges.push(Neo4jEdge {
                source: import.source_id,
                target,
                rel_type: "IMPORTS".into(),
                properties: serde_json::json!({
                    "imported": import.imported,
                    "alias": import.alias,
                    "line": import.line,
                    "source": "python_ast",
                    "resolved": import.target_id.is_some(),
                }),
            });
        }
        for call in code_graph.calls {
            let Some(target) = code_call_target(&call) else {
                continue;
            };
            edges.push(Neo4jEdge {
                source: call.source_id,
                target,
                rel_type: "CALLS".into(),
                properties: serde_json::json!({
                    "target": call.target,
                    "line": call.line,
                    "source": "python_ast",
                    "resolved": call.target_id.is_some(),
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

fn code_import_target(import: &CodeImport, nodes: &mut BTreeMap<String, Neo4jNode>) -> String {
    import
        .target_id
        .clone()
        .unwrap_or_else(|| insert_code_reference(nodes, "import", import.imported.as_str()))
}

fn code_call_target(call: &CodeCall) -> Option<String> {
    call.target_id.clone()
}

fn insert_code_reference(
    nodes: &mut BTreeMap<String, Neo4jNode>,
    reference_kind: &str,
    name: &str,
) -> String {
    let id = format!("code_ref:{}:{}", reference_kind, slugify(name));
    nodes.entry(id.clone()).or_insert_with(|| Neo4jNode {
        id: id.clone(),
        labels: vec!["CodeReference".into()],
        properties: serde_json::json!({
            "name": name,
            "reference_kind": reference_kind,
            "source": "python_ast",
        }),
    });
    id
}

fn add_generated_context_nodes(
    config: &RepoConfig,
    nodes: &mut BTreeMap<String, Neo4jNode>,
) -> Result<()> {
    let repo_root = config_repo_root(config);
    for (stem, title) in [
        ("source_index", "Context Sources Index"),
        ("literature_index", "Literature Source Index"),
        ("data_contracts", "Data Contracts"),
    ] {
        let source_path = format!("docs/_generated/context/{stem}.md");
        let path = repo_root.join(&source_path);
        if !path.is_file() {
            continue;
        }
        let content = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read {}", path.display()))?;
        let id = format!("generated_context:{stem}");
        nodes.insert(
            id.clone(),
            Neo4jNode {
                id,
                labels: vec!["GeneratedContext".into()],
                properties: serde_json::json!({
                    "title": title,
                    "source_path": source_path,
                    "content": content,
                    "source": "make_context",
                }),
            },
        );
        if stem == "data_contracts" {
            add_data_contract_nodes(&content, &source_path, nodes);
        }
    }

    add_glossary_nodes(&repo_root, nodes)?;
    Ok(())
}

fn add_data_contract_nodes(
    content: &str,
    source_path: &str,
    nodes: &mut BTreeMap<String, Neo4jNode>,
) {
    let mut current_title = None::<String>;
    let mut current_start = 0usize;
    let mut current_lines = Vec::new();
    for (line_index, line) in content.lines().enumerate() {
        if let Some(title) = line.strip_prefix("## ") {
            if let Some(previous_title) = current_title.take() {
                insert_data_contract_node(
                    previous_title,
                    source_path,
                    current_start,
                    &current_lines,
                    nodes,
                );
            }
            current_title = Some(title.trim().to_string());
            current_start = line_index + 1;
            current_lines.clear();
        } else if current_title.is_some() {
            current_lines.push(line.to_string());
        }
    }
    if let Some(title) = current_title {
        insert_data_contract_node(title, source_path, current_start, &current_lines, nodes);
    }
}

fn insert_data_contract_node(
    title: String,
    source_path: &str,
    line_start: usize,
    lines: &[String],
    nodes: &mut BTreeMap<String, Neo4jNode>,
) {
    let content = lines.join("\n").trim().to_string();
    if title == "Data Contracts (aria_nbv)" || content.is_empty() {
        return;
    }
    let summary = content
        .lines()
        .find(|line| {
            let trimmed = line.trim();
            !trimmed.is_empty() && !trimmed.starts_with('-') && !trimmed.ends_with(':')
        })
        .unwrap_or_default()
        .trim()
        .to_string();
    let id = format!("data_contract:{}", slugify(&title));
    nodes.insert(
        id.clone(),
        Neo4jNode {
            id,
            labels: vec!["DataContract".into(), "GeneratedContext".into()],
            properties: serde_json::json!({
                "title": title,
                "summary": summary,
                "content": content,
                "source_path": source_path,
                "line_start": line_start,
                "line_end": line_start + lines.len(),
                "source": "make_context",
            }),
        },
    );
}

fn add_glossary_nodes(repo_root: &Path, nodes: &mut BTreeMap<String, Neo4jNode>) -> Result<()> {
    let source_path = "docs/_generated/context/glossary.jsonl";
    let path = repo_root.join(source_path);
    if !path.is_file() {
        return Ok(());
    }
    let raw =
        fs::read_to_string(&path).with_context(|| format!("Failed to read {}", path.display()))?;
    for line in raw.lines().filter(|line| !line.trim().is_empty()) {
        let value: serde_json::Value =
            serde_json::from_str(line).context("Failed to parse generated glossary JSONL row")?;
        let Some(id_value) = value.get("id").and_then(|value| value.as_str()) else {
            continue;
        };
        let id = format!("concept:{}", slugify(id_value));
        nodes.insert(
            id.clone(),
            Neo4jNode {
                id,
                labels: vec!["Concept".into(), "GeneratedContext".into()],
                properties: serde_json::json!({
                    "title": value.get("label").and_then(|value| value.as_str()).unwrap_or(id_value),
                    "name": id_value,
                    "label": value.get("label").cloned().unwrap_or(serde_json::Value::Null),
                    "short": value.get("short").cloned().unwrap_or(serde_json::Value::Null),
                    "definition_short": value.get("definition_short").cloned().unwrap_or(serde_json::Value::Null),
                    "definition_long": value.get("definition_long").cloned().unwrap_or(serde_json::Value::Null),
                    "aliases": value.get("aliases").cloned().unwrap_or(serde_json::Value::Array(vec![])),
                    "kg_tags": value.get("kg_tags").cloned().unwrap_or(serde_json::Value::Array(vec![])),
                    "internal_links": value.get("internal_links").cloned().unwrap_or(serde_json::Value::Array(vec![])),
                    "source_path": source_path,
                    "source": "make_context",
                }),
            },
        );
    }
    Ok(())
}

fn config_repo_root(config: &RepoConfig) -> PathBuf {
    let root = config
        .project
        .as_ref()
        .map(|project| project.root.clone())
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| PathBuf::from("."));
    if root.is_absolute() {
        root
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(root)
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

fn slugify(text: &str) -> String {
    let mut slug = String::new();
    let mut last_dash = false;
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            slug.push('-');
            last_dash = true;
        }
    }
    slug.trim_matches('-').to_string()
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
    use litkg_core::{
        DocumentKind, DownloadMode, PaperSourceRecord, ParseStatus, SinkMode, SourceKind,
    };
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
            semantic_scholar: None,
            project: None,
            sources: std::collections::BTreeMap::new(),
            representation: None,
            backends: None,
            storage: None,
        }
    }

    fn sample_paper(paper_id: &str, citation_key: &str, title: &str) -> ParsedPaper {
        ParsedPaper {
            kind: DocumentKind::Literature,
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
                semantic_scholar: None,
            },
            abstract_text: None,
            sections: vec![],
            figures: vec![],
            tables: vec![],
            citations: vec![],
            citation_references: Vec::new(),
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

    #[test]
    fn exports_ast_backed_python_code_graph() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("pkg")).unwrap();
        fs::write(dir.path().join("pkg/__init__.py"), "").unwrap();
        fs::write(
            dir.path().join("pkg/a.py"),
            r#"
from .b import helper

class Runner:
    def run(self):
        return helper()
"#,
        )
        .unwrap();
        fs::write(
            dir.path().join("pkg/b.py"),
            r#"
def helper():
    return 1
"#,
        )
        .unwrap();
        let mut config = config(dir.path());
        config.project = Some(litkg_core::config::ProjectConfig {
            id: "fixture".into(),
            name: "Fixture".into(),
            root: dir.path().to_path_buf(),
        });
        config.sources.insert(
            "python".into(),
            litkg_core::config::SourceConfig {
                required: true,
                include: vec!["pkg/**/*.py".into()],
                symbols: true,
                edges: Some("codegraphcontext".into()),
                ..Default::default()
            },
        );

        Neo4jSink::export(&config, &[]).unwrap();
        let nodes = fs::read_to_string(config.neo4j_export_root().join("nodes.jsonl")).unwrap();
        let edges = fs::read_to_string(config.neo4j_export_root().join("edges.jsonl")).unwrap();

        assert!(nodes.contains("\"labels\":[\"CodeFile\"]"));
        assert!(nodes.contains("\"labels\":[\"CodeSymbol\"]"));
        assert!(nodes.contains("\"qualified_name\":\"pkg.a.Runner.run\""));
        assert!(edges.contains("\"rel_type\":\"IMPORTS\""));
        assert!(edges.contains("\"target\":\"code_symbol:pkg.b.helper\""));
        assert!(edges.contains("\"rel_type\":\"CALLS\""));
    }

    #[test]
    fn exports_generated_context_nodes() {
        let dir = tempfile::tempdir().unwrap();
        let context_root = dir.path().join("docs/_generated/context");
        fs::create_dir_all(&context_root).unwrap();
        fs::write(
            context_root.join("source_index.md"),
            "# Context Sources Index\n\nPython source and docs.\n",
        )
        .unwrap();
        fs::write(
            context_root.join("literature_index.md"),
            "# Literature Source Index\n\nVIN-NBV paper family.\n",
        )
        .unwrap();
        fs::write(
            context_root.join("data_contracts.md"),
            "# Data Contracts\n\n## app.state.VinPrediction\nPrediction contract.\n\nFields:\n- score: float\n",
        )
        .unwrap();
        fs::write(
            context_root.join("glossary.jsonl"),
            "{\"id\":\"relative-reconstruction-improvement\",\"label\":\"Relative Reconstruction Improvement\",\"definition_short\":\"Oracle reconstruction gain.\",\"aliases\":[\"RRI\"],\"kg_tags\":[\"metric\"],\"internal_links\":[]}\n",
        )
        .unwrap();
        let mut config = config(dir.path());
        config.project = Some(litkg_core::config::ProjectConfig {
            id: "fixture".into(),
            name: "Fixture".into(),
            root: dir.path().to_path_buf(),
        });

        Neo4jSink::export(&config, &[]).unwrap();
        let nodes = fs::read_to_string(config.neo4j_export_root().join("nodes.jsonl")).unwrap();

        assert!(nodes.contains("\"labels\":[\"GeneratedContext\"]"));
        assert!(nodes.contains("\"labels\":[\"DataContract\",\"GeneratedContext\"]"));
        assert!(nodes.contains("\"labels\":[\"Concept\",\"GeneratedContext\"]"));
        assert!(nodes.contains("app.state.VinPrediction"));
        assert!(nodes.contains("Relative Reconstruction Improvement"));
    }
}
