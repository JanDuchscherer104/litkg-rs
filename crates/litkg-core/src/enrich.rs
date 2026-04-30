use crate::ParsedPaper;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

const MIN_TOKEN_LEN: usize = 4;
const MIN_SHARED_TERMS: usize = 2;
const MIN_SIMILARITY_SCORE: f64 = 0.18;
const MAX_TOPIC_FANOUT: usize = 3;
const MAX_EVIDENCE_TERMS: usize = 5;

const STOPWORDS: &[&str] = &[
    "about",
    "after",
    "algorithm",
    "algorithms",
    "also",
    "among",
    "because",
    "before",
    "being",
    "between",
    "conclusion",
    "conclusions",
    "content",
    "dataset",
    "datasets",
    "discussion",
    "during",
    "each",
    "experiments",
    "figure",
    "figures",
    "from",
    "have",
    "into",
    "introduction",
    "method",
    "methods",
    "model",
    "models",
    "paper",
    "propose",
    "proposed",
    "results",
    "section",
    "study",
    "table",
    "tables",
    "their",
    "these",
    "this",
    "through",
    "using",
    "with",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EnrichedEdgeType {
    CitesPaper,
    SimilarTopic,
}

impl EnrichedEdgeType {
    pub fn rel_type(&self) -> &'static str {
        match self {
            Self::CitesPaper => "CITES_PAPER",
            Self::SimilarTopic => "SIMILAR_TOPIC",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EnrichmentStrategy {
    ExactCitationKey,
    CitationDoi,
    CitationArxiv,
    CitationTitle,
    WeightedTokenOverlap,
}

impl EnrichmentStrategy {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ExactCitationKey => "exact_citation_key",
            Self::CitationDoi => "citation_doi",
            Self::CitationArxiv => "citation_arxiv",
            Self::CitationTitle => "citation_title",
            Self::WeightedTokenOverlap => "weighted_token_overlap",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EnrichedEdge {
    pub source_paper_id: String,
    pub target_paper_id: String,
    pub edge_type: EnrichedEdgeType,
    pub strategy: EnrichmentStrategy,
    pub score: f64,
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone)]
struct TopicFeatures {
    paper_id: String,
    tokens: BTreeSet<String>,
}

pub fn infer_enriched_edges(papers: &[ParsedPaper]) -> Vec<EnrichedEdge> {
    let mut edges = infer_citation_edges(papers);
    edges.extend(infer_similar_topic_edges(papers));
    edges.sort_by(|left, right| {
        left.source_paper_id
            .cmp(&right.source_paper_id)
            .then_with(|| left.edge_type.rel_type().cmp(right.edge_type.rel_type()))
            .then_with(|| left.target_paper_id.cmp(&right.target_paper_id))
    });
    edges
}

fn infer_citation_edges(papers: &[ParsedPaper]) -> Vec<EnrichedEdge> {
    let index = CitationTargetIndex::from_papers(papers);
    let mut citation_refs_by_key: BTreeMap<(&str, String), &crate::model::CitationReference> =
        BTreeMap::new();
    for paper in papers {
        for citation_ref in &paper.citation_references {
            if let Some(normalized) = normalize_citation_key(&citation_ref.key) {
                citation_refs_by_key.insert((&paper.metadata.paper_id, normalized), citation_ref);
            }
        }
    }

    let mut edges = Vec::new();
    for paper in papers {
        let mut resolved_targets = BTreeSet::new();
        for citation in &paper.citations {
            let Some(normalized) = normalize_citation_key(citation) else {
                continue;
            };
            let citation_ref = citation_refs_by_key
                .get(&(paper.metadata.paper_id.as_str(), normalized.clone()))
                .copied();
            let Some((target_paper_id, strategy, evidence)) = index.resolve(citation, citation_ref)
            else {
                continue;
            };
            if target_paper_id == paper.metadata.paper_id
                || !resolved_targets.insert(target_paper_id.clone())
            {
                continue;
            }
            edges.push(EnrichedEdge {
                source_paper_id: paper.metadata.paper_id.clone(),
                target_paper_id,
                edge_type: EnrichedEdgeType::CitesPaper,
                strategy,
                score: 1.0,
                evidence,
            });
        }
    }
    edges
}

#[derive(Debug, Default)]
struct CitationTargetIndex {
    by_citation_key: BTreeMap<String, BTreeSet<String>>,
    by_doi: BTreeMap<String, BTreeSet<String>>,
    by_arxiv: BTreeMap<String, BTreeSet<String>>,
    by_title: BTreeMap<String, BTreeSet<String>>,
}

impl CitationTargetIndex {
    fn from_papers(papers: &[ParsedPaper]) -> Self {
        let mut index = Self::default();
        for paper in papers {
            let paper_id = &paper.metadata.paper_id;
            if let Some(citation_key) = &paper.metadata.citation_key {
                if let Some(normalized) = normalize_citation_key(citation_key) {
                    index
                        .by_citation_key
                        .entry(normalized)
                        .or_default()
                        .insert(paper_id.clone());
                }
            }
            if let Some(doi) = normalize_doi_opt(paper.metadata.doi.as_deref()) {
                index
                    .by_doi
                    .entry(doi)
                    .or_default()
                    .insert(paper_id.clone());
            }
            if let Some(arxiv_id) = normalize_arxiv_opt(paper.metadata.arxiv_id.as_deref()) {
                index
                    .by_arxiv
                    .entry(arxiv_id)
                    .or_default()
                    .insert(paper_id.clone());
            }
            if let Some(title) = normalize_title(&paper.metadata.title) {
                index
                    .by_title
                    .entry(title)
                    .or_default()
                    .insert(paper_id.clone());
            }
            if let Some(semantic) = &paper.metadata.semantic_scholar {
                if let Some(doi) = semantic
                    .external_ids
                    .get("DOI")
                    .and_then(|value| normalize_doi_opt(Some(value)))
                {
                    index
                        .by_doi
                        .entry(doi)
                        .or_default()
                        .insert(paper_id.clone());
                }
                if let Some(arxiv_id) = semantic
                    .external_ids
                    .get("ArXiv")
                    .and_then(|value| normalize_arxiv_opt(Some(value)))
                {
                    index
                        .by_arxiv
                        .entry(arxiv_id)
                        .or_default()
                        .insert(paper_id.clone());
                }
                if let Some(title) = semantic
                    .title
                    .as_ref()
                    .and_then(|value| normalize_title(value))
                {
                    index
                        .by_title
                        .entry(title)
                        .or_default()
                        .insert(paper_id.clone());
                }
            }
        }
        index
    }

    fn resolve(
        &self,
        raw_citation: &str,
        citation_ref: Option<&crate::model::CitationReference>,
    ) -> Option<(String, EnrichmentStrategy, Vec<String>)> {
        if let Some(normalized) = normalize_citation_key(raw_citation) {
            if let Some(target) = unique_target(self.by_citation_key.get(&normalized)) {
                return Some((
                    target,
                    EnrichmentStrategy::ExactCitationKey,
                    vec![raw_citation.to_string()],
                ));
            }
        }

        let citation_ref = citation_ref?;
        if let Some(doi) = normalize_doi_opt(citation_ref.doi.as_deref()) {
            if let Some(target) = unique_target(self.by_doi.get(&doi)) {
                return Some((
                    target,
                    EnrichmentStrategy::CitationDoi,
                    vec![format!("{} -> DOI:{doi}", citation_ref.key)],
                ));
            }
        }
        if let Some(arxiv_id) = normalize_arxiv_opt(citation_ref.arxiv_id.as_deref()) {
            if let Some(target) = unique_target(self.by_arxiv.get(&arxiv_id)) {
                return Some((
                    target,
                    EnrichmentStrategy::CitationArxiv,
                    vec![format!("{} -> ARXIV:{arxiv_id}", citation_ref.key)],
                ));
            }
        }
        if let Some(title) = citation_ref
            .title
            .as_ref()
            .and_then(|value| normalize_title(value))
        {
            if let Some(target) = unique_target(self.by_title.get(&title)) {
                return Some((
                    target,
                    EnrichmentStrategy::CitationTitle,
                    vec![format!(
                        "{} -> {}",
                        citation_ref.key,
                        citation_ref.title.as_ref().unwrap()
                    )],
                ));
            }
        }
        None
    }
}

fn unique_target(targets: Option<&BTreeSet<String>>) -> Option<String> {
    let targets = targets?;
    if targets.len() == 1 {
        targets.iter().next().cloned()
    } else {
        None
    }
}

fn infer_similar_topic_edges(papers: &[ParsedPaper]) -> Vec<EnrichedEdge> {
    if papers.len() < 2 {
        return Vec::new();
    }

    let mut features: Vec<_> = papers
        .iter()
        .map(|paper| TopicFeatures {
            paper_id: paper.metadata.paper_id.clone(),
            tokens: tokenize_high_signal_text(paper),
        })
        .collect();
    features.sort_by(|left, right| left.paper_id.cmp(&right.paper_id));

    let doc_frequency = build_doc_frequency(&features);
    let corpus_size = features.len();
    for feature in &mut features {
        feature.tokens = feature
            .tokens
            .iter()
            .filter(|token| !is_broad_token(token, &doc_frequency, corpus_size))
            .cloned()
            .collect();
    }

    let mut candidates = Vec::new();
    for (left_index, left) in features.iter().enumerate() {
        for right in features.iter().skip(left_index + 1) {
            let shared_terms: Vec<_> = left.tokens.intersection(&right.tokens).cloned().collect();
            if shared_terms.len() < MIN_SHARED_TERMS {
                continue;
            }

            let similarity = weighted_jaccard(&left.tokens, &right.tokens, &doc_frequency);
            if similarity < MIN_SIMILARITY_SCORE {
                continue;
            }

            candidates.push(EnrichedEdge {
                source_paper_id: left.paper_id.clone(),
                target_paper_id: right.paper_id.clone(),
                edge_type: EnrichedEdgeType::SimilarTopic,
                strategy: EnrichmentStrategy::WeightedTokenOverlap,
                score: round_score(similarity),
                evidence: select_evidence(&shared_terms, &doc_frequency),
            });
        }
    }

    candidates.sort_by(compare_candidate_priority);
    let mut selected = Vec::new();
    let mut degree_by_paper: BTreeMap<String, usize> = BTreeMap::new();
    for candidate in candidates {
        let source_degree = degree_by_paper
            .get(&candidate.source_paper_id)
            .copied()
            .unwrap_or_default();
        let target_degree = degree_by_paper
            .get(&candidate.target_paper_id)
            .copied()
            .unwrap_or_default();
        if source_degree >= MAX_TOPIC_FANOUT || target_degree >= MAX_TOPIC_FANOUT {
            continue;
        }
        *degree_by_paper
            .entry(candidate.source_paper_id.clone())
            .or_default() += 1;
        *degree_by_paper
            .entry(candidate.target_paper_id.clone())
            .or_default() += 1;
        selected.push(candidate);
    }
    selected
}

fn normalize_citation_key(raw: &str) -> Option<String> {
    let normalized: String = raw
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect();
    if normalized.len() < 3 {
        return None;
    }
    Some(normalized)
}

fn normalize_doi_opt(raw: Option<&str>) -> Option<String> {
    let value = raw?
        .trim()
        .trim_start_matches("https://doi.org/")
        .trim_start_matches("http://doi.org/")
        .trim_start_matches("doi:")
        .trim()
        .trim_end_matches('.');
    if value.to_ascii_lowercase().starts_with("10.") {
        Some(value.to_ascii_lowercase())
    } else {
        None
    }
}

fn normalize_arxiv_opt(raw: Option<&str>) -> Option<String> {
    let value = raw?
        .trim()
        .trim_start_matches("arXiv:")
        .trim_start_matches("arxiv:")
        .trim_start_matches("https://arxiv.org/abs/")
        .trim_start_matches("http://arxiv.org/abs/")
        .trim_start_matches("https://arxiv.org/pdf/")
        .trim_start_matches("http://arxiv.org/pdf/")
        .trim_end_matches(".pdf")
        .trim();
    let normalized = value.to_ascii_lowercase();
    if normalized
        .chars()
        .next()
        .is_some_and(|character| character.is_ascii_digit())
        && normalized.contains('.')
    {
        Some(normalized)
    } else {
        None
    }
}

fn normalize_title(raw: &str) -> Option<String> {
    let normalized = raw
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect::<String>();
    if normalized.len() >= 12 {
        Some(normalized)
    } else {
        None
    }
}

fn tokenize_high_signal_text(paper: &ParsedPaper) -> BTreeSet<String> {
    let mut text = Vec::new();
    text.push(paper.metadata.title.as_str());
    if let Some(abstract_text) = &paper.abstract_text {
        text.push(abstract_text.as_str());
    }
    for section in &paper.sections {
        text.push(section.title.as_str());
    }
    for figure in &paper.figures {
        text.push(figure.caption.as_str());
    }
    for table in &paper.tables {
        text.push(table.caption.as_str());
    }

    text.into_iter()
        .flat_map(tokenize)
        .filter(|token| !STOPWORDS.contains(&token.as_str()))
        .collect()
}

fn tokenize(text: &str) -> Vec<String> {
    text.split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter_map(normalize_token)
        .collect()
}

fn normalize_token(raw: &str) -> Option<String> {
    let trimmed = raw.trim().to_ascii_lowercase();
    if trimmed.len() < MIN_TOKEN_LEN || trimmed.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }
    Some(trimmed)
}

fn build_doc_frequency(features: &[TopicFeatures]) -> BTreeMap<String, usize> {
    let mut doc_frequency = BTreeMap::new();
    for feature in features {
        for token in &feature.tokens {
            *doc_frequency.entry(token.clone()).or_default() += 1;
        }
    }
    doc_frequency
}

fn is_broad_token(
    token: &str,
    doc_frequency: &BTreeMap<String, usize>,
    corpus_size: usize,
) -> bool {
    if corpus_size < 4 {
        return false;
    }
    let frequency = doc_frequency.get(token).copied().unwrap_or_default();
    frequency * 2 > corpus_size
}

fn weighted_jaccard(
    left: &BTreeSet<String>,
    right: &BTreeSet<String>,
    doc_frequency: &BTreeMap<String, usize>,
) -> f64 {
    let shared_weight = left
        .intersection(right)
        .map(|token| token_weight(token, doc_frequency))
        .sum::<f64>();
    if shared_weight == 0.0 {
        return 0.0;
    }

    let union_weight = left
        .union(right)
        .map(|token| token_weight(token, doc_frequency))
        .sum::<f64>();
    if union_weight == 0.0 {
        return 0.0;
    }

    shared_weight / union_weight
}

fn token_weight(token: &str, doc_frequency: &BTreeMap<String, usize>) -> f64 {
    let frequency = doc_frequency.get(token).copied().unwrap_or(1) as f64;
    1.0 / frequency
}

fn select_evidence(
    shared_terms: &[String],
    doc_frequency: &BTreeMap<String, usize>,
) -> Vec<String> {
    let mut evidence = shared_terms.to_vec();
    evidence.sort_by(|left, right| {
        doc_frequency
            .get(left)
            .copied()
            .unwrap_or_default()
            .cmp(&doc_frequency.get(right).copied().unwrap_or_default())
            .then_with(|| left.cmp(right))
    });
    evidence.truncate(MAX_EVIDENCE_TERMS);
    evidence
}

fn round_score(value: f64) -> f64 {
    (value * 1_000_000.0).round() / 1_000_000.0
}

fn compare_candidate_priority(left: &EnrichedEdge, right: &EnrichedEdge) -> Ordering {
    right
        .score
        .partial_cmp(&left.score)
        .unwrap_or(Ordering::Equal)
        .then_with(|| left.source_paper_id.cmp(&right.source_paper_id))
        .then_with(|| left.target_paper_id.cmp(&right.target_paper_id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        CitationReference, DownloadMode, PaperFigure, PaperSection, PaperSourceRecord, PaperTable,
        ParseStatus, SourceKind,
    };

    #[test]
    fn infers_similar_topic_edges_from_high_signal_text() {
        let papers = vec![
            paper(
                "slam-survey",
                "Visual odometry with bundle adjustment",
                Some("This paper studies pose graph optimization for stereo visual odometry."),
                &["Bundle adjustment", "Pose graph optimization"],
                &["Stereo visual odometry pipeline"],
                &[],
            ),
            paper(
                "stereo-tracking",
                "Stereo visual odometry and pose graph refinement",
                Some("We improve bundle adjustment for stereo tracking and pose graph updates."),
                &["Pose graph refinement", "Bundle adjustment"],
                &[],
                &["Stereo tracking results"],
            ),
            paper(
                "protein-folding",
                "Protein folding with diffusion sampling",
                Some("A diffusion model for folding proteins from amino acid sequences."),
                &["Protein structure inference"],
                &[],
                &[],
            ),
        ];

        let edges = infer_enriched_edges(&papers);
        let similar_edges: Vec<_> = edges
            .iter()
            .filter(|edge| edge.edge_type == EnrichedEdgeType::SimilarTopic)
            .collect();
        assert_eq!(similar_edges.len(), 1);
        let edge = similar_edges[0];
        assert_eq!(edge.source_paper_id, "slam-survey");
        assert_eq!(edge.target_paper_id, "stereo-tracking");
        assert_eq!(edge.strategy, EnrichmentStrategy::WeightedTokenOverlap);
        assert!(edge.score >= MIN_SIMILARITY_SCORE);
        assert!(edge.evidence.contains(&"adjustment".to_string()));
        assert!(edge.evidence.contains(&"bundle".to_string()));
    }

    #[test]
    fn inference_is_order_independent() {
        let papers = vec![
            paper(
                "paper-b",
                "Bundle adjustment for visual odometry",
                Some("Pose graph optimization for stereo SLAM."),
                &["Pose graph optimization"],
                &[],
                &[],
            ),
            paper(
                "paper-a",
                "Stereo visual odometry with bundle adjustment",
                Some("We study pose graph refinement for SLAM systems."),
                &["Bundle adjustment"],
                &[],
                &[],
            ),
            paper(
                "paper-c",
                "Protein diffusion sampling",
                Some("Protein folding under diffusion dynamics."),
                &["Protein structure"],
                &[],
                &[],
            ),
        ];
        let mut reversed = papers.clone();
        reversed.reverse();

        assert_eq!(
            infer_enriched_edges(&papers),
            infer_enriched_edges(&reversed)
        );
    }

    #[test]
    fn resolves_local_citations_by_exact_citation_key() {
        let mut citing_paper = paper(
            "paper-c",
            "Evaluation of stereo SLAM baselines",
            Some("We extend prior stereo visual odometry systems."),
            &["Experimental setup"],
            &[],
            &[],
        );
        citing_paper.citations = vec![
            "paper-a2026".into(),
            "Paper-B2026".into(),
            "paper-a2026".into(),
        ];

        let edges = infer_enriched_edges(&[
            paper(
                "paper-a",
                "Stereo visual odometry with bundle adjustment",
                Some("Pose graph refinement for stereo visual odometry."),
                &["Bundle adjustment"],
                &[],
                &[],
            ),
            citing_paper,
            paper(
                "paper-b",
                "Pose graph refinement for stereo odometry",
                Some("Bundle adjustment improves stereo visual tracking."),
                &["Pose graph refinement"],
                &[],
                &[],
            ),
        ]);
        let citation_edges: Vec<_> = edges
            .iter()
            .filter(|edge| edge.edge_type == EnrichedEdgeType::CitesPaper)
            .collect();

        assert_eq!(citation_edges.len(), 2);
        assert_eq!(citation_edges[0].source_paper_id, "paper-c");
        assert_eq!(citation_edges[0].target_paper_id, "paper-a");
        assert_eq!(
            citation_edges[0].strategy,
            EnrichmentStrategy::ExactCitationKey
        );
        assert_eq!(citation_edges[0].score, 1.0);
        assert_eq!(citation_edges[1].target_paper_id, "paper-b");
    }

    #[test]
    fn resolves_local_citations_by_bibliography_identifiers_and_titles() {
        let mut citing_paper = paper(
            "paper-c",
            "Evaluation of NBV baselines",
            Some("We cite papers using a local bibliography with different keys."),
            &["Related work"],
            &[],
            &[],
        );
        citing_paper.citations = vec![
            "local-doi-key".into(),
            "local-arxiv-key".into(),
            "local-title-key".into(),
        ];
        citing_paper.citation_references = vec![
            CitationReference {
                key: "local-doi-key".into(),
                doi: Some("10.1109/3DV62453.2024.00044".into()),
                ..CitationReference::default()
            },
            CitationReference {
                key: "local-arxiv-key".into(),
                arxiv_id: Some("2406.10224".into()),
                ..CitationReference::default()
            },
            CitationReference {
                key: "local-title-key".into(),
                title: Some("Title Only Target Paper".into()),
                ..CitationReference::default()
            },
        ];

        let mut doi_target = paper("paper-doi", "Dynamic 3D Gaussians", None, &[], &[], &[]);
        doi_target.metadata.doi = Some("10.1109/3dv62453.2024.00044".into());
        let mut arxiv_target = paper("paper-arxiv", "EFM3D", None, &[], &[], &[]);
        arxiv_target.metadata.arxiv_id = Some("2406.10224".into());
        let title_target = paper(
            "paper-title",
            "Title Only Target Paper",
            None,
            &[],
            &[],
            &[],
        );

        let edges = infer_enriched_edges(&[doi_target, arxiv_target, title_target, citing_paper]);
        let citation_edges: Vec<_> = edges
            .iter()
            .filter(|edge| edge.edge_type == EnrichedEdgeType::CitesPaper)
            .collect();

        assert_eq!(citation_edges.len(), 3);
        assert!(citation_edges.iter().any(|edge| {
            edge.target_paper_id == "paper-doi" && edge.strategy == EnrichmentStrategy::CitationDoi
        }));
        assert!(citation_edges.iter().any(|edge| {
            edge.target_paper_id == "paper-arxiv"
                && edge.strategy == EnrichmentStrategy::CitationArxiv
        }));
        assert!(citation_edges.iter().any(|edge| {
            edge.target_paper_id == "paper-title"
                && edge.strategy == EnrichmentStrategy::CitationTitle
        }));
    }

    #[test]
    fn serializes_edge_shape_stably() {
        let edge = EnrichedEdge {
            source_paper_id: "paper-a".into(),
            target_paper_id: "paper-b".into(),
            edge_type: EnrichedEdgeType::SimilarTopic,
            strategy: EnrichmentStrategy::WeightedTokenOverlap,
            score: 0.25,
            evidence: vec!["bundle".into(), "odometry".into()],
        };

        let serialized = serde_json::to_string(&edge).unwrap();
        assert_eq!(
            serialized,
            "{\"source_paper_id\":\"paper-a\",\"target_paper_id\":\"paper-b\",\"edge_type\":\"SIMILAR_TOPIC\",\"strategy\":\"weighted_token_overlap\",\"score\":0.25,\"evidence\":[\"bundle\",\"odometry\"]}"
        );
    }

    fn paper(
        paper_id: &str,
        title: &str,
        abstract_text: Option<&str>,
        sections: &[&str],
        figures: &[&str],
        tables: &[&str],
    ) -> ParsedPaper {
        ParsedPaper {
            kind: crate::model::DocumentKind::Literature,
            metadata: PaperSourceRecord {
                paper_id: paper_id.into(),
                citation_key: Some(format!("{paper_id}2026")),
                title: title.into(),
                authors: vec!["Test Author".into()],
                year: Some("2026".into()),
                arxiv_id: None,
                doi: None,
                url: None,
                tex_dir: None,
                pdf_file: None,
                source_kind: SourceKind::ManifestAndBib,
                download_mode: DownloadMode::ManifestSource,
                has_local_tex: true,
                has_local_pdf: false,
                parse_status: ParseStatus::Parsed,
                semantic_scholar: None,
            },
            abstract_text: abstract_text.map(str::to_string),
            sections: sections
                .iter()
                .map(|title| PaperSection {
                    level: 1,
                    title: (*title).into(),
                    content: String::new(),
                })
                .collect(),
            figures: figures
                .iter()
                .map(|caption| PaperFigure {
                    caption: (*caption).into(),
                })
                .collect(),
            tables: tables
                .iter()
                .map(|caption| PaperTable {
                    caption: (*caption).into(),
                })
                .collect(),
            citations: Vec::new(),
            citation_references: Vec::new(),
            provenance: Vec::new(),
        }
    }
}
