use crate::config::RepoConfig;
use crate::markdown::parse_markdown_document;
use crate::model::{DocumentKind, MaterializedDoc, ParsedPaper};
use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub fn ingest_markdown_docs(
    config: &RepoConfig,
    dir: &Path,
    recursive: bool,
    default_kind: DocumentKind,
) -> Result<Vec<ParsedPaper>> {
    let mut parsed_papers = Vec::new();
    let walker = if recursive {
        WalkDir::new(dir)
    } else {
        WalkDir::new(dir).max_depth(1)
    };

    let repo_root = dir.parent();

    for entry in walker.into_iter().flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let ext = path.extension().and_then(|e| e.to_str());
        if !matches!(ext, Some("md" | "qmd")) {
            continue;
        }

        let parsed = parse_markdown_document(path, repo_root, default_kind)?;
        parsed_papers.push(parsed);
    }

    if !parsed_papers.is_empty() {
        write_parsed_papers(config.parsed_root(), &parsed_papers)?;
    }

    Ok(parsed_papers)
}

pub fn ingest_configured_sources(config: &RepoConfig) -> Result<Vec<ParsedPaper>> {
    let mut all_parsed = Vec::new();
    let project_root = config
        .project
        .as_ref()
        .map(|p| p.root.clone())
        .unwrap_or_else(|| PathBuf::from("."));

    for (name, source) in &config.sources {
        if name == "literature"
            || name == "python"
            || name == "external_docs"
            || name == "web"
            || name == "typst"
        {
            continue; // Handled by other logic or planned
        }

        let kind = if name.contains("transcript") {
            DocumentKind::Transcript
        } else if name.contains("memory") || name.contains("guidance") {
            DocumentKind::ResearchNote
        } else {
            DocumentKind::Documentation
        };

        for pattern in &source.include {
            let full_pattern = project_root.join(pattern);
            let paths = glob::glob(full_pattern.to_str().context("invalid glob pattern")?)
                .with_context(|| format!("failed to expand glob {}", pattern))?;

            for entry in paths.flatten() {
                if !entry.is_file() {
                    continue;
                }

                let is_excluded = source.exclude.iter().any(|ex| {
                    let ex_pattern = project_root.join(ex);
                    if let Ok(mut ex_paths) = glob::glob(ex_pattern.to_str().unwrap_or_default()) {
                        ex_paths.any(|p| p.ok().is_some_and(|p| p == entry))
                    } else {
                        false
                    }
                });

                if is_excluded {
                    continue;
                }

                let ext = entry.extension().and_then(|e| e.to_str());
                if !matches!(ext, Some("md" | "qmd")) {
                    continue;
                }

                let parsed = parse_markdown_document(&entry, Some(&project_root), kind)?;
                all_parsed.push(parsed);
            }
        }
    }

    if !all_parsed.is_empty() {
        write_parsed_papers(config.parsed_root(), &all_parsed)?;
    }

    Ok(all_parsed)
}

pub fn matched_relevance_tags(paper: &ParsedPaper, tags: &[String]) -> Vec<String> {
    let haystack = format!(
        "{}\n{}\n{}",
        paper.metadata.title,
        paper.abstract_text.clone().unwrap_or_default(),
        paper
            .sections
            .iter()
            .map(|section| format!("{}\n{}", section.title, section.content))
            .collect::<Vec<_>>()
            .join("\n")
    )
    .to_lowercase();

    tags.iter()
        .filter(|tag| haystack.contains(&tag.to_lowercase()))
        .cloned()
        .collect()
}

