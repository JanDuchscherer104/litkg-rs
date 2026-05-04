use anyhow::Result;
use litkg_neo4j::{load_export_bundle, Neo4jExportBundle, Neo4jNode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GraphModality {
    All,
    Code,
    Docs,
    GeneratedContext,
    Literature,
    Memory,
    Backlog,
    ExternalDocs,
}

impl GraphModality {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Code => "code",
            Self::Docs => "docs",
            Self::GeneratedContext => "generated-context",
            Self::Literature => "literature",
            Self::Memory => "memory",
            Self::Backlog => "backlog",
            Self::ExternalDocs => "external-docs",
        }
    }

    pub fn selectable() -> &'static [GraphModality] {
        &[
            Self::Code,
            Self::Docs,
            Self::GeneratedContext,
            Self::Literature,
            Self::Memory,
            Self::Backlog,
            Self::ExternalDocs,
        ]
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct GraphFilter {
    #[serde(default)]
    pub include: BTreeSet<GraphModality>,
    #[serde(default)]
    pub exclude: BTreeSet<GraphModality>,
}

impl GraphFilter {
    pub fn all() -> Self {
        Self::default()
    }

    pub fn explicit_all() -> Self {
        Self {
            include: GraphModality::selectable().iter().copied().collect(),
            exclude: BTreeSet::new(),
        }
    }

    pub fn only(modalities: impl IntoIterator<Item = GraphModality>) -> Self {
        Self {
            include: modalities
                .into_iter()
                .filter(|modality| *modality != GraphModality::All)
                .collect(),
            exclude: BTreeSet::new(),
        }
    }

    pub fn explicit_for_ui(&self) -> Self {
        let mut explicit = if self.include.is_empty() || self.include.contains(&GraphModality::All)
        {
            Self::explicit_all()
        } else {
            Self {
                include: self.include.clone(),
                exclude: BTreeSet::new(),
            }
        };
        if self.exclude.contains(&GraphModality::All) {
            explicit.include.clear();
        } else {
            for modality in &self.exclude {
                explicit.include.remove(modality);
            }
        }
        explicit
    }

    pub fn set_enabled(&mut self, modality: GraphModality, enabled: bool) {
        if modality == GraphModality::All {
            *self = if enabled {
                Self::explicit_all()
            } else {
                Self {
                    include: BTreeSet::new(),
                    exclude: BTreeSet::from([GraphModality::All]),
                }
            };
            return;
        }
        if enabled {
            self.exclude.remove(&GraphModality::All);
            self.include.insert(modality);
        } else {
            self.include.remove(&modality);
            if self.include.is_empty() {
                self.exclude.insert(GraphModality::All);
            }
        }
        self.exclude.remove(&modality);
    }

    pub fn is_enabled(&self, modality: GraphModality) -> bool {
        if self.exclude.contains(&GraphModality::All) {
            return false;
        }
        if self.exclude.contains(&modality) {
            return false;
        }
        self.include.is_empty()
            || self.include.contains(&GraphModality::All)
            || self.include.contains(&modality)
    }

