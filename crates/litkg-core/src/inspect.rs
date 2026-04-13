use crate::config::RepoConfig;
use crate::materialize::matched_relevance_tags;
use crate::model::{PaperSourceRecord, ParseStatus, ParsedPaper};
use anyhow::{bail, Result};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

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
    pub parsed_json_path: PathBuf,
    pub materialized_markdown_path: PathBuf,
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

pub fn compute_corpus_stats(
    registry: &[PaperSourceRecord],
    parsed_papers: &[ParsedPaper],
) -> CorpusStats {
    let live_parsed = live_parsed_papers(registry, parsed_papers);
    let mut source_kind_counts = BTreeMap::new();
    let mut download_mode_counts = BTreeMap::new();
    let mut parse_status_counts = BTreeMap::new();

    for record in registry {
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
        total_papers: registry.len(),
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
        papers_with_local_tex: registry
            .iter()
            .filter(|record| record.has_local_tex)
            .count(),
        papers_with_local_pdf: registry
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
) -> Result<Vec<SearchHit>> {
    let query = query.trim();
    if limit == 0 {
        bail!("Search limit must be at least 1");
    }
    if query.is_empty() {
        bail!("Search query must not be empty");
    }

    let parsed_by_id: BTreeMap<&str, &ParsedPaper> = parsed_papers
        .iter()
        .map(|paper| (paper.metadata.paper_id.as_str(), paper))
        .collect();
    let query_lower = query.to_lowercase();
    let mut results = Vec::new();

    for record in registry {
        let parsed = parsed_by_id.get(record.paper_id.as_str()).copied();
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
    results.truncate(limit);
    Ok(results)
}

pub fn inspect_paper(
    config: &RepoConfig,
    registry: &[PaperSourceRecord],
    parsed_papers: &[ParsedPaper],
    selector: &str,
) -> Result<PaperInspection> {
    let record = resolve_record(registry, selector)?;
    let live_parsed = live_parsed_papers(registry, parsed_papers);
    let parsed = live_parsed
        .iter()
        .copied()
        .find(|paper| paper.metadata.paper_id == record.paper_id);

    let cited_by = record
        .citation_key
        .as_ref()
        .map(|citation_key| {
            let mut incoming = live_parsed
                .iter()
                .copied()
                .filter(|paper| paper.metadata.paper_id != record.paper_id)
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
        parsed_json_path: config
            .parsed_root()
            .join(format!("{}.json", record.paper_id)),
        materialized_markdown_path: config
            .generated_docs_root
            .join(format!("{}.md", record.paper_id)),
        local_tex_dir: record.tex_dir.as_ref().map(|dir| config.tex_root.join(dir)),
        local_pdf_path: record
            .pdf_file
            .as_ref()
            .map(|file| config.pdf_root.join(file)),
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

fn live_parsed_papers<'a>(
    registry: &'a [PaperSourceRecord],
    parsed_papers: &'a [ParsedPaper],
) -> Vec<&'a ParsedPaper> {
    let registry_ids = registry
        .iter()
        .map(|record| record.paper_id.as_str())
        .collect::<BTreeSet<_>>();
    parsed_papers
        .iter()
        .filter(|paper| registry_ids.contains(paper.metadata.paper_id.as_str()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SinkMode;
    use crate::model::{DownloadMode, PaperFigure, PaperSection, PaperTable, SourceKind};
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
            sink: SinkMode::Graphify,
            graphify_rebuild_command: None,
            download_pdfs: true,
            relevance_tags: vec!["loop closure".into(), "ADVIO".into()],
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
            },
        ]
    }

    fn sample_parsed() -> Vec<ParsedPaper> {
        vec![
            ParsedPaper {
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
                provenance: vec!["manifest".into(), "bib".into()],
            },
            ParsedPaper {
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
                provenance: vec!["manifest".into()],
            },
        ]
    }

    #[test]
    fn computes_corpus_stats_from_registry_and_parsed_papers() {
        let mut parsed = sample_parsed();
        parsed.push(ParsedPaper {
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
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].paper_id, "alpha");
        assert!(hits[0]
            .matched_fields
            .iter()
            .any(|field| field == "paper_id"));
        assert!(hits[0].matched_fields.iter().any(|field| field == "title"));
        assert!(hits[0].relevance_tags.iter().any(|tag| tag == "ADVIO"));

        let content_hits = search_papers(
            &sample_registry(),
            &sample_parsed(),
            &[],
            "loop closure",
            10,
        )
        .unwrap();
        assert_eq!(content_hits[0].paper_id, "alpha");
        assert!(content_hits[0]
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
        assert_eq!(caption_hits[0].paper_id, "alpha");
        assert!(caption_hits[0]
            .matched_fields
            .iter()
            .any(|field| field == "table_captions"));
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
        let mut parsed = sample_parsed();
        parsed.push(ParsedPaper {
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
            },
            abstract_text: None,
            sections: vec![],
            figures: vec![],
            tables: vec![],
            citations: vec!["smith2025alpha".into()],
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
}
