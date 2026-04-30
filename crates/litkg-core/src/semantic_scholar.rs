use crate::config::{default_semantic_scholar_fields, RepoConfig, SemanticScholarConfig};
use crate::model::{PaperSourceRecord, SemanticScholarAuthor, SemanticScholarPaper};
use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::env;
use std::thread;
use std::time::{Duration, Instant};

use std::path::PathBuf;

const GRAPH_BASE: &str = "https://api.semanticscholar.org/graph/v1";
const RECOMMENDATIONS_BASE: &str = "https://api.semanticscholar.org/recommendations/v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SemanticScholarService {
    Graph,
    Recommendations,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticScholarMethod {
    Get,
    Post,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SemanticScholarRequest {
    method: SemanticScholarMethod,
    service: SemanticScholarService,
    path: String,
    params: Vec<(String, String)>,
    body: Option<Value>,
}

impl SemanticScholarRequest {
    fn cache_key(&self) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(match self.method {
            SemanticScholarMethod::Get => b"GET".as_slice(),
            SemanticScholarMethod::Post => b"POST".as_slice(),
        });
        hasher.update(b"|");
        hasher.update(match self.service {
            SemanticScholarService::Graph => b"GRAPH".as_slice(),
            SemanticScholarService::Recommendations => b"REC".as_slice(),
        });
        hasher.update(b"|");
        hasher.update(self.path.as_bytes());
        hasher.update(b"|");
        for (k, v) in &self.params {
            hasher.update(k.as_bytes());
            hasher.update(b"=");
            hasher.update(v.as_bytes());
            hasher.update(b"&");
        }
        hasher.update(b"|");
        if let Some(body) = &self.body {
            hasher.update(serde_json::to_string(body).unwrap().as_bytes());
        }
        hex::encode(hasher.finalize())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SemanticScholarHttpResponse {
    pub status: u16,
    pub retry_after_s: Option<f64>,
    pub body: Value,
}

pub trait SemanticScholarTransport {
    fn request(&mut self, request: &SemanticScholarRequest) -> Result<SemanticScholarHttpResponse>;
}

pub struct UreqSemanticScholarTransport {
    agent: ureq::Agent,
    graph_base: String,
    recommendations_base: String,
    api_key: Option<String>,
}

impl UreqSemanticScholarTransport {
    pub fn from_config(config: &SemanticScholarConfig) -> Result<Self> {
        let agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(30)))
            .build()
            .into();
        Ok(Self {
            agent,
            graph_base: GRAPH_BASE.into(),
            recommendations_base: RECOMMENDATIONS_BASE.into(),
            api_key: env::var(&config.api_key_env)
                .ok()
                .filter(|key| !key.is_empty()),
        })
    }

    fn url(&self, service: SemanticScholarService, path: &str) -> String {
        let base = match service {
            SemanticScholarService::Graph => &self.graph_base,
            SemanticScholarService::Recommendations => &self.recommendations_base,
        };
        format!("{}{}", base.trim_end_matches('/'), path)
    }
}

impl SemanticScholarTransport for UreqSemanticScholarTransport {
    fn request(&mut self, request: &SemanticScholarRequest) -> Result<SemanticScholarHttpResponse> {
        let url = self.url(request.service, &request.path);
        let mut response = match (&request.method, &request.body) {
            (SemanticScholarMethod::Get, _) => {
                let mut builder = self.agent.get(&url);
                for (key, value) in &request.params {
                    builder = builder.query(key, value);
                }
                if let Some(api_key) = &self.api_key {
                    builder = builder.header("x-api-key", api_key);
                }
                builder.call()
            }
            (SemanticScholarMethod::Post, body) => {
                let mut builder = self.agent.post(&url);
                for (key, value) in &request.params {
                    builder = builder.query(key, value);
                }
                if let Some(api_key) = &self.api_key {
                    builder = builder.header("x-api-key", api_key);
                }
                match body {
                    Some(body) => builder.send_json(body),
                    None => builder.send_empty(),
                }
            }
        }
        .with_context(|| format!("Semantic Scholar request failed for {url}"))?;
        let retry_after_s = response
            .headers()
            .get("Retry-After")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<f64>().ok());
        let status = response.status().as_u16();
        let body = response
            .body_mut()
            .read_json::<Value>()
            .unwrap_or(Value::Null);
        Ok(SemanticScholarHttpResponse {
            status,
            retry_after_s,
            body,
        })
    }
}