    pub fn matches(&self, modality: GraphModality) -> bool {
        modality == GraphModality::All || self.is_enabled(modality)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GraphEntryQuery {
    pub query: String,
    #[serde(default)]
    pub filter: GraphFilter,
    #[serde(default)]
    pub repo_root: Option<PathBuf>,
    #[serde(default)]
    pub use_rg: bool,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct GraphSearchHit {
    pub node_id: String,
    pub title: String,
    pub kind: String,
    pub modality: GraphModality,
    pub score: i64,
    pub matched_field: String,
    pub repo_path: Option<String>,
    pub line_start: Option<usize>,
    pub line_end: Option<usize>,
    pub snippet: Option<String>,
    #[serde(flatten)]
    pub rank: Option<litkg_core::WeightedScore>,
}

#[derive(Clone, Debug)]
pub struct GraphNodeRecord {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub subtitle: String,
    pub description: String,
    pub labels: Vec<String>,
    pub properties: BTreeMap<String, String>,
    pub modality: GraphModality,
    pub search_text: String,
    pub repo_path: Option<String>,
    pub line_start: Option<usize>,
    pub line_end: Option<usize>,
}

impl GraphNodeRecord {
    pub fn from_raw(raw: &Neo4jNode) -> Self {
        let properties = properties_map(&raw.properties);
        let kind = raw
            .labels
            .first()
            .cloned()
            .unwrap_or_else(|| "Node".to_string());
        let title = primary_title(raw.id.as_str(), &kind, &properties);
        let subtitle = subtitle_for_node(&kind, &properties);
        let description = description_for_node(&properties);
        let modality = classify_modality(&raw.labels, &properties);
        let repo_path = path_property(&properties);
        let line_start = parse_usize_property(&properties, "line_start");
        let line_end = parse_usize_property(&properties, "line_end");
        let search_text = build_search_text(
            raw.id.as_str(),
            &kind,
            &raw.labels,
            &properties,
            &description,
        );

        Self {
            id: raw.id.clone(),
            kind,
            title,
            subtitle,
            description,
            labels: raw.labels.clone(),
            properties,
            modality,
            search_text,
            repo_path,
            line_start,
            line_end,
        }
    }
}

pub fn build_node_records(bundle: &Neo4jExportBundle) -> Vec<GraphNodeRecord> {
    let mut records = bundle
        .nodes
        .iter()
        .map(GraphNodeRecord::from_raw)
        .collect::<Vec<_>>();
    records.sort_by(|left, right| left.id.cmp(&right.id));
    records
}

pub fn load_and_search_bundle(
    bundle_root: impl AsRef<Path>,
    query: GraphEntryQuery,
) -> Result<Vec<GraphSearchHit>> {
    let bundle = load_export_bundle(bundle_root)?;
    Ok(search_export_bundle(&bundle, query))
}

pub fn search_export_bundle(
    bundle: &Neo4jExportBundle,
    query: GraphEntryQuery,
) -> Vec<GraphSearchHit> {
    let records = build_node_records(bundle);
    search_records(&records, query)
}

pub fn search_records(records: &[GraphNodeRecord], query: GraphEntryQuery) -> Vec<GraphSearchHit> {
    let limit = query.limit.max(1);
    let mut hits = metadata_search(records, &query);
    if query.use_rg {
        if let Some(repo_root) = query.repo_root.as_ref() {
            hits.extend(rg_search(records, &query, repo_root));
        }
    }
    dedup_and_rank(hits, limit)
}

pub fn classify_modality(
    labels: &[String],
    properties: &BTreeMap<String, String>,
) -> GraphModality {
    let has = |label: &str| labels.iter().any(|item| item == label);
    if has("CodeFile") || has("CodeModule") || has("CodeSymbol") || has("CodeReference") {
        return GraphModality::Code;
    }
    if has("RepoSurface")
        && properties
            .get("surface_kind")
            .is_some_and(|kind| kind == "code_surface")
    {
        return GraphModality::Code;
    }
    if has("GeneratedContext") || has("DataContract") || has("Concept") {
        return GraphModality::GeneratedContext;
    }
    if has("ProjectMemory") {
        return GraphModality::Memory;
    }
    if has("AgentBacklogIssue") || has("AgentBacklogTodo") {
        return GraphModality::Backlog;
    }
    if has("Paper")
        || has("PaperSection")
        || has("Citation")
        || has("Author")
        || has("FieldOfStudy")
        || has("ExternalIdentifier")
        || has("PaperSurface")
    {
        return GraphModality::Literature;
    }
    if has("ExternalDoc")
        || has("ExternalDocLeaf")
        || has("Context7Leaf")
        || has("McpResource")
        || has("McpTool")
    {
        return GraphModality::ExternalDocs;
    }
    if has("DocSurface")
        || path_property(properties).is_some_and(|path| {
            path.ends_with(".qmd") || path.ends_with(".md") || path.ends_with(".typ")
        })
    {
        return GraphModality::Docs;
    }
    GraphModality::Docs
}

pub fn description_for_node(properties: &BTreeMap<String, String>) -> String {
    for key in [
        "doc_summary",
        "summary",
        "definition_short",
        "definition_long",
        "text",
        "abstract",
        "content",
    ] {
        if let Some(value) = properties.get(key).filter(|value| !value.trim().is_empty()) {
            return truncate(value.trim(), 700);
        }
    }
    String::new()
}

pub fn primary_title(id: &str, kind: &str, properties: &BTreeMap<String, String>) -> String {
    match kind {
        "Paper" | "PaperSection" => properties
            .get("title")
            .cloned()
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| id.to_string()),
        "Citation" => properties
            .get("citation_key")
            .cloned()
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| id.to_string()),
        "CodeSymbol" => properties
            .get("qualified_name")
            .or_else(|| properties.get("name"))
            .cloned()
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| id.to_string()),
        "CodeFile" | "CodeModule" => properties
            .get("repo_path")
            .or_else(|| properties.get("name"))
            .cloned()
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| id.to_string()),
        "DataContract" | "Concept" | "GeneratedContext" => properties
            .get("title")
            .or_else(|| properties.get("label"))
            .or_else(|| properties.get("name"))
            .cloned()
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| id.to_string()),
        _ => properties
            .get("title")
            .or_else(|| properties.get("name"))
            .cloned()
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| id.to_string()),
    }
}

