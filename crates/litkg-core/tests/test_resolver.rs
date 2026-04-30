use litkg_core::{
    normalize_arxiv_id, normalize_doi, normalize_title, IdentityResolver, Provenance,
    ProvenanceSpan, ResolutionCandidate,
};

fn dummy_provenance(id: &str) -> Provenance {
    Provenance {
        source_id: id.to_string(),
        source_hash: format!("hash-{}", id),
        span: ProvenanceSpan::Unknown,
        adapter_name: "test".to_string(),
        adapter_version: "0.1.0".to_string(),
        ingested_at: "2026-04-30T12:00:00Z".to_string(),
        confidence: 1.0,
    }
}

#[test]
fn normalizes_identifiers_correctly() {
    assert_eq!(normalize_doi("https://doi.org/10.1000/182"), "10.1000/182");
    assert_eq!(normalize_doi("doi:10.1000/182/"), "10.1000/182");

    assert_eq!(normalize_arxiv_id("arxiv:2505.06219v3"), "2505.06219");
    assert_eq!(
        normalize_arxiv_id("https://arxiv.org/abs/2402.16174"),
        "2402.16174"
    );

    assert_eq!(
        normalize_title("ViSTA-SLAM: Visual SLAM"),
        "vistaslamvisualslam"
    );
}

#[test]
fn resolves_identity_with_priority() {
    let mut resolver = IdentityResolver::new();

    // Candidate 1: BibTeX with DOI
    resolver.add_candidate(ResolutionCandidate {
        title: "ViSTA-SLAM".to_string(),
        authors: vec!["Zhang".to_string()],
        year: Some("2026".to_string()),
        doi: Some("10.1234/vista".to_string()),
        arxiv_id: Some("2509.01584".to_string()),
        s2_paper_id: None,
        bib_key: Some("zhang2026".to_string()),
        provenance: dummy_provenance("bib"),
    });

    // Candidate 2: Manifest with arXiv
    resolver.add_candidate(ResolutionCandidate {
        title: "ViSTA-SLAM".to_string(),
        authors: vec![],
        year: None,
        doi: None,
        arxiv_id: Some("2509.01584".to_string()),
        s2_paper_id: None,
        bib_key: None,
        provenance: dummy_provenance("manifest"),
    });

    let (grouped, decisions, _) = resolver.resolve();

    // Both should merge under the DOI-based ID because they share an arXiv ID
    assert_eq!(grouped.len(), 1);
    let (id, candidates) = grouped.iter().next().unwrap();
    assert_eq!(id.as_str(), "paper:doi:10.1234/vista");
    assert_eq!(candidates.len(), 2);

    // Check merge decisions
    assert_eq!(decisions.len(), 2);
    assert_eq!(decisions[0].canonical_id, *id);
    assert_eq!(decisions[1].canonical_id, *id);
}

#[test]
fn detects_title_conflicts_for_same_identifier() {
    let mut resolver = IdentityResolver::new();

    resolver.add_candidate(ResolutionCandidate {
        title: "ViSTA-SLAM: Visual SLAM".to_string(),
        authors: vec![],
        year: None,
        doi: Some("10.1234/vista".to_string()),
        arxiv_id: None,
        s2_paper_id: None,
        bib_key: None,
        provenance: dummy_provenance("source1"),
    });

    resolver.add_candidate(ResolutionCandidate {
        title: "Completely Different Paper".to_string(),
        authors: vec![],
        year: None,
        doi: Some("10.1234/vista".to_string()), // Same DOI!
        arxiv_id: None,
        s2_paper_id: None,
        bib_key: None,
        provenance: dummy_provenance("source2"),
    });

    let (grouped, _, conflicts) = resolver.resolve();

    // They should still merge into one group because they share the DOI
    assert_eq!(grouped.len(), 1);

    // But a conflict should be emitted
    assert_eq!(conflicts.len(), 1);
    assert_eq!(
        conflicts[0].kind,
        litkg_core::schema::ConflictKind::DoiTitleMismatch
    );
    assert!(conflicts[0].message.contains("Title mismatch"));
}