pub struct SemanticScholarClient<T: SemanticScholarTransport = UreqSemanticScholarTransport> {
    config: SemanticScholarConfig,
    transport: T,
    last_request_at: Option<Instant>,
    cache_dir: Option<PathBuf>,
}

impl SemanticScholarClient<UreqSemanticScholarTransport> {
    pub fn from_config(config: SemanticScholarConfig, cache_dir: Option<PathBuf>) -> Result<Self> {
        let transport = UreqSemanticScholarTransport::from_config(&config)?;
        Ok(Self::with_transport(config, transport, cache_dir))
    }
}

impl<T: SemanticScholarTransport> SemanticScholarClient<T> {
    pub fn with_transport(
        config: SemanticScholarConfig,
        transport: T,
        cache_dir: Option<PathBuf>,
    ) -> Self {
        Self {
            config,
            transport,
            last_request_at: None,
            cache_dir,
        }
    }

    pub fn get_paper(&mut self, paper_id: &str, fields: &[String]) -> Result<SemanticScholarPaper> {
        self.request_json(SemanticScholarRequest {
            method: SemanticScholarMethod::Get,
            service: SemanticScholarService::Graph,
            path: format!("/paper/{}", encode_path_segment(paper_id)),
            params: vec![("fields".into(), fields_csv(fields))],
            body: None,
        })
    }

    pub fn get_papers_batch(
        &mut self,
        paper_ids: &[String],
        fields: &[String],
    ) -> Result<Vec<Option<SemanticScholarPaper>>> {
        self.request_json(SemanticScholarRequest {
            method: SemanticScholarMethod::Post,
            service: SemanticScholarService::Graph,
            path: "/paper/batch".into(),
            params: vec![("fields".into(), fields_csv(fields))],
            body: Some(serde_json::json!({ "ids": paper_ids })),
        })
    }

    pub fn search_papers(
        &mut self,
        request: &SemanticScholarSearchRequest,
    ) -> Result<Vec<SemanticScholarPaper>> {
        let mut out = Vec::new();
        let limit = request.limit.max(1);
        let mut token = request.token.clone();
        while out.len() < limit {
            let mut params = request.params();
            if let Some(next_token) = &token {
                params.push(("token".into(), next_token.clone()));
            }
            let page: SemanticScholarSearchResponse =
                self.request_json(SemanticScholarRequest {
                    method: SemanticScholarMethod::Get,
                    service: SemanticScholarService::Graph,
                    path: "/paper/search/bulk".into(),
                    params,
                    body: None,
                })?;
            if page.data.is_empty() {
                break;
            }
            out.extend(page.data);
            token = page.token;
            if token.is_none() {
                break;
            }
        }
        out.truncate(limit);
        Ok(out)
    }

    pub fn recommend_papers(
        &mut self,
        positive_paper_ids: &[String],
        negative_paper_ids: &[String],
        limit: usize,
        fields: &[String],
    ) -> Result<Vec<SemanticScholarPaper>> {
        let response: SemanticScholarRecommendationResponse =
            self.request_json(SemanticScholarRequest {
                method: SemanticScholarMethod::Post,
                service: SemanticScholarService::Recommendations,
                path: "/papers".into(),
                params: vec![
                    ("fields".into(), fields_csv(fields)),
                    ("limit".into(), limit.max(1).to_string()),
                ],
                body: Some(serde_json::json!({
                    "positivePaperIds": positive_paper_ids,
                    "negativePaperIds": negative_paper_ids,
                })),
            })?;
        Ok(response.recommended_papers)
    }