pub fn subtitle_for_node(kind: &str, properties: &BTreeMap<String, String>) -> String {
    match kind {
        "Paper" => {
            let mut parts = Vec::new();
            if let Some(year) = properties.get("year").filter(|value| !value.is_empty()) {
                parts.push(year.clone());
            }
            if let Some(arxiv) = properties.get("arxiv_id").filter(|value| !value.is_empty()) {
                parts.push(format!("arXiv:{arxiv}"));
            }
            parts.join(" · ")
        }
        "PaperSection" => {
            let mut parts = Vec::new();
            if let Some(level) = properties.get("level").filter(|value| !value.is_empty()) {
                parts.push(format!("Level {level}"));
            }
            if let Some(paper_id) = properties.get("paper_id").filter(|value| !value.is_empty()) {
                parts.push(paper_id.clone());
            }
            parts.join(" · ")
        }
        "Citation" => properties
            .get("citation_key")
            .cloned()
            .unwrap_or_else(String::new),
        "CodeSymbol" => {
            let mut parts = Vec::new();
            if let Some(kind) = properties
                .get("symbol_kind")
                .filter(|value| !value.is_empty())
            {
                parts.push(kind.clone());
            }
            if let Some(path) = properties
                .get("repo_path")
                .filter(|value| !value.is_empty())
            {
                parts.push(path.clone());
            }
            parts.join(" · ")
        }
        "GeneratedContext" | "DataContract" | "Concept" => properties
            .get("source_path")
            .cloned()
            .unwrap_or_else(String::new),
        _ => String::new(),
    }
}

pub fn path_property(properties: &BTreeMap<String, String>) -> Option<String> {
    ["repo_path", "source_path", "file", "path"]
        .iter()
        .find_map(|key| {
            properties
                .get(*key)
                .map(|value| value.trim())
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
        })
}

pub fn truncate(value: &str, max_len: usize) -> String {
    if value.chars().count() <= max_len {
        return value.to_string();
    }
    let clipped: String = value.chars().take(max_len.saturating_sub(1)).collect();
    format!("{clipped}…")
}

