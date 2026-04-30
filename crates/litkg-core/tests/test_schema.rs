use litkg_core::{
    CanonicalNode, NodeKind, Provenance, ProvenanceSpan, StableId,
};
use serde_json::json;

#[test]
fn creates_canonical_node_with_provenance() {
    let id = StableId::new("paper:doi:10.1000/182");
    let provenance = Provenance {
        source_id: "docs/references.bib".to_string(),
        source_hash: "abc123hash".to_string(),
        span: ProvenanceSpan::LineRange { start: 10, end: 20 },
        adapter_name: "bibtex-adapter".to_string(),
        adapter_version: "0.1.0".to_string(),
        ingested_at: "2026-04-30T12:00:00Z".to_string(),
        confidence: 1.0,
    };

    let node = CanonicalNode {
        id,
        kind: NodeKind::Paper,
        label: "A Unified Knowledge Graph".to_string(),
        aliases: vec![],
        properties: json!({"year": 2026}),
        provenance: vec![provenance],
        schema_version: "0.1.0".to_string(),
    };

    assert_eq!(node.kind, NodeKind::Paper);
    assert_eq!(node.provenance.len(), 1);
    assert_eq!(node.properties["year"], 2026);
}

#[test]
fn serializes_and_deserializes_stable_id() {
    let id = StableId::new("doc:aria-nbv:index.qmd");
    let serialized = serde_json::to_string(&id).unwrap();
    assert_eq!(serialized, "\"doc:aria-nbv:index.qmd\"");

    let deserialized: StableId = serde_json::from_str(&serialized).unwrap();
    assert_eq!(deserialized, id);
}