    pub fn get_authors_batch(
        &mut self,
        author_ids: &[String],
        fields: &[String],
    ) -> Result<Vec<Option<SemanticScholarAuthor>>> {
        self.request_json(SemanticScholarRequest {
            method: SemanticScholarMethod::Post,
            service: SemanticScholarService::Graph,
            path: "/author/batch".into(),
            params: vec![("fields".into(), fields_csv(fields))],
            body: Some(serde_json::json!({ "ids": author_ids })),
        })
    }

    pub fn get_citations(
        &mut self,
        paper_id: &str,
        limit: usize,
        fields: &[String],
    ) -> Result<Vec<SemanticScholarPaper>> {
        let page: SemanticScholarCitationPage = self.request_json(SemanticScholarRequest {
            method: SemanticScholarMethod::Get,
            service: SemanticScholarService::Graph,
            path: format!("/paper/{}/citations", encode_path_segment(paper_id)),
            params: vec![
                ("fields".into(), fields_csv(fields)),
                ("limit".into(), limit.max(1).to_string()),
            ],
            body: None,
        })?;
        Ok(page
            .data
            .into_iter()
            .filter_map(|entry| entry.citing_paper)
            .collect())
    }

    pub fn get_references(
        &mut self,
        paper_id: &str,
        limit: usize,
        fields: &[String],
    ) -> Result<Vec<SemanticScholarPaper>> {
        let page: SemanticScholarReferencePage = self.request_json(SemanticScholarRequest {
            method: SemanticScholarMethod::Get,
            service: SemanticScholarService::Graph,
            path: format!("/paper/{}/references", encode_path_segment(paper_id)),
            params: vec![
                ("fields".into(), fields_csv(fields)),
                ("limit".into(), limit.max(1).to_string()),
            ],
            body: None,
        })?;
        Ok(page
            .data
            .into_iter()
            .filter_map(|entry| entry.cited_paper)
            .collect())
    }

    fn request_json<R>(&mut self, request: SemanticScholarRequest) -> Result<R>
    where
        R: for<'de> Deserialize<'de> + Serialize,
    {
        let cache_key = request.cache_key();
        if let Some(dir) = &self.cache_dir {
            if let Ok(data) = cacache::read_sync(dir, &cache_key) {
                if let Ok(parsed) = serde_json::from_slice::<R>(&data) {
                    return Ok(parsed);
                }
            }
        }

        for attempt in 0..=self.config.max_retries {
            self.throttle();
            let response = self.transport.request(&request)?;
            self.last_request_at = Some(Instant::now());
            if response.status == 429 || (500..600).contains(&response.status) {
                if attempt >= self.config.max_retries {
                    return Err(anyhow!(
                        "Semantic Scholar returned HTTP {} after {} attempt(s)",
                        response.status,
                        attempt + 1
                    ));
                }
                let backoff_s = response
                    .retry_after_s
                    .unwrap_or_else(|| 2_f64.powi(attempt as i32).min(16.0));
                if backoff_s > 0.0 {
                    thread::sleep(Duration::from_secs_f64(backoff_s));
                }
                continue;
            }
            if !(200..300).contains(&response.status) {
                return Err(anyhow!(
                    "Semantic Scholar returned HTTP {} with body {}",
                    response.status,
                    response.body
                ));
            }
            let parsed = serde_json::from_value::<R>(response.body)
                .context("Failed to parse Semantic Scholar response")?;
            if let Some(dir) = &self.cache_dir {
                if let Ok(data) = serde_json::to_vec(&parsed) {
                    let _ = cacache::write_sync(dir, &cache_key, data);
                }
            }
            return Ok(parsed);
        }
        unreachable!("retry loop always returns")
    }