pub fn emit_markdown(config: &RepoConfig, paper: &ParsedPaper) -> MaterializedDoc {
    let tags = matched_relevance_tags(paper, &config.relevance_tags);
    let path = config
        .generated_docs_root
        .join(format!("{}.md", paper.metadata.paper_id));
    let mut lines = vec![
        "---".to_string(),
        format!("paper_id: {}", paper.metadata.paper_id),
        format!(
            "citation_key: {}",
            paper.metadata.citation_key.clone().unwrap_or_default()
        ),
        format!("title: \"{}\"", paper.metadata.title.replace('"', "\\\"")),
        format!("year: {}", paper.metadata.year.clone().unwrap_or_default()),
        format!(
            "arxiv_id: {}",
            paper.metadata.arxiv_id.clone().unwrap_or_default()
        ),
        format!("doi: {}", paper.metadata.doi.clone().unwrap_or_default()),
        format!("url: {}", paper.metadata.url.clone().unwrap_or_default()),
        format!(
            "semantic_scholar_paper_id: {}",
            paper
                .metadata
                .semantic_scholar
                .as_ref()
                .and_then(|item| item.paper_id.clone())
                .unwrap_or_default()
        ),
        format!(
            "semantic_scholar_citation_count: {}",
            paper
                .metadata
                .semantic_scholar
                .as_ref()
                .and_then(|item| item.citation_count)
                .map(|count| count.to_string())
                .unwrap_or_default()
        ),
        format!("source_kind: {:?}", paper.metadata.source_kind),
        format!("download_mode: {:?}", paper.metadata.download_mode),
        format!("has_local_tex: {}", paper.metadata.has_local_tex),
        format!("has_local_pdf: {}", paper.metadata.has_local_pdf),
        format!("parse_status: {:?}", paper.metadata.parse_status),
        format!(
            "kg_tags: [{}]",
            tags.iter()
                .map(|tag| format!("\"{tag}\""))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        "---".to_string(),
        String::new(),
        format!("# {}", paper.metadata.title),
        String::new(),
        "## Metadata".to_string(),
        String::new(),
        format!(
            "- Citation key: {}",
            paper
                .metadata
                .citation_key
                .clone()
                .unwrap_or_else(|| "n/a".into())
        ),
        format!(
            "- Year: {}",
            paper.metadata.year.clone().unwrap_or_else(|| "n/a".into())
        ),
        format!(
            "- arXiv: {}",
            paper
                .metadata
                .arxiv_id
                .clone()
                .unwrap_or_else(|| "n/a".into())
        ),
        format!(
            "- DOI: {}",
            paper.metadata.doi.clone().unwrap_or_else(|| "n/a".into())
        ),
        format!(
            "- URL: {}",
            paper.metadata.url.clone().unwrap_or_else(|| "n/a".into())
        ),
        String::new(),
    ];

    if let Some(semantic_paper) = &paper.metadata.semantic_scholar {
        lines.extend([
            "## Semantic Scholar".to_string(),
            String::new(),
            format!(
                "- Paper ID: {}",
                semantic_paper.paper_id.as_deref().unwrap_or("n/a")
            ),
            format!(
                "- Corpus ID: {}",
                semantic_paper
                    .corpus_id
                    .map(|id| id.to_string())
                    .unwrap_or_else(|| "n/a".into())
            ),
            format!(
                "- Citation count: {}",
                semantic_paper
                    .citation_count
                    .map(|count| count.to_string())
                    .unwrap_or_else(|| "n/a".into())
            ),
            format!(
                "- Influential citation count: {}",
                semantic_paper
                    .influential_citation_count
                    .map(|count| count.to_string())
                    .unwrap_or_else(|| "n/a".into())
            ),
            format!(
                "- Fields of study: {}",
                if semantic_paper.fields_of_study.is_empty() {
                    "n/a".into()
                } else {
                    semantic_paper.fields_of_study.join(", ")
                }
            ),
        ]);
        if let Some(tldr) = semantic_paper
            .tldr
            .as_ref()
            .and_then(|item| item.text.as_deref())
        {
            lines.push(format!("- TLDR: {tldr}"));
        }
        lines.push(String::new());
    }

    lines.extend([
        "## Abstract".to_string(),
        String::new(),
        paper
            .abstract_text
            .clone()
            .or_else(|| {
                paper
                    .metadata
                    .semantic_scholar
                    .as_ref()
                    .and_then(|item| item.abstract_text.clone())
            })
            .unwrap_or_else(|| "No abstract was extracted from local sources.".into()),
        String::new(),
        "## Section Map".to_string(),
        String::new(),
    ]);

    if paper.sections.is_empty() {
        lines.push("- No structured sections were extracted.".to_string());
    } else {
        for section in &paper.sections {
            lines.push(format!("- L{} {}", section.level, section.title));
        }
    }

    lines.push(String::new());
    lines.push("## Main Sections".to_string());
    lines.push(String::new());
    if paper.sections.is_empty() {
        lines.push(
            "No local TeX source was available, so this paper is represented as metadata only."
                .to_string(),
        );
    } else {
        for section in &paper.sections {
            lines.push(format!("### {}", section.title));
            lines.push(String::new());
            lines.push(if section.content.is_empty() {
                "[Section content was empty after TeX normalization.]".to_string()
            } else {
                section.content.clone()
            });
            lines.push(String::new());
        }
    }

    lines.push("## Figures And Tables".to_string());
    lines.push(String::new());
    if paper.figures.is_empty() && paper.tables.is_empty() {
        lines.push("No figure or table captions were extracted.".to_string());
    } else {
        for figure in &paper.figures {
            lines.push(format!("- Figure: {}", figure.caption));
        }
        for table in &paper.tables {
            lines.push(format!("- Table: {}", table.caption));
        }
    }

    lines.push(String::new());
    lines.push("## Citations".to_string());
    lines.push(String::new());
    if paper.citations.is_empty() {
        lines.push("- No citation keys were extracted.".to_string());
    } else {
        for citation in &paper.citations {
            lines.push(format!("- {}", citation));
        }
    }

    lines.push(String::new());
    lines.push("## Repo Relevance".to_string());
    lines.push(String::new());
    if tags.is_empty() {
        lines.push("- No repo relevance tags matched the current config.".to_string());
    } else {
        for tag in tags {
            lines.push(format!("- {}", tag));
        }
    }

    MaterializedDoc {
        path,
        content: lines.join("\n"),
    }
}

pub fn write_materialized_doc(doc: &MaterializedDoc) -> Result<()> {
    if let Some(parent) = doc.path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&doc.path, &doc.content)
        .with_context(|| format!("Failed to write {}", doc.path.display()))?;
    Ok(())
}

