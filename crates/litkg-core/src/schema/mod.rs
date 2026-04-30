pub mod id;
pub mod merge;
pub mod ontology;
pub mod provenance;
pub mod record;
pub mod harness;

pub use id::StableId;
pub use merge::{Conflict, ConflictKind, MergeDecision, MergeReason};
pub use ontology::{EdgeKind, NodeKind};
pub use provenance::{Provenance, ProvenanceSpan};
pub use record::{Alias, CanonicalEdge, CanonicalNode};
