use crate::model::{
    DocumentKind, PaperSection, PaperSourceRecord, ParseStatus, ParsedPaper, SourceKind,
};
use anyhow::{Context, Result};
use pulldown_cmark::{Event, Parser, Tag, TagEnd};
use serde::Deserialize;
use std::collections::BTreeSet;
use std::path::Path;

#[derive(Debug, Deserialize, Default)]
struct Frontmatter {
    title: Option<String>,
    author: Option<String>,
    authors: Option<Vec<String>>,
    date: Option<String>,
    _tags: Option<Vec<String>>,
    kind: Option<DocumentKind>,
}

pub fn parse_markdown_document(
    path: &Path,
    repo_root: Option<&Path>,
    default_kind: DocumentKind,
) -> Result<ParsedPaper> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read markdown file {}", path.display()))?;

    let (frontmatter, markdown_content) = parse_frontmatter(&content);
    let mut metadata = build_metadata(path, repo_root, &frontmatter, &default_kind);

    let mut sections = Vec::new();
    let mut current_section_title = String::new();
    let mut current_section_content = String::new();
    let mut current_level = 0u8;

    let mut citations = BTreeSet::new();
    let figures = Vec::new();
    let tables = Vec::new();

    let parser = Parser::new(markdown_content);
    let mut in_heading = false;
    let mut in_blockquote = false;
    let mut in_reasoning = false;

    for event in parser {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                if !current_section_title.is_empty() || !current_section_content.is_empty() {
                    sections.push(PaperSection {
                        level: current_level,
                        title: current_section_title.clone(),
                        content: current_section_content.trim().to_string(),
                    });
                }
                current_section_title.clear();
                current_section_content.clear();
                current_level = level as u8;
                in_heading = true;
            }
            Event::End(TagEnd::Heading(_)) => {
                in_heading = false;
            }
            Event::Start(Tag::BlockQuote) => {
                in_blockquote = true;
                current_section_content.push_str("\n> ");
            }
            Event::End(TagEnd::BlockQuote) => {
                in_blockquote = false;
                in_reasoning = false; // Reset if blockquote ends
                current_section_content.push('\n');
            }
            Event::Text(text) => {
                if in_heading {
                    current_section_title.push_str(&text);
                } else {
                    let mut content_to_add = text.to_string();

                    if content_to_add.contains("::: {.reasoning}")
                        || content_to_add.contains("<reasoning>")
                    {
                        in_reasoning = true;
                        continue; // Skip the marker itself
                    } else if content_to_add.trim() == ":::"
                        || content_to_add.contains("</reasoning>")
                    {
                        in_reasoning = false;
                        continue; // Skip the marker itself
                    }

                    if in_blockquote
                        && (content_to_add.to_lowercase().starts_with("reasoning:")
                            || content_to_add.to_lowercase().starts_with("thought:"))
                    {
                        in_reasoning = true;
                    }

                    if in_reasoning {
                        content_to_add = format!("[Reasoning: {}]", content_to_add.trim());
                    }
                    current_section_content.push_str(&content_to_add);
                }
                // Extract @citations
                extract_citations_from_text(&text, &mut citations);
            }
            Event::Code(code) => {
                current_section_content.push_str(&format!("`{}`", code));
                extract_citations_from_text(&code, &mut citations);
            }
            Event::Start(Tag::Image { .. }) => {
                // Placeholder for figure extraction
            }
            Event::Start(Tag::Table(_)) => {
                // Placeholder for table extraction
            }
            _ => {}
        }
    }

    if !current_section_title.is_empty() || !current_section_content.is_empty() {
        sections.push(PaperSection {
            level: current_level,
            title: current_section_title,
            content: current_section_content.trim().to_string(),
        });
    }

    // If no title found in headers, use filename or frontmatter
    if metadata.title.is_empty() {
        metadata.title = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Untitled")
            .to_string();
    }

    Ok(ParsedPaper {
        kind: frontmatter.kind.unwrap_or(default_kind),
        metadata,
        abstract_text: None,
        sections,
        figures,
        tables,
        citations: citations.into_iter().collect(),
        citation_references: Vec::new(), // Resolved later
        provenance: vec![path.display().to_string()],
    })
}

fn parse_frontmatter(content: &str) -> (Frontmatter, &str) {
    if !content.starts_with("---\n") {
        return (Frontmatter::default(), content);
    }
    let Some(end) = content[4..].find("---\n") else {
        return (Frontmatter::default(), content);
    };
    let yaml = &content[4..4 + end];
    let frontmatter = serde_yaml::from_str(yaml).unwrap_or_default();
    (frontmatter, &content[4 + end + 4..])
}

fn build_metadata(
    path: &Path,
    _repo_root: Option<&Path>,
    frontmatter: &Frontmatter,
    default_kind: &DocumentKind,
) -> PaperSourceRecord {
    let title = frontmatter.title.clone().unwrap_or_default();
    let authors = if let Some(a) = &frontmatter.author {
        vec![a.clone()]
    } else {
        frontmatter.authors.clone().unwrap_or_default()
    };

    let kind = frontmatter.kind.as_ref().unwrap_or(default_kind);
    let source_kind = match kind {
        DocumentKind::Transcript => SourceKind::Transcript,
        _ => SourceKind::Documentation,
    };

    PaperSourceRecord {
        paper_id: slug::slugify(path.file_stem().and_then(|s| s.to_str()).unwrap_or("doc")),
        citation_key: None,
        title,
        authors,
        year: frontmatter.date.clone(),
        arxiv_id: None,
        doi: None,
        url: None,
        tex_dir: None,
        pdf_file: None,
        source_kind,
        download_mode: crate::model::DownloadMode::MetadataOnly,
        has_local_tex: false,
        has_local_pdf: false,
        parse_status: ParseStatus::Parsed,
        semantic_scholar: None,
    }
}

fn extract_citations_from_text(text: &str, citations: &mut BTreeSet<String>) {
    // Basic @citation extraction
    let re = regex::Regex::new(r"@([a-zA-Z0-9_:-]+)").unwrap();
    for cap in re.captures_iter(text) {
        let key = cap[1].to_string();
        if !key.chars().next().unwrap_or(' ').is_numeric() {
            citations.insert(key);
        }
    }
}
