use crate::bibtex::{parse_bibtex, BibEntry};
use crate::config::RepoConfig;
use crate::model::{
    CitationReference, DocumentKind, PaperFigure, PaperSection, PaperSourceRecord, PaperTable,
    ParseStatus, ParsedPaper,
};
use anyhow::{Context, Result};
use regex::Regex;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, OnceLock};
use std::thread;
use walkdir::WalkDir;

const MAX_PARSE_THREADS: usize = 8;

pub fn parse_registry_papers(
    config: &RepoConfig,
    registry: &[PaperSourceRecord],
) -> Result<Vec<ParsedPaper>> {
    if registry.is_empty() {
        return Ok(Vec::new());
    }

    let worker_count = thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(1)
        .min(registry.len())
        .min(MAX_PARSE_THREADS);

    if worker_count <= 1 {
        let mut parsed = Vec::with_capacity(registry.len());
        for record in registry {
            parsed.push(parse_registry_record(config, record)?);
        }
        return Ok(parsed);
    }

    let (sender, receiver) = mpsc::channel::<Vec<(usize, Result<ParsedPaper>)>>();
    let mut by_index: Vec<Option<Result<ParsedPaper>>> =
        (0..registry.len()).map(|_| None).collect();

    thread::scope(|scope| {
        for worker_index in 0..worker_count {
            let sender = sender.clone();
            scope.spawn(move || {
                let mut batch = Vec::new();
                for index in (worker_index..registry.len()).step_by(worker_count) {
                    batch.push((index, parse_registry_record(config, &registry[index])));
                }
                let _ = sender.send(batch);
            });
        }
        drop(sender);
        for batch in receiver {
            for (index, parsed) in batch {
                by_index[index] = Some(parsed);
            }
        }
    });

    let mut parsed = Vec::with_capacity(registry.len());
    for entry in by_index {
        parsed.push(entry.expect("parse worker did not return a result")?);
    }
    Ok(parsed)
}

fn parse_registry_record(config: &RepoConfig, record: &PaperSourceRecord) -> Result<ParsedPaper> {
    if let Some(tex_dir) = &record.tex_dir {
        let root_dir = config.tex_root.join(tex_dir);
        if path_has_files(&root_dir) {
            return parse_paper_dir(record, &root_dir);
        }
    }
    Ok(metadata_only_paper(record))
}

fn parse_paper_dir(record: &PaperSourceRecord, root_dir: &Path) -> Result<ParsedPaper> {
    let root_file = discover_root_tex(root_dir)?
        .with_context(|| format!("No TeX root found under {}", root_dir.display()))?;
    let merged = inline_tex_file(&root_file, root_dir, &mut BTreeSet::new())?;
    let abstract_text = extract_environment(&merged, "abstract");
    let sections = extract_sections(&merged);
    let figures = extract_figure_captions(&merged)
        .into_iter()
        .map(|caption| PaperFigure { caption })
        .collect();
    let tables = extract_table_captions(&merged)
        .into_iter()
        .map(|caption| PaperTable { caption })
        .collect();
    let citations = extract_citations(&merged);
    let citation_references = extract_citation_references(root_dir, &citations)?;
    let title = extract_command_value(&merged, "title").unwrap_or_else(|| record.title.clone());

    let mut metadata = record.clone();
    metadata.title = title;
    metadata.parse_status = ParseStatus::Parsed;

    Ok(ParsedPaper {
        kind: DocumentKind::Literature,
        metadata,
        abstract_text,
        sections,
        figures,
        tables,
        citations,
        citation_references,
        provenance: vec![root_file.display().to_string()],
    })
}

fn metadata_only_paper(record: &PaperSourceRecord) -> ParsedPaper {
    ParsedPaper {
        kind: DocumentKind::Literature,
        metadata: record.clone(),
        abstract_text: None,
        sections: Vec::new(),
        figures: Vec::new(),
        tables: Vec::new(),
        citations: Vec::new(),
        citation_references: Vec::new(),
        provenance: Vec::new(),
    }
}

fn discover_root_tex(root_dir: &Path) -> Result<Option<PathBuf>> {
    let main = root_dir.join("main.tex");
    if main.is_file() {
        return Ok(Some(main));
    }
    for entry in WalkDir::new(root_dir).into_iter().flatten() {
        let path = entry.path();
        if !path.is_file() || path.extension().and_then(|ext| ext.to_str()) != Some("tex") {
            continue;
        }
        let text = fs::read_to_string(path)?;
        if text.contains("\\documentclass") && text.contains("\\begin{document}") {
            return Ok(Some(path.to_path_buf()));
        }
    }
    Ok(None)
}