fn metadata_search(records: &[GraphNodeRecord], query: &GraphEntryQuery) -> Vec<GraphSearchHit> {
    let raw_query = query.query.trim();
    let query_lower = raw_query.to_lowercase();
    let terms = query_terms(raw_query);
    let mut hits = Vec::new();

    for record in records
        .iter()
        .filter(|record| query.filter.matches(record.modality))
    {
        let (score, matched_field) = if query_lower.is_empty() {
            (1, "default".to_string())
        } else {
            score_record(record, query_lower.as_str(), &terms)
        };
        if score > 0 {
            hits.push(GraphSearchHit {
                node_id: record.id.clone(),
                title: record.title.clone(),
                kind: record.kind.clone(),
                modality: record.modality,
                score,
                matched_field,
                repo_path: record.repo_path.clone(),
                line_start: record.line_start,
                line_end: record.line_end,
                snippet: (!record.description.is_empty()).then(|| record.description.clone()),
                rank: None,
            });
        }
    }
    hits
}

fn score_record(record: &GraphNodeRecord, query: &str, terms: &[String]) -> (i64, String) {
    let mut score = 0;
    let mut matched = "search_text";
    let title = record.title.to_lowercase();
    let id = record.id.to_lowercase();
    let path = record.repo_path.clone().unwrap_or_default().to_lowercase();
    let description = record.description.to_lowercase();
    if title == query {
        score += 1000;
        matched = "title";
    } else if title.contains(query) {
        score += 240;
        matched = "title";
    }
    if id.contains(query) {
        score += 180;
        matched = "id";
    }
    if path.contains(query) {
        score += 140;
        matched = "path";
    }
    if description.contains(query) {
        score += 120;
        matched = "description";
    }
    if record.search_text.contains(query) {
        score += 80;
    }
    for term in terms {
        if title.contains(term) {
            score += 60;
        }
        if id.contains(term) {
            score += 35;
        }
        if path.contains(term) {
            score += 30;
        }
        if description.contains(term) {
            score += 25;
        }
        if record.search_text.contains(term) {
            score += 10;
        }
    }
    (score, matched.to_string())
}

fn rg_search(
    records: &[GraphNodeRecord],
    query: &GraphEntryQuery,
    repo_root: &Path,
) -> Vec<GraphSearchHit> {
    let raw_query = query.query.trim();
    if raw_query.is_empty() {
        return Vec::new();
    }
    let output = Command::new("rg")
        .current_dir(repo_root)
        .args([
            "--json",
            "--line-number",
            "--column",
            "--hidden",
            "--max-count",
            "5",
            "--max-filesize",
            "2M",
            "--glob",
            "!.git/**",
            "--glob",
            "!target/**",
            "--glob",
            "!.venv/**",
            "--glob",
            "!.data/**",
            raw_query,
            ".",
        ])
        .output();

    let Ok(output) = output else {
        return Vec::new();
    };
    if !(output.status.success() || output.status.code() == Some(1)) {
        return Vec::new();
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut hits = Vec::new();
    for line in stdout.lines() {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if value.get("type").and_then(Value::as_str) != Some("match") {
            continue;
        }
        let Some(data) = value.get("data") else {
            continue;
        };
        let Some(path) = data
            .get("path")
            .and_then(|path| path.get("text"))
            .and_then(Value::as_str)
        else {
            continue;
        };
        let line_number = data
            .get("line_number")
            .and_then(Value::as_u64)
            .map(|value| value as usize);
        let snippet = data
            .get("lines")
            .and_then(|lines| lines.get("text"))
            .and_then(Value::as_str)
            .map(|value| truncate(value.trim(), 240));

        let normalized_path = normalize_repo_path(path);
        if let Some(record) =
            best_record_for_rg_hit(records, &query.filter, &normalized_path, line_number)
        {
            let query_lower = raw_query.to_lowercase();
            let title_or_id_match = record.title.to_lowercase().contains(&query_lower)
                || record.id.to_lowercase().contains(&query_lower);
            hits.push(GraphSearchHit {
                node_id: record.id.clone(),
                title: record.title.clone(),
                kind: record.kind.clone(),
                modality: record.modality,
                score: if title_or_id_match { 1050 } else { 700 },
                matched_field: "rg".into(),
                repo_path: record
                    .repo_path
                    .clone()
                    .or_else(|| Some(normalized_path.clone())),
                line_start: line_number,
                line_end: line_number,
                snippet: snippet.clone(),
                rank: None,
            });
        }
    }
    hits
}

fn best_record_for_rg_hit<'a>(
    records: &'a [GraphNodeRecord],
    filter: &GraphFilter,
    path: &str,
    line: Option<usize>,
) -> Option<&'a GraphNodeRecord> {
    let mut candidates = records
        .iter()
        .filter(|record| filter.matches(record.modality))
        .filter(|record| {
            record
                .repo_path
                .as_deref()
                .is_some_and(|repo_path| normalize_repo_path(repo_path) == path)
        })
        .collect::<Vec<_>>();

    if let Some(line) = line {
        candidates.sort_by_key(|record| {
            let contains_line = record
                .line_start
                .zip(record.line_end)
                .is_some_and(|(start, end)| start <= line && line <= end);
            let span = record
                .line_start
                .zip(record.line_end)
                .map(|(start, end)| end.saturating_sub(start))
                .unwrap_or(usize::MAX);
            (!contains_line, span, record.title.clone())
        });
    } else {
        candidates.sort_by_key(|record| (record.line_start.is_none(), record.title.clone()));
    }

    candidates.into_iter().next()
}

