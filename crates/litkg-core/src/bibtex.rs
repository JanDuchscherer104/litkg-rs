use anyhow::{bail, Result};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BibEntry {
    pub entry_type: String,
    pub citation_key: String,
    pub fields: BTreeMap<String, String>,
}

pub fn parse_bibtex(text: &str) -> Result<Vec<BibEntry>> {
    let mut entries = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let mut index = 0usize;

    while index < chars.len() {
        if chars[index] != '@' {
            index += 1;
            continue;
        }
        index += 1;
        let type_start = index;
        while index < chars.len() && chars[index] != '{' && chars[index] != '(' {
            index += 1;
        }
        if index >= chars.len() {
            bail!("Malformed BibTeX entry: missing opening delimiter");
        }
        let entry_type = chars[type_start..index]
            .iter()
            .collect::<String>()
            .trim()
            .to_lowercase();
        let opening = chars[index];
        let closing = if opening == '{' { '}' } else { ')' };
        index += 1;

        let key_start = index;
        while index < chars.len() && chars[index] != ',' {
            index += 1;
        }
        if index >= chars.len() {
            bail!("Malformed BibTeX entry: missing citation key separator");
        }
        let citation_key = chars[key_start..index]
            .iter()
            .collect::<String>()
            .trim()
            .to_string();
        index += 1;

        let body_start = index;
        let mut depth = 1i32;
        while index < chars.len() && depth > 0 {
            if chars[index] == opening {
                depth += 1;
            } else if chars[index] == closing {
                depth -= 1;
            }
            index += 1;
        }
        if depth != 0 {
            bail!("Malformed BibTeX entry: unbalanced delimiters");
        }
        let body = chars[body_start..index - 1].iter().collect::<String>();
        let fields = parse_fields(&body);
        entries.push(BibEntry {
            entry_type,
            citation_key,
            fields,
        });
    }

    Ok(entries)
}

fn parse_fields(body: &str) -> BTreeMap<String, String> {
    let chars: Vec<char> = body.chars().collect();
    let mut fields = BTreeMap::new();
    let mut index = 0usize;

    while index < chars.len() {
        while index < chars.len() && (chars[index].is_whitespace() || chars[index] == ',') {
            index += 1;
        }
        if index >= chars.len() {
            break;
        }

        let name_start = index;
        while index < chars.len() && chars[index] != '=' {
            index += 1;
        }
        if index >= chars.len() {
            break;
        }
        let field_name = chars[name_start..index]
            .iter()
            .collect::<String>()
            .trim()
            .to_lowercase();
        index += 1;
        while index < chars.len() && chars[index].is_whitespace() {
            index += 1;
        }
        if index >= chars.len() {
            break;
        }

        let value = match chars[index] {
            '{' => parse_braced_value(&chars, &mut index),
            '"' => parse_quoted_value(&chars, &mut index),
            _ => parse_bare_value(&chars, &mut index),
        };
        fields.insert(field_name, cleanup_bib_value(&value));
    }

    fields
}

fn parse_braced_value(chars: &[char], index: &mut usize) -> String {
    let mut depth = 0i32;
    let start = *index;
    while *index < chars.len() {
        if chars[*index] == '{' {
            depth += 1;
        } else if chars[*index] == '}' {
            depth -= 1;
            if depth == 0 {
                *index += 1;
                break;
            }
        }
        *index += 1;
    }
    chars[start + 1..*index - 1].iter().collect::<String>()
}

fn parse_quoted_value(chars: &[char], index: &mut usize) -> String {
    let start = *index;
    *index += 1;
    while *index < chars.len() {
        if chars[*index] == '"' && chars.get(index.saturating_sub(1)) != Some(&'\\') {
            *index += 1;
            break;
        }
        *index += 1;
    }
    chars[start + 1..*index - 1].iter().collect::<String>()
}

fn parse_bare_value(chars: &[char], index: &mut usize) -> String {
    let start = *index;
    while *index < chars.len() && chars[*index] != ',' && chars[*index] != '\n' {
        *index += 1;
    }
    chars[start..*index].iter().collect::<String>()
}

fn cleanup_bib_value(value: &str) -> String {
    value
        .replace(['\n', '\t'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bib_entry() {
        let input = r#"
@misc{zhang2026vistaslam,
  title={ViSTA-SLAM: Visual SLAM with Symmetric Two-view Association},
  author={Ganlin Zhang and Shenhan Qian},
  year={2026},
  eprint={2509.01584},
  url={https://arxiv.org/abs/2509.01584},
}
"#;
        let entries = parse_bibtex(input).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].citation_key, "zhang2026vistaslam");
        assert_eq!(entries[0].fields["eprint"], "2509.01584");
    }
}
