use crate::config::RepoConfig;
use crate::model::{
    PaperFigure, PaperSection, PaperSourceRecord, PaperTable, ParseStatus, ParsedPaper,
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
    let title = extract_command_value(&merged, "title").unwrap_or_else(|| record.title.clone());

    let mut metadata = record.clone();
    metadata.title = title;
    metadata.parse_status = ParseStatus::Parsed;

    Ok(ParsedPaper {
        metadata,
        abstract_text,
        sections,
        figures,
        tables,
        citations,
        provenance: vec![root_file.display().to_string()],
    })
}

fn metadata_only_paper(record: &PaperSourceRecord) -> ParsedPaper {
    ParsedPaper {
        metadata: record.clone(),
        abstract_text: None,
        sections: Vec::new(),
        figures: Vec::new(),
        tables: Vec::new(),
        citations: Vec::new(),
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
    for capture in citation_regex().captures_iter(text) {
        if let Some(keys) = capture.get(1) {
            for key in keys.as_str().split(',') {
                let trimmed = key.trim();
                if !trimmed.is_empty() {
                    citations.insert(trimmed.to_string());
                }
            }
        }
    }
    citations.into_iter().collect()
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
    let bytes = text.as_bytes();
    if bytes.get(opening_index).copied() != Some(b'{') {
        return None;
    }
    let mut depth = 0i32;
    let mut start = opening_index + 1;
    for (index, byte) in bytes.iter().enumerate().skip(opening_index) {
        match byte {
            b'{' => {
                depth += 1;
                if depth == 1 {
                    start = index + 1;
                }
            }
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some((text[start..index].to_string(), index + 1));
                }
            }
            _ => {}
        }
    }
    None
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

fn citation_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\\cite[a-zA-Z*]*\{([^}]*)\}").unwrap())
}

fn latex_command_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\\[a-zA-Z]+\*?").unwrap())
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
            sink: SinkMode::Graphify,
            graphify_rebuild_command: None,
            download_pdfs: true,
            relevance_tags: vec![],
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
