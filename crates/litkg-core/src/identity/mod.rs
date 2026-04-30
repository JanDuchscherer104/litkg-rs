pub mod resolution;

pub use resolution::{
    normalize_arxiv_id, normalize_author_name, normalize_doi, normalize_title, IdentityResolver,
    ResolutionCandidate,
};