    fn throttle(&self) {
        let Some(last_request_at) = self.last_request_at else {
            return;
        };
        let min_interval_s = self.config.min_interval_s.max(0.0);
        let elapsed = last_request_at.elapsed().as_secs_f64();
        if elapsed < min_interval_s {
            thread::sleep(Duration::from_secs_f64(min_interval_s - elapsed));
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SemanticScholarSearchRequest {
    pub query: String,
    pub limit: usize,
    #[serde(default = "default_search_fields")]
    pub fields: Vec<String>,
    #[serde(default)]
    pub year: Option<String>,
    #[serde(default)]
    pub publication_date_or_year: Option<String>,
    #[serde(default)]
    pub fields_of_study: Vec<String>,
    #[serde(default)]
    pub venue: Vec<String>,
    #[serde(default)]
    pub sort: Option<String>,
    #[serde(default)]
    pub min_citation_count: Option<u64>,
    #[serde(default)]
    pub open_access_pdf: Option<bool>,
    #[serde(default)]
    pub token: Option<String>,
}

impl SemanticScholarSearchRequest {
    pub fn new(query: impl Into<String>, limit: usize, fields: Vec<String>) -> Self {
        Self {
            query: query.into(),
            limit,
            fields,
            year: None,
            publication_date_or_year: None,
            fields_of_study: Vec::new(),
            venue: Vec::new(),
            sort: None,
            min_citation_count: None,
            open_access_pdf: None,
            token: None,
        }
    }

    fn params(&self) -> Vec<(String, String)> {
        let mut params = vec![
            ("query".into(), self.query.clone()),
            ("fields".into(), fields_csv(&self.fields)),
        ];
        if let Some(year) = &self.year {
            params.push(("year".into(), year.clone()));
        }
        if let Some(publication_date_or_year) = &self.publication_date_or_year {
            params.push((
                "publicationDateOrYear".into(),
                publication_date_or_year.clone(),
            ));
        }
        if !self.fields_of_study.is_empty() {
            params.push(("fieldsOfStudy".into(), self.fields_of_study.join(",")));
        }
        if !self.venue.is_empty() {
            params.push(("venue".into(), self.venue.join(",")));
        }
        if let Some(sort) = &self.sort {
            params.push(("sort".into(), sort.clone()));
        }
        if let Some(min_citation_count) = self.min_citation_count {
            params.push(("minCitationCount".into(), min_citation_count.to_string()));
        }
        if let Some(open_access_pdf) = self.open_access_pdf {
            params.push(("openAccessPdf".into(), open_access_pdf.to_string()));
        }
        params
    }
}

fn default_search_fields() -> Vec<String> {
    default_semantic_scholar_fields()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SemanticScholarSearchResponse {
    #[serde(default)]
    pub data: Vec<SemanticScholarPaper>,
    #[serde(default)]
    pub token: Option<String>,
    #[serde(default)]
    pub total: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SemanticScholarRecommendationResponse {
    #[serde(rename = "recommendedPapers", default)]
    pub recommended_papers: Vec<SemanticScholarPaper>,
}

pub type SemanticScholarBatchResponse = Vec<Option<SemanticScholarPaper>>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct SemanticScholarCitationPage {
    #[serde(default)]
    data: Vec<SemanticScholarCitationEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct SemanticScholarCitationEntry {
    citing_paper: Option<SemanticScholarPaper>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct SemanticScholarReferencePage {
    #[serde(default)]
    data: Vec<SemanticScholarReferenceEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct SemanticScholarReferenceEntry {
    cited_paper: Option<SemanticScholarPaper>,
}

pub fn enrich_registry_with_semantic_scholar(
    config: &RepoConfig,
    registry: &[PaperSourceRecord],
) -> Result<Vec<PaperSourceRecord>> {
    let semantic_config = config.semantic_scholar_config();
    let cache_dir = Some(config.runtime_cache_root().join("semantic_scholar"));
    let mut client = SemanticScholarClient::from_config(semantic_config.clone(), cache_dir)?;
    enrich_registry_with_semantic_scholar_client(registry, &semantic_config, &mut client)
}

pub fn enrich_registry_with_semantic_scholar_client<T: SemanticScholarTransport>(
    registry: &[PaperSourceRecord],
    config: &SemanticScholarConfig,
    client: &mut SemanticScholarClient<T>,
) -> Result<Vec<PaperSourceRecord>> {
    let fields = if config.fields.is_empty() {
        default_semantic_scholar_fields()
    } else {
        config.fields.clone()
    };
    let batch_size = config.batch_size.max(1);
    let mut updated = registry.to_vec();
    let mut queue = Vec::new();
    for (index, record) in registry.iter().enumerate() {
        if let Some(identifier) = semantic_scholar_identifier(record) {
            queue.push((index, identifier));
        }
    }
    for batch in queue.chunks(batch_size) {
        let ids = batch
            .iter()
            .map(|(_, identifier)| identifier.clone())
            .collect::<Vec<_>>();
        let papers = client.get_papers_batch(&ids, &fields)?;
        for ((index, _), paper) in batch.iter().zip(papers) {
            if let Some(paper) = paper {
                merge_semantic_scholar_paper(&mut updated[*index], paper);
            }
        }
    }
    Ok(updated)
}

pub fn semantic_scholar_identifier(record: &PaperSourceRecord) -> Option<String> {
    if let Some(paper_id) = record
        .semantic_scholar
        .as_ref()
        .and_then(|paper| paper.paper_id.as_deref())
    {
        if !paper_id.trim().is_empty() {
            return Some(paper_id.trim().to_string());
        }
    }
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
    None
}

fn merge_semantic_scholar_paper(record: &mut PaperSourceRecord, paper: SemanticScholarPaper) {
    if record.doi.is_none() {
        record.doi = paper.external_ids.get("DOI").cloned();
    }
    if record.arxiv_id.is_none() {
        record.arxiv_id = paper.external_ids.get("ArXiv").cloned();
    }
    if record.url.is_none() {
        record.url = paper.url.clone();
    }
    if record.year.is_none() {
        record.year = paper.year.map(|year| year.to_string());
    }
    if record.authors.is_empty() && !paper.authors.is_empty() {
        record.authors = paper
            .authors
            .iter()
            .map(|author| author.name.clone())
            .filter(|name| !name.is_empty())
            .collect();
    }
    record.semantic_scholar = Some(paper);
}

fn fields_csv(fields: &[String]) -> String {
    if fields.is_empty() {
        default_semantic_scholar_fields().join(",")
    } else {
        fields.join(",")
    }
}

fn encode_path_segment(raw: &str) -> String {
    let mut encoded = String::new();
    for byte in raw.bytes() {
        let ch = byte as char;
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | '~' | ':') {
            encoded.push(ch);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DownloadMode, ParseStatus, SourceKind};
    use std::collections::VecDeque;

    #[derive(Default)]
    struct FakeTransport {
        requests: Vec<SemanticScholarRequest>,
        responses: VecDeque<SemanticScholarHttpResponse>,
    }

    impl FakeTransport {
        fn push_json(&mut self, body: Value) {
            self.responses.push_back(SemanticScholarHttpResponse {
                status: 200,
                retry_after_s: None,
                body,
            });
        }
    }

    impl SemanticScholarTransport for FakeTransport {
        fn request(
            &mut self,
            request: &SemanticScholarRequest,
        ) -> Result<SemanticScholarHttpResponse> {
            self.requests.push(request.clone());
            self.responses
                .pop_front()
                .context("missing fake Semantic Scholar response")
        }
    }

    fn record() -> PaperSourceRecord {
        PaperSourceRecord {
            paper_id: "vin-nbv".into(),
            citation_key: Some("frahm2025vinnbv".into()),
            title: "VIN-NBV".into(),
            authors: vec![],
            year: None,
            arxiv_id: Some("2501.01234".into()),
            doi: None,
            url: None,
            tex_dir: None,
            pdf_file: None,
            source_kind: SourceKind::Bib,
            download_mode: DownloadMode::MetadataOnly,
            has_local_tex: false,
            has_local_pdf: false,
            parse_status: ParseStatus::MetadataOnly,
            semantic_scholar: None,
        }
    }

    #[test]
    fn builds_identifier_from_doi_before_arxiv() {
        let mut record = record();
        record.doi = Some("10.1000/example".into());
        assert_eq!(
            semantic_scholar_identifier(&record).as_deref(),
            Some("DOI:10.1000/example")
        );
    }

    #[test]
    fn enriches_registry_from_batch_response() {
        let mut transport = FakeTransport::default();
        transport.push_json(serde_json::json!([
            {
                "paperId": "abc123",
                "corpusId": 42,
                "externalIds": {"ArXiv": "2501.01234", "DOI": "10.1000/vin"},
                "title": "VIN-NBV",
                "url": "https://semanticscholar.org/paper/abc123",
                "year": 2025,
                "authors": [{"authorId": "1", "name": "A. Author"}],
                "citationCount": 7,
                "fieldsOfStudy": ["Computer Science"]
            }
        ]));
        let config = SemanticScholarConfig {
            min_interval_s: 0.0,
            ..SemanticScholarConfig::default()
        };
        let mut client = SemanticScholarClient::with_transport(config.clone(), transport, None);

        let enriched =
            enrich_registry_with_semantic_scholar_client(&[record()], &config, &mut client)
                .unwrap();

        assert_eq!(enriched[0].doi.as_deref(), Some("10.1000/vin"));
        assert_eq!(enriched[0].authors, vec!["A. Author"]);
        let paper = enriched[0].semantic_scholar.as_ref().unwrap();
        assert_eq!(paper.paper_id.as_deref(), Some("abc123"));
        assert_eq!(paper.citation_count, Some(7));
    }

    #[test]
    fn search_uses_bulk_endpoint_and_request_params() {
        let mut transport = FakeTransport::default();
        transport.push_json(serde_json::json!({
            "data": [{"paperId": "p1", "title": "Next Best View"}],
            "token": null
        }));
        let config = SemanticScholarConfig {
            min_interval_s: 0.0,
            ..SemanticScholarConfig::default()
        };
        let mut client = SemanticScholarClient::with_transport(config, transport, None);
        let request = SemanticScholarSearchRequest::new(
            "\"next best view\"",
            5,
            vec!["paperId".into(), "title".into()],
        );

        let papers = client.search_papers(&request).unwrap();

        assert_eq!(papers.len(), 1);
        assert_eq!(client.transport.requests[0].path, "/paper/search/bulk");
        assert!(client.transport.requests[0]
            .params
            .contains(&("query".into(), "\"next best view\"".into())));
    }

    #[test]
    fn citation_and_reference_helpers_extract_nested_papers() {
        let mut transport = FakeTransport::default();
        transport.push_json(serde_json::json!({
            "data": [{"citingPaper": {"paperId": "citing", "title": "Citing Paper"}}]
        }));
        transport.push_json(serde_json::json!({
            "data": [{"citedPaper": {"paperId": "cited", "title": "Cited Paper"}}]
        }));
        let config = SemanticScholarConfig {
            min_interval_s: 0.0,
            ..SemanticScholarConfig::default()
        };
        let mut client = SemanticScholarClient::with_transport(config, transport, None);

        let citations = client
            .get_citations("abc123", 10, &["paperId".into(), "title".into()])
            .unwrap();
        let references = client
            .get_references("abc123", 10, &["paperId".into(), "title".into()])
            .unwrap();

        assert_eq!(citations[0].paper_id.as_deref(), Some("citing"));
        assert_eq!(references[0].paper_id.as_deref(), Some("cited"));
        assert_eq!(client.transport.requests[0].path, "/paper/abc123/citations");
        assert_eq!(
            client.transport.requests[1].path,
            "/paper/abc123/references"
        );
    }

    #[test]
    fn encodes_path_segments_for_lookup_ids() {
        assert_eq!(
            encode_path_segment("DOI:10.1000/a b"),
            "DOI:10.1000%2Fa%20b"
        );
    }
}
