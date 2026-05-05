use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
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
    } else if path.contains("/.agents/skills/") || path.starts_with(".agents/skills/") {
        "agent_skill".into()
    } else if path.ends_with("AGENTS.md") {
        "agent_guidance".into()
    } else if path.contains("/aria_nbv/") || path.starts_with("aria_nbv/") {
        "code".into()
    } else if path.contains("/docs/") || path.starts_with("docs/") {
        "docs".into()
    } else {
        "default".into()
    }
}
