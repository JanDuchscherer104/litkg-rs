use rust_stemmers::{Algorithm, Stemmer};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WeightedScore {
    pub source_type: String,
    pub score_lexical: f32,
    pub score_authority: f32,
    pub score_freshness: f32,
    pub score_final: f32,
    pub authority: String,
    pub why: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bm25Params {
    pub k1: f32,
    pub b: f32,
}

impl Default for Bm25Params {
    fn default() -> Self {
        Self { k1: 1.5, b: 0.75 }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Bm25Document {
    pub terms: BTreeMap<String, usize>,
    pub length: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Bm25Corpus {
    params: Bm25Params,
    document_count: usize,
    average_document_length: f32,
    document_frequencies: BTreeMap<String, usize>,
}

pub struct SearchTokenizer {
    stemmer: Stemmer,
}

impl Default for SearchTokenizer {
    fn default() -> Self {
        Self {
            stemmer: Stemmer::create(Algorithm::English),
        }
    }
}

impl SearchTokenizer {
    pub fn raw_tokens(&self, text: &str) -> Vec<String> {
        raw_search_tokens(text)
    }

    pub fn stem_token(&self, token: &str) -> String {
        self.stemmer.stem(token).to_string()
    }

    pub fn stemmed_tokens(&self, text: &str) -> Vec<String> {
        self.raw_tokens(text)
            .into_iter()
            .map(|token| self.stem_token(&token))
            .filter(|token| !token.is_empty())
            .collect()
    }

    pub fn query_terms(&self, raw_terms: &[String]) -> Vec<String> {
        let mut terms = BTreeSet::new();
        for raw_term in raw_terms {
            let normalized = normalize_search_token(raw_term);
            if normalized.is_empty() || is_search_stopword(&normalized) {
                continue;
            }
            terms.insert(self.stem_token(&normalized));
        }
        terms.into_iter().collect()
    }
}

impl Bm25Document {
    pub fn from_text(tokenizer: &SearchTokenizer, text: &str) -> Self {
        let mut terms = BTreeMap::new();
        let mut length = 0;
        for token in tokenizer.stemmed_tokens(text) {
            *terms.entry(token).or_insert(0) += 1;
            length += 1;
        }
        Self { terms, length }
    }
}

impl Bm25Corpus {
    pub fn from_documents(documents: &[Bm25Document], params: Bm25Params) -> Self {
        let document_count = documents.len();
        let total_length = documents
            .iter()
            .map(|document| document.length)
            .sum::<usize>();
        let average_document_length = if document_count == 0 {
            1.0
        } else {
            (total_length as f32 / document_count as f32).max(1.0)
        };
        let mut document_frequencies = BTreeMap::<String, usize>::new();
        for document in documents {
            for term in document.terms.keys() {
                *document_frequencies.entry(term.clone()).or_insert(0) += 1;
            }
        }
        Self {
            params,
            document_count,
            average_document_length,
            document_frequencies,
        }
    }

    pub fn score(&self, document: &Bm25Document, query_terms: &[String]) -> f32 {
        if self.document_count == 0 || document.length == 0 || query_terms.is_empty() {
            return 0.0;
        }
        let mut score = 0.0;
        for term in query_terms.iter().collect::<BTreeSet<_>>() {
            let Some(&term_frequency) = document.terms.get(term.as_str()) else {
                continue;
            };
            let document_frequency = *self.document_frequencies.get(term.as_str()).unwrap_or(&0);
            let idf = ((self.document_count as f32 - document_frequency as f32 + 0.5)
                / (document_frequency as f32 + 0.5)
                + 1.0)
                .ln();
            let tf = term_frequency as f32;
            let length = document.length as f32;
            let denominator = tf
                + self.params.k1
                    * (1.0 - self.params.b + self.params.b * length / self.average_document_length);
            if denominator > 0.0 {
                score += idf * (tf * (self.params.k1 + 1.0)) / denominator;
            }
        }
        score
    }

    pub fn vocabulary(&self) -> BTreeSet<String> {
        self.document_frequencies.keys().cloned().collect()
    }
}

pub fn raw_search_tokens(text: &str) -> Vec<String> {
    text.split(|ch: char| !ch.is_alphanumeric())
        .map(normalize_search_token)
        .filter(|token| token.len() >= 2 && !is_search_stopword(token))
        .collect()
}

pub fn normalize_search_token(token: &str) -> String {
    token
        .trim()
        .chars()
        .flat_map(char::to_lowercase)
        .filter(|ch| ch.is_alphanumeric())
        .collect()
}

pub fn best_fuzzy_replacement(
    token: &str,
    vocabulary: &BTreeSet<String>,
    max_distance: usize,
) -> Option<String> {
    let normalized = normalize_search_token(token);
    if normalized.len() < 5 || is_search_stopword(&normalized) {
        return None;
    }
    vocabulary
        .iter()
        .filter(|candidate| candidate.len() >= 5)
        .filter_map(|candidate| {
            let distance = strsim::levenshtein(&normalized, candidate);
            (distance <= max_distance).then_some((distance, candidate))
        })
        .min_by(|(left_distance, left), (right_distance, right)| {
            left_distance
                .cmp(right_distance)
                .then_with(|| left.len().cmp(&right.len()))
                .then_with(|| left.cmp(right))
        })
        .map(|(_, candidate)| candidate.clone())
}

pub fn calculate_weighted_score(
    source_path: &Path,
    lexical_score: f32,
    authority_tiers: Option<&BTreeMap<String, f32>>,
) -> WeightedScore {
    let mut authority_multiplier = 1.0;
    let mut authority_label = "default".to_string();
    let mut why = vec![];

    if let Some(tiers) = authority_tiers {
        let path_str = normalize_path(&source_path.to_string_lossy());

        let mut matched_pattern = None;
        let mut matched_multiplier = 1.0;

        for (pattern, &tier_multiplier) in tiers {
            let pattern_regex =
                glob::Pattern::new(pattern).unwrap_or_else(|_| glob::Pattern::new("*").unwrap());
            if pattern_regex.matches(&path_str) || suffix_pattern_matches(&path_str, pattern) {
                // If multiple match, we take the highest multiplier
                if tier_multiplier > matched_multiplier || matched_pattern.is_none() {
                    matched_multiplier = tier_multiplier;
                    matched_pattern = Some(pattern.clone());
                }
            }
        }

        if let Some(pattern) = matched_pattern {
            authority_multiplier = matched_multiplier;
            authority_label = if matched_multiplier >= 1.5 {
                "canonical".to_string()
            } else if matched_multiplier >= 1.2 {
                "active".to_string()
            } else if matched_multiplier < 1.0 {
                "historical".to_string()
            } else {
                "default".to_string()
            };
            why.push(format!("matches authority tier pattern '{}'", pattern));
        } else {
            why.push("no authority tier match".to_string());
        }
    } else {
        why.push("no authority tiers configured".to_string());
    }

    let mut freshness_multiplier = 1.0;
    if let Ok(metadata) = fs::metadata(source_path) {
        if let Ok(mtime) = metadata.modified() {
            if let Ok(elapsed) = mtime.elapsed() {
                let days = elapsed.as_secs_f32() / 86400.0;
                // 30 day half-life
                freshness_multiplier = 0.5_f32.powf(days / 30.0).clamp(0.1, 1.0);
                if days > 30.0 {
                    why.push("stale (>30 days old)".to_string());
                } else {
                    why.push("recently updated".to_string());
                }
            }
        }
    }

    let score_final = lexical_score * authority_multiplier * freshness_multiplier;

    WeightedScore {
        source_type: infer_source_type(source_path),
        score_lexical: lexical_score,
        score_authority: authority_multiplier,
        score_freshness: freshness_multiplier,
        score_final,
        authority: authority_label,
        why,
    }
}

pub fn is_search_stopword(term: &str) -> bool {
    matches!(
        term,
        "a" | "an"
            | "and"
            | "are"
            | "as"
            | "at"
            | "be"
            | "by"
            | "for"
            | "from"
            | "in"
            | "into"
            | "is"
            | "it"
            | "of"
            | "on"
            | "or"
            | "that"
            | "the"
            | "this"
            | "to"
            | "with"
            | "about"
            | "after"
            | "before"
            | "can"
            | "could"
            | "make"
            | "need"
            | "needs"
            | "use"
            | "using"
            | "work"
            | "write"
            | "writing"
            | "task"
            | "implement"
            | "implementation"
            | "fix"
            | "update"
            | "change"
            | "edit"
            | "add"
            | "remove"
            | "refactor"
    )
}

pub fn source_quality_adjustment(source_type: &str, source_path: &Path) -> (f32, Option<String>) {
    let path = normalize_path(&source_path.to_string_lossy()).to_ascii_lowercase();
    if source_type == "generated_context" {
        return (
            0.55,
            Some("penalized generated context below authored/current sources".into()),
        );
    }
    if matches!(source_type, "audit_log" | "transcript" | "episodic_memory") {
        return (
            0.5,
            Some("penalized audit/transcript/episodic material below curated sources".into()),
        );
    }
    if path.contains("/audit")
        || path.contains("audit_")
        || path.contains("/transcript")
        || path.contains("transcript_")
        || path.ends_with(".rrd")
    {
        return (
            0.5,
            Some("penalized audit/transcript-like material below curated sources".into()),
        );
    }
    if path.ends_with(".agents/resolved.toml") {
        return (
            0.35,
            Some("penalized resolved backlog below active backlog".into()),
        );
    }
    match source_type {
        "active_backlog" => (1.25, Some("boosted active backlog source".into())),
        "code" => (1.2, Some("boosted implementation code source".into())),
        "agent_guidance" | "agent_skill" => {
            (1.15, Some("boosted owning guidance or skill source".into()))
        }
        "docs" => (1.08, Some("boosted authored documentation source".into())),
        "canonical_memory" => (1.05, Some("boosted canonical memory source".into())),
        _ => (1.0, None),
    }
}

pub fn apply_source_quality_adjustment(
    score: &mut WeightedScore,
    source_type: &str,
    source_path: &Path,
) {
    score.source_type = source_type.to_string();
    let (multiplier, why) = source_quality_adjustment(source_type, source_path);
    score.score_final *= multiplier;
    if let Some(why) = why {
        score.why.push(why);
    }
}

fn normalize_path(path: &str) -> String {
    path.replace('\\', "/")
}

fn suffix_pattern_matches(path: &str, pattern: &str) -> bool {
    if pattern.starts_with('/') {
        return false;
    }
    let path = normalize_path(path);
    let pattern = normalize_path(pattern);
    let Ok(pattern_regex) = glob::Pattern::new(&pattern) else {
        return false;
    };
    let parts = path.split('/').collect::<Vec<_>>();
    for index in 0..parts.len() {
        if pattern_regex.matches(&parts[index..].join("/")) {
            return true;
        }
    }
    false
}

fn infer_source_type(source_path: &Path) -> String {
    let path = normalize_path(&source_path.to_string_lossy());
    if path.contains("/.agents/memory/state/") || path.starts_with(".agents/memory/state/") {
        "canonical_memory".into()
    } else if path.ends_with(".agents/issues.toml")
        || path.ends_with(".agents/todos.toml")
        || path.ends_with(".agents/refactors.toml")
        || path.ends_with(".agents/resolved.toml")
    {
        "active_backlog".into()
    } else if path.contains("/.agents/memory/history/")
        || path.starts_with(".agents/memory/history/")
        || path.contains("/.agents/work/")
        || path.starts_with(".agents/work/")
        || path.contains("/.agents/archive/")
        || path.starts_with(".agents/archive/")
    {
        "episodic_memory".into()
    } else if path.contains("/docs/_generated/context/")
        || path.starts_with("docs/_generated/context/")
    {
        "generated_context".into()
    } else if path.contains("/transcript")
        || path.contains("transcript_")
        || path.contains("/session")
        || path.contains("session_")
    {
        "transcript".into()
    } else if path.contains("/audit") || path.contains("audit_") {
        "audit_log".into()
    } else if path.contains("/.agents/skills/") || path.starts_with(".agents/skills/") {
        "agent_skill".into()
    } else if path.ends_with("AGENTS.md") {
        "agent_guidance".into()
    } else if path.contains("/aria_nbv/")
        || path.starts_with("aria_nbv/")
        || path.ends_with(".rs")
        || path.ends_with(".py")
        || path.ends_with(".ts")
        || path.ends_with(".tsx")
        || path.ends_with(".js")
        || path.ends_with(".jsx")
        || path.contains("/src/")
        || path.starts_with("src/")
        || path.contains("/crates/")
        || path.starts_with("crates/")
    {
        "code".into()
    } else if path.contains("/docs/") || path.starts_with("docs/") {
        "docs".into()
    } else {
        "default".into()
    }
}