fn inline_tex_file(
    path: &Path,
    root_dir: &Path,
    visited: &mut BTreeSet<PathBuf>,
) -> Result<String> {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    if !visited.insert(canonical.clone()) {
        return Ok(String::new());
    }
    let text = fs::read_to_string(&canonical)
        .with_context(|| format!("Failed to read {}", canonical.display()))?;
    let stripped = strip_comments(&text);
    let mut result = String::new();
    let mut last_end = 0usize;
    for capture in include_regex().captures_iter(&stripped) {
        let matched = capture.get(0).unwrap();
        result.push_str(&stripped[last_end..matched.start()]);
        let include_path = resolve_include_path(
            capture.get(1).unwrap().as_str(),
            canonical.parent().unwrap_or(root_dir),
        );
        if include_path.is_file() {
            result.push_str(&inline_tex_file(&include_path, root_dir, visited)?);
        }
        last_end = matched.end();
    }
    result.push_str(&stripped[last_end..]);
    Ok(result)
}

fn resolve_include_path(raw: &str, base_dir: &Path) -> PathBuf {
    let mut candidate = base_dir.join(raw);
    if candidate.extension().is_none() {
        candidate.set_extension("tex");
    }
    candidate
}

fn strip_comments(text: &str) -> String {
    text.lines()
        .map(|line| {
            let mut out = String::new();
            let mut prev = '\0';
            for ch in line.chars() {
                if ch == '%' && prev != '\\' {
                    break;
                }
                out.push(ch);
                prev = ch;
            }
            out
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn extract_environment(text: &str, env: &str) -> Option<String> {
    let start_marker = format!(r"\begin{{{env}}}");
    let end_marker = format!(r"\end{{{env}}}");
    let start = text.find(&start_marker)? + start_marker.len();
    let end = text[start..].find(&end_marker)? + start;
    Some(cleanup_tex(&text[start..end]))
}

fn extract_sections(text: &str) -> Vec<PaperSection> {
    let matches: Vec<_> = section_regex()
        .captures_iter(text)
        .filter_map(|cap| {
            let full = cap.get(0)?;
            let level = match cap.get(1)?.as_str() {
                "section" => 1,
                "subsection" => 2,
                _ => 3,
            };
            let title = cleanup_tex(cap.get(2)?.as_str());
            Some((full.start(), full.end(), level, title))
        })
        .collect();

    let mut sections = Vec::new();
    for (index, (_, end, level, title)) in matches.iter().enumerate() {
        let next_start = matches
            .get(index + 1)
            .map(|item| item.0)
            .unwrap_or(text.len());
        let content = cleanup_tex(&text[*end..next_start]);
        sections.push(PaperSection {
            level: *level,
            title: title.clone(),
            content,
        });
    }
    sections
}

fn extract_figure_captions(text: &str) -> Vec<String> {
    let mut captions = Vec::new();
    for capture in figure_block_regex().captures_iter(text) {
        if let Some(block) = capture.get(1) {
            captions.extend(extract_command_values(block.as_str(), "caption"));
        }
    }
    captions
}

fn extract_table_captions(text: &str) -> Vec<String> {
    let mut captions = Vec::new();
    for capture in table_block_regex().captures_iter(text) {
        if let Some(block) = capture.get(1) {
            captions.extend(extract_command_values(block.as_str(), "caption"));
        }
    }
    captions
}

fn extract_citations(text: &str) -> Vec<String> {
    let mut citations = BTreeSet::new();
    let mut cursor = 0usize;
    while let Some(relative_start) = text[cursor..].find('\\') {
        let command_start = cursor + relative_start;
        let Some((command, mut index)) = read_latex_command(text, command_start) else {
            cursor = command_start + 1;
            continue;
        };
        if !is_citation_command(command.as_str()) {
            cursor = index;
            continue;
        }
        if text[index..].starts_with('*') {
            index += 1;
        }
        index = skip_ws_and_optional_args(text, index);
        let Some((keys, end_index)) = extract_balanced_braces(text, index) else {
            cursor = index;
            continue;
        };
        for key in keys.split(',') {
            if let Some(normalized) = cleanup_citation_key(key) {
                citations.insert(normalized);
            }
        }
        cursor = end_index;
    }
    citations.into_iter().collect()
}

fn extract_citation_references(
    root_dir: &Path,
    citations: &[String],
) -> Result<Vec<CitationReference>> {
    if citations.is_empty() {
        return Ok(Vec::new());
    }
    let cited_keys: BTreeSet<_> = citations.iter().map(|key| key.as_str()).collect();
    let mut references = Vec::new();
    for entry in bibliography_entries(root_dir)? {
        if !cited_keys.contains(entry.citation_key.as_str()) {
            continue;
        }
        references.push(citation_reference_from_bib_entry(&entry));
    }
    references.sort_by(|left, right| left.key.cmp(&right.key));
    Ok(references)
}

fn bibliography_entries(root_dir: &Path) -> Result<Vec<BibEntry>> {
    let mut entries = Vec::new();
    for entry in WalkDir::new(root_dir).into_iter().flatten() {
        let path = entry.path();
        if !path.is_file() || path.extension().and_then(|ext| ext.to_str()) != Some("bib") {
            continue;
        }
        let text = fs::read_to_string(path)
            .with_context(|| format!("Failed to read bibliography {}", path.display()))?;
        entries.extend(parse_bibtex(&text).unwrap_or_default());
    }
    Ok(entries)
}

fn citation_reference_from_bib_entry(entry: &BibEntry) -> CitationReference {
    let title = entry
        .fields
        .get("title")
        .cloned()
        .filter(|value| !value.is_empty());
    let doi = entry
        .fields
        .get("doi")
        .and_then(|value| normalize_doi(value))
        .or_else(|| entry.fields.get("url").and_then(|value| extract_doi(value)));
    let arxiv_id = entry
        .fields
        .get("eprint")
        .and_then(|value| normalize_arxiv_id(value))
        .or_else(|| {
            entry
                .fields
                .get("arxiv")
                .and_then(|value| normalize_arxiv_id(value))
        })
        .or_else(|| {
            entry
                .fields
                .get("journal")
                .and_then(|value| normalize_arxiv_id(value))
        })
        .or_else(|| {
            entry
                .fields
                .get("volume")
                .and_then(|value| normalize_arxiv_id(value))
        })
        .or_else(|| {
            entry
                .fields
                .get("url")
                .and_then(|value| normalize_arxiv_id(value))
        });
    CitationReference {
        key: entry.citation_key.clone(),
        title,
        doi,
        arxiv_id,
        url: entry.fields.get("url").cloned(),
    }
}

fn read_latex_command(text: &str, start: usize) -> Option<(String, usize)> {
    let bytes = text.as_bytes();
    if bytes.get(start).copied() != Some(b'\\') {
        return None;
    }
    let mut end = start + 1;
    while let Some(byte) = bytes.get(end) {
        if !byte.is_ascii_alphabetic() {
            break;
        }
        end += 1;
    }
    if end == start + 1 {
        return None;
    }
    Some((text[start + 1..end].to_string(), end))
}

fn is_citation_command(command: &str) -> bool {
    let command = command.to_ascii_lowercase();
    command.starts_with("cite")
        || matches!(
            command.as_str(),
            "autocite"
                | "parencite"
                | "textcite"
                | "supercite"
                | "footcite"
                | "smartcite"
                | "fullcite"
                | "nocite"
        )
}

fn skip_ws_and_optional_args(text: &str, mut index: usize) -> usize {
    loop {
        index = skip_ascii_whitespace(text, index);
        if !text[index..].starts_with('[') {
            return index;
        }
        let Some((_, end_index)) = extract_balanced_delimited(text, index, b'[', b']') else {
            return index;
        };
        index = end_index;
    }
}

fn skip_ascii_whitespace(text: &str, mut index: usize) -> usize {
    while matches!(text.as_bytes().get(index), Some(byte) if byte.is_ascii_whitespace()) {
        index += 1;
    }
    index
}

fn cleanup_citation_key(raw: &str) -> Option<String> {
    let key = raw.trim().trim_matches(|ch: char| ch.is_whitespace());
    if key.is_empty() || key == "*" {
        return None;
    }
    Some(key.to_string())
}

fn extract_command_value(text: &str, command: &str) -> Option<String> {
    extract_command_values(text, command).into_iter().next()
}

fn extract_command_values(text: &str, command: &str) -> Vec<String> {
    let plain = format!(r"\{command}");
    let mut values = Vec::new();
    let mut cursor = 0usize;
    while let Some(relative_start) = text[cursor..].find(&plain) {
        let command_start = cursor + relative_start;
        let mut brace_index = command_start + plain.len();
        if text[brace_index..].starts_with('*') {
            brace_index += 1;
        }
        if !text[brace_index..].starts_with('{') {
            cursor = command_start + 1;
            continue;
        }
        if let Some((value, end_index)) = extract_balanced_braces(text, brace_index) {
            values.push(cleanup_tex(&value));
            cursor = end_index;
        } else {
            break;
        }
    }
    values
}

fn extract_balanced_braces(text: &str, opening_index: usize) -> Option<(String, usize)> {
    extract_balanced_delimited(text, opening_index, b'{', b'}')
}

fn extract_balanced_delimited(
    text: &str,
    opening_index: usize,
    opening: u8,
    closing: u8,
) -> Option<(String, usize)> {
    let bytes = text.as_bytes();
    if bytes.get(opening_index).copied() != Some(opening) {
        return None;
    }
    let mut depth = 0i32;
    let mut start = opening_index + 1;
    for (index, byte) in bytes.iter().enumerate().skip(opening_index) {
        if *byte == opening {
            depth += 1;
            if depth == 1 {
                start = index + 1;
            }
        } else if *byte == closing {
            depth -= 1;
            if depth == 0 {
                return Some((text[start..index].to_string(), index + 1));
            }
        }
    }
    None
}

fn normalize_doi(raw: &str) -> Option<String> {
    let trimmed = raw
        .trim()
        .trim_start_matches("https://doi.org/")
        .trim_start_matches("http://doi.org/")
        .trim_start_matches("doi:")
        .trim();
    if trimmed.to_ascii_lowercase().starts_with("10.") {
        Some(trimmed.trim_end_matches('.').to_ascii_lowercase())
    } else {
        None
    }
}

fn extract_doi(raw: &str) -> Option<String> {
    doi_regex()
        .captures(raw)
        .and_then(|capture| normalize_doi(capture.get(1)?.as_str()))
}

fn normalize_arxiv_id(raw: &str) -> Option<String> {
    arxiv_regex()
        .captures(raw)
        .and_then(|capture| capture.get(1))
        .map(|matched| matched.as_str().trim_end_matches(".pdf").to_string())
}

fn cleanup_tex(text: &str) -> String {
    let cleaned = latex_command_regex().replace_all(text, "");
    cleaned
        .replace('\n', " ")
        .replace('{', "")
        .replace('}', "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

fn path_has_files(path: &Path) -> bool {
    path.is_dir()
        && path
            .read_dir()
            .map(|entries| {
                entries.flatten().any(|entry| {
                    let path = entry.path();
                    path.is_file() || path_has_files(&path)
                })
            })
            .unwrap_or(false)
}

fn include_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\\(?:input|include)\{([^}]+)\}").unwrap())
}

fn section_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\\(section|subsection|subsubsection)\*?\{([^}]*)\}").unwrap())
}

fn figure_block_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\\begin\{figure\}(?s)(.*?)\\end\{figure\}").unwrap())
}