pub fn write_parsed_papers(parsed_root: impl AsRef<Path>, papers: &[ParsedPaper]) -> Result<()> {
    let parsed_root = parsed_root.as_ref();
    fs::create_dir_all(parsed_root)?;
    for paper in papers {
        let path = parsed_root.join(format!("{}.json", paper.metadata.paper_id));
        let body = serde_json::to_string_pretty(paper)?;
        fs::write(&path, body).with_context(|| format!("Failed to write {}", path.display()))?;
    }
    Ok(())
}

pub fn load_parsed_papers(parsed_root: impl AsRef<Path>) -> Result<Vec<ParsedPaper>> {
    let parsed_root = parsed_root.as_ref();
    if !parsed_root.exists() {
        return Ok(Vec::new());
    }
    let mut papers = Vec::new();
    let mut paths: Vec<PathBuf> = fs::read_dir(parsed_root)?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("json"))
        .collect();
    paths.sort();
    for path in paths {
        let raw = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read {}", path.display()))?;
        papers.push(
            serde_json::from_str(&raw)
                .with_context(|| format!("Failed to parse {}", path.display()))?,
        );
    }
    Ok(papers)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SinkMode;
    use crate::model::{DownloadMode, PaperSourceRecord, ParseStatus, SourceKind};

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
            sink: SinkMode::Graphify,
            graphify_rebuild_command: None,
            download_pdfs: true,
            relevance_tags: vec!["ViSTA-SLAM".into(), "ADVIO".into()],
            semantic_scholar: None,
        }
    }

    #[test]
    fn emits_markdown_for_metadata_only_paper() {
        let dir = tempfile::tempdir().unwrap();
        let repo_config = config(dir.path());
        let paper = ParsedPaper {
            metadata: PaperSourceRecord {
                paper_id: "vista".into(),
                citation_key: Some("zhang2026vistaslam".into()),
                title: "ViSTA-SLAM".into(),
                authors: vec![],
                year: Some("2026".into()),
                arxiv_id: Some("2509.01584".into()),
                doi: None,
                url: Some("https://arxiv.org/abs/2509.01584".into()),
                tex_dir: None,
                pdf_file: None,
                source_kind: SourceKind::Bib,
                download_mode: DownloadMode::MetadataOnly,
                has_local_tex: false,
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
        };

        let doc = emit_markdown(&repo_config, &paper);
        assert!(doc.content.contains("# ViSTA-SLAM"));
        assert!(doc.content.contains("metadata only"));
    }
}
