use crate::schema::{Conflict, MergeDecision, MergeReason, Provenance, StableId};
use regex::Regex;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

pub fn normalize_doi(doi: &str) -> String {
    let doi = doi.trim().to_lowercase();
    let doi = doi
        .strip_prefix("https://doi.org/")
        .or_else(|| doi.strip_prefix("http://doi.org/"))
        .or_else(|| doi.strip_prefix("doi:"))
        .unwrap_or(&doi);
    doi.trim_matches('/').to_string()
}

pub fn normalize_arxiv_id(id: &str) -> String {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"(?i)(?:arxiv:)?(?:https?://arxiv\.org/(?:abs|pdf)/)?(\d{4}\.\d{4,5})(?:v\d+)?")
            .unwrap()
    });

    if let Some(caps) = re.captures(id) {
        return caps[1].to_string();
    }

    id.trim()
        .to_lowercase()
        .strip_prefix("arxiv:")
        .unwrap_or(id)
        .split('v')
        .next()
        .unwrap_or("")
        .to_string()
}

pub fn normalize_title(title: &str) -> String {
    title
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

pub fn normalize_author_name(name: &str) -> String {
    let name = name.trim();
    if let Some((family, given)) = name.split_once(',') {
        format!(
            "{} {}",
            given.trim().to_lowercase(),
            family.trim().to_lowercase()
        )
    } else {
        name.to_lowercase()
    }
}

pub struct ResolutionCandidate {
    pub title: String,
    pub authors: Vec<String>,
    pub year: Option<String>,
    pub doi: Option<String>,
    pub arxiv_id: Option<String>,
    pub s2_paper_id: Option<String>,
    pub bib_key: Option<String>,
    pub provenance: Provenance,
}

pub struct IdentityResolver {
    candidates: Vec<ResolutionCandidate>,
}

impl IdentityResolver {
    pub fn new() -> Self {
        Self {
            candidates: Vec::new(),
        }
    }

    pub fn add_candidate(&mut self, candidate: ResolutionCandidate) {
        self.candidates.push(candidate);
    }

    pub fn resolve(
        self,
    ) -> (
        BTreeMap<StableId, Vec<ResolutionCandidate>>,
        Vec<MergeDecision>,
        Vec<Conflict>,
    ) {
        let mut decisions = Vec::new();
        let conflicts = Vec::new();

        if self.candidates.is_empty() {
            return (BTreeMap::new(), decisions, conflicts);
        }

        // 1. Build identity map: candidate_index -> Set<NormalizedIdentifier>
        let mut candidate_ids = Vec::new();
        for candidate in &self.candidates {
            let mut ids = BTreeSet::new();
            if let Some(doi) = &candidate.doi {
                ids.insert(format!("doi:{}", normalize_doi(doi)));
            }
            if let Some(arxiv) = &candidate.arxiv_id {
                ids.insert(format!("arxiv:{}", normalize_arxiv_id(arxiv)));
            }
            if let Some(s2) = &candidate.s2_paper_id {
                ids.insert(format!("s2:{}", s2));
            }
            candidate_ids.push(ids);
        }

        // 2. Find connected components of candidates based on shared identifiers
        let n = self.candidates.len();
        let mut adj = vec![Vec::new(); n];
        for i in 0..n {
            for j in i + 1..n {
                if !candidate_ids[i].is_disjoint(&candidate_ids[j]) {
                    adj[i].push(j);
                    adj[j].push(i);
                }
            }
        }

        let mut visited = vec![false; n];
        let mut component_maps = Vec::new();

        for i in 0..n {
            if visited[i] {
                continue;
            }

            let mut component = Vec::new();
            let mut queue = std::collections::VecDeque::new();
            queue.push_back(i);
            visited[i] = true;

            while let Some(u) = queue.pop_front() {
                component.push(u);
                for &v in &adj[u] {
                    if !visited[v] {
                        visited[v] = true;
                        queue.push_back(v);
                    }
                }
            }

            // 3. For each component, determine the canonical StableId
            let (canonical_id, primary_reason) =
                self.select_canonical_id_for_component(&component, &candidate_ids);

            for &idx in &component {
                let candidate = &self.candidates[idx];
                decisions.push(MergeDecision {
                    canonical_id: canonical_id.clone(),
                    merged_id: StableId::new(format!("candidate:{}", idx)),
                    reason: primary_reason,
                    confidence: 1.0,
                    evidence: vec![candidate.provenance.clone()],
                });
            }

            component_maps.push((canonical_id, component));
        }

        // Re-assembling the final output with ownership
        let mut final_grouped = BTreeMap::new();
        let mut temp_map: BTreeMap<usize, ResolutionCandidate> =
            self.candidates.into_iter().enumerate().collect();

        for (id, indices) in component_maps {
            let mut group = Vec::new();
            for idx in indices {
                if let Some(c) = temp_map.remove(&idx) {
                    group.push(c);
                }
            }
            final_grouped.insert(id, group);
        }

        (final_grouped, decisions, conflicts)
    }

    fn select_canonical_id_for_component(
        &self,
        component: &[usize],
        candidate_ids: &[BTreeSet<String>],
    ) -> (StableId, MergeReason) {
        // Collect all IDs in component
        let mut all_ids = BTreeSet::new();
        for &idx in component {
            for id in &candidate_ids[idx] {
                all_ids.insert(id.clone());
            }
        }

        // Priority 1: DOI
        if let Some(doi) = all_ids.iter().find(|id| id.starts_with("doi:")) {
            return (
                StableId::new(format!("paper:{}", doi)),
                MergeReason::DoiExact,
            );
        }
        // Priority 2: arXiv
        if let Some(arxiv) = all_ids.iter().find(|id| id.starts_with("arxiv:")) {
            return (
                StableId::new(format!("paper:{}", arxiv)),
                MergeReason::ArxivExact,
            );
        }
        // Priority 3: S2
        if let Some(s2) = all_ids.iter().find(|id| id.starts_with("s2:")) {
            return (
                StableId::new(format!("paper:{}", s2)),
                MergeReason::S2PaperIdExact,
            );
        }

        // Priority 4: Hash-based (for cases with NO identifiers)
        // Use the hash of the first candidate in the component
        let first_idx = component[0];
        (
            StableId::new(format!("paper:hash:{}", first_idx)),
            MergeReason::Manual,
        )
    }
}