fn table_block_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\\begin\{table\}(?s)(.*?)\\end\{table\}").unwrap())
}

fn latex_command_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\\[a-zA-Z]+\*?").unwrap())
}

fn doi_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)(10\.\d{4,9}/[^\s{}]+)").unwrap())
}

fn arxiv_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)(?:arxiv[:./\s]+|abs/|pdf/)?([0-9]{4}\.[0-9]{4,5}(?:v[0-9]+)?)").unwrap()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SinkMode;
    use crate::model::{DownloadMode, SourceKind};

    fn sample_record() -> PaperSourceRecord {
        PaperSourceRecord {
            paper_id: "vista".into(),
            citation_key: Some("zhang2026vistaslam".into()),
            title: "ViSTA-SLAM".into(),
            authors: vec!["Ganlin Zhang".into()],
            year: Some("2026".into()),
            arxiv_id: Some("2509.01584".into()),
            doi: None,
            url: Some("https://arxiv.org/abs/2509.01584".into()),
            tex_dir: Some("paper".into()),
            pdf_file: Some("vista.pdf".into()),
            source_kind: SourceKind::ManifestAndBib,
            download_mode: DownloadMode::ManifestSourcePlusPdf,
            has_local_tex: true,
            has_local_pdf: false,
            parse_status: ParseStatus::Downloaded,
            semantic_scholar: None,
        }
    }

    fn sample_config(root: &Path) -> RepoConfig {
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
            relevance_tags: vec![],
            semantic_scholar: None,
        }
    }

    #[test]
    fn parses_simple_tex_tree() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("paper");
        fs::create_dir_all(root.join("sec")).unwrap();
        fs::write(
            root.join("main.tex"),
            r#"\documentclass{article}
\title{ViSTA-SLAM}
\begin{document}
\begin{abstract}
This is the abstract.
\end{abstract}
\section{Method}
Overview text.
\input{sec/details}
\begin{figure}
\caption{A frontend figure.}
\end{figure}
\cite{foo,bar}
\end{document}"#,
        )
        .unwrap();
        fs::write(
            root.join("sec/details.tex"),
            r#"\subsection{Details} Dense details here."#,
        )
        .unwrap();

        let parsed = parse_paper_dir(&sample_record(), &root).unwrap();
        assert_eq!(parsed.metadata.parse_status, ParseStatus::Parsed);
        assert_eq!(parsed.abstract_text.unwrap(), "This is the abstract.");
        assert_eq!(parsed.sections.len(), 2);
        assert_eq!(parsed.figures[0].caption, "A frontend figure.");
        assert_eq!(parsed.citations, vec!["bar".to_string(), "foo".to_string()]);
    }

    #[test]
    fn parses_natbib_biblatex_citations_and_local_bib_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("paper");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("main.tex"),
            r#"\documentclass{article}
