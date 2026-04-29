use crate::config::SemanticScholarConfig;
use crate::model::SemanticScholarPaper;
use crate::{PaperSourceRecord, RepoConfig};
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::env;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SemanticScholarMethod {
    Batch,
    Recommendations,
    Search,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct SemanticScholarBatchResponse {
    #[serde(default)]
    pub papers: Vec<Option<SemanticScholarPaper>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct SemanticScholarRecommendationResponse {
    #[serde(default)]
    pub recommended_papers: Vec<SemanticScholarPaper>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct SemanticScholarSearchRequest {
    pub query: String,
    #[serde(default)]
    pub fields: Vec<String>,
    pub limit: Option<usize>,
    pub year: Option<String>,
    pub publication_date_or_year: Option<String>,
    #[serde(default)]
    pub fields_of_study: Vec<String>,
    #[serde(default)]
    pub venue: Vec<String>,
    pub sort: Option<String>,
    pub min_citation_count: Option<u64>,
    pub open_access_pdf: Option<bool>,
}

impl SemanticScholarSearchRequest {
    pub fn new(query: String, limit: usize, fields: Vec<String>) -> Self {
        Self {
            query,
            fields,
            limit: Some(limit),
            year: None,
            publication_date_or_year: None,
            fields_of_study: Vec::new(),
            venue: Vec::new(),
            sort: None,
            min_citation_count: None,
            open_access_pdf: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct SemanticScholarSearchResponse {
    pub total: Option<u64>,
    pub offset: Option<u64>,
    #[serde(default)]
    pub data: Vec<SemanticScholarPaper>,
}

pub trait SemanticScholarTransport {
    fn get_json(&self, path: &str, api_key: Option<&str>) -> Result<Value>;
    fn post_json(&self, path: &str, body: &Value, api_key: Option<&str>) -> Result<Value>;
}

#[derive(Debug, Clone)]
pub struct UreqSemanticScholarTransport {
    pub base_url: String,
}

impl Default for UreqSemanticScholarTransport {
    fn default() -> Self {
        Self {
            base_url: "https://api.semanticscholar.org/graph/v1".to_string(),
        }
    }
}

impl SemanticScholarTransport for UreqSemanticScholarTransport {
    fn get_json(&self, _path: &str, _api_key: Option<&str>) -> Result<Value> {
        bail!("Semantic Scholar HTTP transport is not wired yet")
    }

    fn post_json(&self, _path: &str, _body: &Value, _api_key: Option<&str>) -> Result<Value> {
        bail!("Semantic Scholar HTTP transport is not wired yet")
    }
}

#[derive(Debug, Clone)]
pub struct SemanticScholarClient<T = UreqSemanticScholarTransport> {
    config: SemanticScholarConfig,
    transport: T,
    api_key: Option<String>,
}

impl SemanticScholarClient<UreqSemanticScholarTransport> {
    pub fn from_config(config: SemanticScholarConfig) -> Result<Self> {
        let api_key = env::var(&config.api_key_env)
            .ok()
            .filter(|value| !value.is_empty());
        Ok(Self {
            config,
            transport: UreqSemanticScholarTransport::default(),
            api_key,
        })
    }
}

impl<T> SemanticScholarClient<T>
where
    T: SemanticScholarTransport,
{
    pub fn new(config: SemanticScholarConfig, transport: T, api_key: Option<String>) -> Self {
        Self {
            config,
            transport,
            api_key,
        }
    }

    pub fn batch_papers(&self, identifiers: &[String]) -> Result<SemanticScholarBatchResponse> {
        if identifiers.is_empty() {
            return Ok(SemanticScholarBatchResponse::default());
        }
        let body = serde_json::json!({ "ids": identifiers });
        let fields = self.config.fields.join(",");
        let value = self.transport.post_json(
            &format!("/paper/batch?fields={fields}"),
            &body,
            self.api_key.as_deref(),
        )?;
        let papers: Vec<Option<SemanticScholarPaper>> = serde_json::from_value(value)?;
        Ok(SemanticScholarBatchResponse { papers })
    }

    pub fn search_papers(
        &mut self,
        request: &SemanticScholarSearchRequest,
    ) -> Result<Vec<SemanticScholarPaper>> {
        if request.query.trim().is_empty() {
            bail!("Semantic Scholar search query must not be empty");
        }
        let fields = request.fields.join(",");
        let mut query = vec![
            ("query", request.query.as_str()),
            ("fields", fields.as_str()),
        ];
        let limit = request.limit.map(|value| value.to_string());
        if let Some(limit) = limit.as_deref() {
            query.push(("limit", limit));
        }
        if let Some(year) = request.year.as_deref() {
            query.push(("year", year));
        }
        if let Some(publication_date_or_year) = request.publication_date_or_year.as_deref() {
            query.push(("publicationDateOrYear", publication_date_or_year));
        }
        let fields_of_study = request.fields_of_study.join(",");
        if !fields_of_study.is_empty() {
            query.push(("fieldsOfStudy", fields_of_study.as_str()));
        }
        let venue = request.venue.join(",");
        if !venue.is_empty() {
            query.push(("venue", venue.as_str()));
        }
        if let Some(sort) = request.sort.as_deref() {
            query.push(("sort", sort));
        }
        let min_citation_count = request.min_citation_count.map(|value| value.to_string());
        if let Some(min_citation_count) = min_citation_count.as_deref() {
            query.push(("minCitationCount", min_citation_count));
        }
        let open_access_pdf = request.open_access_pdf.map(|value| value.to_string());
        if let Some(open_access_pdf) = open_access_pdf.as_deref() {
            query.push(("openAccessPdf", open_access_pdf));
        }
        let value = self.transport.get_json(
            &format!("/paper/search?{}", encode_query(&query)),
            self.api_key.as_deref(),
        )?;
        let response: SemanticScholarSearchResponse = serde_json::from_value(value)?;
        Ok(response.data)
    }

    pub fn get_paper(&mut self, paper_id: &str, fields: &[String]) -> Result<SemanticScholarPaper> {
        if paper_id.trim().is_empty() {
            bail!("Semantic Scholar paper id must not be empty");
        }
        let path = format!(
            "/paper/{}?fields={}",
            percent_encode(paper_id),
            percent_encode(&fields.join(","))
        );
        let value = self.transport.get_json(&path, self.api_key.as_deref())?;
        Ok(serde_json::from_value(value)?)
    }

    pub fn recommend_papers(
        &mut self,
        positive_paper_ids: &[String],
        negative_paper_ids: &[String],
        limit: usize,
        fields: &[String],
    ) -> Result<Vec<SemanticScholarPaper>> {
        if positive_paper_ids.is_empty() {
            bail!("Semantic Scholar recommendations require at least one positive paper id");
        }
        let body = serde_json::json!({
            "positivePaperIds": positive_paper_ids,
            "negativePaperIds": negative_paper_ids,
        });
        let path = format!(
            "/recommendations/v1/papers?limit={}&fields={}",
            limit,
            percent_encode(&fields.join(","))
        );
        let value = self
            .transport
            .post_json(&path, &body, self.api_key.as_deref())?;
        let response: SemanticScholarRecommendationResponse = serde_json::from_value(value)?;
        Ok(response.recommended_papers)
    }
}

fn encode_query(params: &[(&str, &str)]) -> String {
    params
        .iter()
        .filter(|(_, value)| !value.is_empty())
        .map(|(key, value)| format!("{}={}", percent_encode(key), percent_encode(value)))
        .collect::<Vec<_>>()
        .join("&")
}

fn percent_encode(value: &str) -> String {
    value
        .bytes()
        .flat_map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                vec![byte as char]
            }
            _ => format!("%{byte:02X}").chars().collect(),
        })
        .collect()
}

pub fn semantic_scholar_identifier(record: &PaperSourceRecord) -> Option<String> {
    if let Some(doi) = record
        .doi
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        return Some(format!("DOI:{}", doi.trim()));
    }
    if let Some(arxiv_id) = record
        .arxiv_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        return Some(format!("ARXIV:{}", arxiv_id.trim()));
    }
    record
        .url
        .as_deref()
        .filter(|value| value.contains("semanticscholar.org/paper/"))
        .map(str::to_string)
}

pub fn enrich_registry_with_semantic_scholar(
    config: &RepoConfig,
    registry: &[PaperSourceRecord],
) -> Result<Vec<PaperSourceRecord>> {
    let scholar_config = config.semantic_scholar_config();
    let client = SemanticScholarClient::from_config(scholar_config)?;
    enrich_registry_with_semantic_scholar_client(config, registry, &client)
}

pub fn enrich_registry_with_semantic_scholar_client<T>(
    config: &RepoConfig,
    registry: &[PaperSourceRecord],
    client: &SemanticScholarClient<T>,
) -> Result<Vec<PaperSourceRecord>>
where
    T: SemanticScholarTransport,
{
    let scholar_config = config.semantic_scholar_config();
    if !scholar_config.enabled {
        return Ok(registry.to_vec());
    }

    let mut positions = Vec::new();
    let mut identifiers = Vec::new();
    for (index, record) in registry.iter().enumerate() {
        if let Some(identifier) = semantic_scholar_identifier(record) {
            positions.push(index);
            identifiers.push(identifier);
        }
    }

    let response = client.batch_papers(&identifiers)?;
    let mut enriched = registry.to_vec();
    let mut by_position: BTreeMap<usize, SemanticScholarPaper> = BTreeMap::new();
    for (position, paper) in positions.into_iter().zip(response.papers.into_iter()) {
        if let Some(paper) = paper {
            by_position.insert(position, paper);
        }
    }
    for (position, paper) in by_position {
        enriched[position].semantic_scholar = Some(paper);
    }
    Ok(enriched)
}