fn dedup_and_rank(hits: Vec<GraphSearchHit>, limit: usize) -> Vec<GraphSearchHit> {
    let mut best = BTreeMap::<String, GraphSearchHit>::new();
    for hit in hits {
        best.entry(hit.node_id.clone())
            .and_modify(|existing| {
                if hit.score > existing.score {
                    *existing = hit.clone();
                }
            })
            .or_insert(hit);
    }
    let mut hits = best.into_values().collect::<Vec<_>>();
    hits.sort_by_key(|hit| (Reverse(hit.score), hit.title.clone(), hit.node_id.clone()));
    hits.truncate(limit);
    hits
}

fn query_terms(query: &str) -> Vec<String> {
    query
        .split(|ch: char| ch.is_whitespace() || ch == '|')
        .map(str::trim)
        .filter(|term| !term.is_empty())
        .map(str::to_lowercase)
        .collect()
}

fn build_search_text(
    id: &str,
    kind: &str,
    labels: &[String],
    properties: &BTreeMap<String, String>,
    description: &str,
) -> String {
    let mut parts = vec![
        id.to_lowercase(),
        kind.to_lowercase(),
        labels.join(" ").to_lowercase(),
        description.to_lowercase(),
    ];
    for key in [
        "title",
        "name",
        "label",
        "paper_id",
        "citation_key",
        "qualified_name",
        "signature",
        "doc_summary",
        "summary",
        "definition_short",
        "definition_long",
        "source_path",
        "repo_path",
        "file",
        "module",
        "text",
        "content",
        "aliases",
        "kg_tags",
    ] {
        if let Some(value) = properties.get(key) {
            let snippet = if key == "content" || key == "text" {
                truncate(value.as_str(), 1200)
            } else {
                value.clone()
            };
            parts.push(snippet.to_lowercase());
        }
    }
    parts.join(" ")
}

fn properties_map(value: &Value) -> BTreeMap<String, String> {
    match value {
        Value::Object(map) => map
            .iter()
            .map(|(key, value)| (key.clone(), json_value_string(value)))
            .collect(),
        _ => {
            let mut properties = BTreeMap::new();
            properties.insert("value".to_string(), json_value_string(value));
            properties
        }
    }
}

fn json_value_string(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(value) => value.clone(),
        _ => value.to_string(),
    }
}

fn parse_usize_property(properties: &BTreeMap<String, String>, key: &str) -> Option<usize> {
    properties.get(key).and_then(|value| value.parse().ok())
}