\title{Citation Forms}
\begin{document}
\section{Related Work}
\citet{rawKey} and \citep[see][Sec.~2]{doiKey, arxivKey}.
\parencite[cf.][]{titleKey}
\textcite*{rawKey}
\end{document}"#,
        )
        .unwrap();
        fs::write(
            root.join("refs.bib"),
            r#"
@article{doiKey,
  title={A DOI Paper},
  doi={10.1109/3DV62453.2024.00044}
}
@misc{arxivKey,
  title={An arXiv Paper},
  eprint={2406.10224},
  archivePrefix={arXiv},
  url={https://arxiv.org/abs/2406.10224}
}
@misc{titleKey,
  title={Title Only Target}
}
"#,
        )
        .unwrap();

        let parsed = parse_paper_dir(&sample_record(), &root).unwrap();
        assert_eq!(
            parsed.citations,
            vec![
                "arxivKey".to_string(),
                "doiKey".to_string(),
                "rawKey".to_string(),
                "titleKey".to_string()
            ]
        );
        assert_eq!(parsed.citation_references.len(), 3);
        assert_eq!(
            parsed.citation_references[0].arxiv_id.as_deref(),
            Some("2406.10224")
        );
        assert_eq!(
            parsed.citation_references[1].doi.as_deref(),
            Some("10.1109/3dv62453.2024.00044")
        );
        assert_eq!(
            parsed.citation_references[2].title.as_deref(),
            Some("Title Only Target")
        );
    }

    #[test]
    fn parses_large_registry_surface_deterministically() {
        let dir = tempfile::tempdir().unwrap();
        let config = sample_config(dir.path());
        fs::create_dir_all(&config.tex_root).unwrap();

        let mut registry = Vec::new();
        for idx in 0..64usize {
            let tex_dir = format!("paper-{idx}");
            let root = config.tex_root.join(&tex_dir);
            fs::create_dir_all(&root).unwrap();
            fs::write(
                root.join("main.tex"),
                format!(
                    "\\documentclass{{article}}
\\title{{Synthetic {idx}}}
\\begin{{document}}
\\begin{{abstract}}Abstract {idx}.\\end{{abstract}}
\\section{{Method}}Method {idx}.
\\cite{{c{idx},shared}}
\\end{{document}}"
                ),
            )
            .unwrap();

            let mut record = sample_record();
            record.paper_id = format!("paper-{idx:03}");
            record.citation_key = Some(format!("paper{idx}"));
            record.title = format!("Synthetic {idx}");
            record.tex_dir = Some(tex_dir);
            registry.push(record);
        }

        let parsed = parse_registry_papers(&config, &registry).unwrap();
        assert_eq!(parsed.len(), registry.len());
        for (idx, paper) in parsed.iter().enumerate() {
            assert_eq!(paper.metadata.paper_id, registry[idx].paper_id);
            assert_eq!(paper.metadata.parse_status, ParseStatus::Parsed);
            assert_eq!(
                paper.citations,
                vec![format!("c{idx}"), "shared".to_string()]
            );
            let expected_abstract = format!("Abstract {idx}.");
            assert_eq!(
                paper.abstract_text.as_ref().map(String::as_str),
                Some(expected_abstract.as_str())
            );
        }
    }
}
