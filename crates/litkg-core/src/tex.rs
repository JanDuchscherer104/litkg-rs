use crate::config::RepoConfig;
use crate::model::{
    PaperFigure, PaperSection, PaperSourceRecord, PaperTable, ParseStatus, ParsedPaper,
};
use anyhow::{Context, Result};
use regex::Regex;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub fn parse_registry_papers(
    config: &RepoConfig,
    registry: &[PaperSourceRecord],
) -> Result<Vec<ParsedPaper>> {
    let mut parsed = Vec::new();
    for record in registry {
        if let Some(tex_dir) = &record.tex_dir {
            let root_dir = config.tex_root.join(tex_dir);
            if path_has_files(&root_dir) {
                parsed.push(parse_paper_dir(record, &root_dir)?);
                continue;
            }
        }
        parsed.push(metadata_only_paper(record));
    }
    Ok(parsed)
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
    let include_re = Regex::new(r"\\(?:input|include)\{([^}]+)\}").unwrap();
    let mut result = String::new();
    let mut last_end = 0usize;
    for capture in include_re.captures_iter(&stripped) {
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
    let pattern = format!(r"\\begin\{{{env}\}}(?s)(.*?)\\end\{{{env}\}}");
    let re = Regex::new(&pattern).unwrap();
    re.captures(text)
        .and_then(|cap| cap.get(1))
        .map(|value| cleanup_tex(value.as_str()))
}

fn extract_sections(text: &str) -> Vec<PaperSection> {
    let re = Regex::new(r"\\(section|subsection|subsubsection)\*?\{([^}]*)\}").unwrap();
    let matches: Vec<_> = re
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
    let figure_re = Regex::new(r"\\begin\{figure\}(?s)(.*?)\\end\{figure\}").unwrap();
    let mut captions = Vec::new();
    for capture in figure_re.captures_iter(text) {
        if let Some(block) = capture.get(1) {
            captions.extend(extract_command_values(block.as_str(), "caption"));
        }
    }
    captions
}

fn extract_table_captions(text: &str) -> Vec<String> {
    let table_re = Regex::new(r"\\begin\{table\}(?s)(.*?)\\end\{table\}").unwrap();
    let mut captions = Vec::new();
    for capture in table_re.captures_iter(text) {
        if let Some(block) = capture.get(1) {
            captions.extend(extract_command_values(block.as_str(), "caption"));
        }
    }
    captions
}

fn extract_citations(text: &str) -> Vec<String> {
    let re = Regex::new(r"\\cite[a-zA-Z*]*\{([^}]*)\}").unwrap();
    let mut citations = BTreeSet::new();
    for capture in re.captures_iter(text) {
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
    let pattern = format!(r"\\{command}\*?\{{");
    let re = Regex::new(&pattern).unwrap();
    let mut values = Vec::new();
    for matched in re.find_iter(text) {
        let brace_index = matched.end() - 1;
        if let Some((value, _)) = extract_balanced_braces(text, brace_index) {
            values.push(cleanup_tex(&value));
        }
    }
    values
}

fn extract_balanced_braces(text: &str, opening_index: usize) -> Option<(String, usize)> {
    let chars: Vec<char> = text.chars().collect();
    let mut depth = 0i32;
    let mut start = None;
    for index in opening_index..chars.len() {
        match chars[index] {
            '{' => {
                depth += 1;
                if depth == 1 {
                    start = Some(index + 1);
                }
            }
            '}' => {
                depth -= 1;
                if depth == 0 {
                    let content = chars[start?..index].iter().collect::<String>();
                    return Some((content, index + 1));
                }
            }
            _ => {}
        }
    }
    None
}

fn cleanup_tex(text: &str) -> String {
    let command_re = Regex::new(r"\\[a-zA-Z]+\*?").unwrap();
    let cleaned = command_re.replace_all(text, "");
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

#[cfg(test)]
mod tests {
    use super::*;
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
}