fn normalize_repo_path(path: &str) -> String {
    path.trim_start_matches("./").replace('\\', "/")
}

fn default_limit() -> usize {
    24
}

#[cfg(test)]
mod tests {
    use super::*;
    use litkg_neo4j::Neo4jNode;

    fn node(id: &str, labels: &[&str], properties: Value) -> Neo4jNode {
        Neo4jNode {
            id: id.into(),
            labels: labels.iter().map(|label| (*label).into()).collect(),
            properties,
        }
    }

    #[test]
    fn classifies_common_modalities() {
        let cases = [
            (
                node("code", &["CodeSymbol"], serde_json::json!({})),
                GraphModality::Code,
            ),
            (
                node("paper", &["Paper"], serde_json::json!({})),
                GraphModality::Literature,
            ),
            (
                node("memory", &["ProjectMemory"], serde_json::json!({})),
                GraphModality::Memory,
            ),
            (
                node("doc", &["RepoSurface", "DocSurface"], serde_json::json!({})),
                GraphModality::Docs,
            ),
            (
                node("ctx", &["GeneratedContext"], serde_json::json!({})),
                GraphModality::GeneratedContext,
            ),
        ];
        for (node, expected) in cases {
            let record = GraphNodeRecord::from_raw(&node);
            assert_eq!(record.modality, expected);
        }
    }

    #[test]
    fn filters_records_by_modality() {
        let records = vec![
            GraphNodeRecord::from_raw(&node(
                "code",
                &["CodeSymbol"],
                serde_json::json!({"qualified_name": "pkg.VinPrediction"}),
            )),
            GraphNodeRecord::from_raw(&node(
                "paper",
                &["Paper"],
                serde_json::json!({"title": "VIN paper"}),
            )),
        ];
        let hits = search_records(
            &records,
            GraphEntryQuery {
                query: "VIN".into(),
                filter: GraphFilter::only([GraphModality::Code]),
                repo_root: None,
                use_rg: false,
                limit: 10,
            },
        );
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].node_id, "code");
    }

    #[test]
    fn searches_docstrings_and_descriptions() {
        let records = vec![GraphNodeRecord::from_raw(&node(
            "symbol",
            &["CodeSymbol"],
            serde_json::json!({
                "qualified_name": "pkg.Model",
                "doc_summary": "Predicts oracle relative reconstruction improvement."
            }),
        ))];
        let hits = search_records(
            &records,
            GraphEntryQuery {
                query: "relative reconstruction".into(),
                filter: GraphFilter::all(),
                repo_root: None,
                use_rg: false,
                limit: 10,
            },
        );
        assert_eq!(hits[0].node_id, "symbol");
        assert_eq!(hits[0].matched_field, "description");
    }

    #[test]
    fn maps_rg_hits_to_nearest_line_node() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("pkg")).unwrap();
        std::fs::write(
            dir.path().join("pkg/model.py"),
            "class VinPrediction:\n    pass\n",
        )
        .unwrap();
        let records = vec![
            GraphNodeRecord::from_raw(&node(
                "file",
                &["CodeFile"],
                serde_json::json!({"repo_path": "pkg/model.py"}),
            )),
            GraphNodeRecord::from_raw(&node(
                "symbol",
                &["CodeSymbol"],
                serde_json::json!({
                    "qualified_name": "pkg.model.VinPrediction",
                    "repo_path": "pkg/model.py",
                    "line_start": 1,
                    "line_end": 2
                }),
            )),
        ];
        let hits = search_records(
            &records,
            GraphEntryQuery {
                query: "VinPrediction".into(),
                filter: GraphFilter::only([GraphModality::Code]),
                repo_root: Some(dir.path().to_path_buf()),
                use_rg: true,
                limit: 10,
            },
        );
        assert!(hits.iter().any(|hit| {
            hit.node_id == "symbol" && hit.matched_field == "rg" && hit.line_start == Some(1)
        }));
    }
}
