# litkg-rs review issue bundle — 2026-04-14

This file captures issue-ready follow-ups from the 2026-04-14 repo assessment.
It is meant to be easy to copy into GitHub issues or the local `.agents` backlog.

---

## Issue 1 — Replace workstation-bound example configs with portable templates

**Suggested labels:** `bug`, `portability`, `docs`, `good first issue`

### Problem
The repo presents itself as reusable and repo-independent, but the shipped `examples/prml-vslam.toml` is hard-wired to one workstation under `/Users/jd/...`. That makes the example misleading and blocks immediate reuse by other users.

### Acceptance criteria
- No example config committed to the repo contains `/Users/jd/` or other machine-specific absolute paths.
- README quick start points to a portable config that a fresh clone can run.
- At least one end-to-end example works without requiring `prml-vslam` to exist locally.
- Consumer-specific configs are clearly labeled as such.

### Likely touch points
- `examples/prml-vslam.toml`
- `README.md`
- `docs/bootstrap-templates.md`
- `docs/codex-setup.md`

---

## Issue 2 — Define the supported core workflow and demote experimental surfaces

**Suggested labels:** `design`, `product`, `docs`

### Problem
The repo already exposes many surfaces: literature ingest, markdown materialization, Neo4j export, native viewer, benchmark execution, autoresearch target rendering, GitHub issue sync, MLflow tracking, and local KG helper scripts. That breadth creates ambiguity about what is truly core versus experimental.

### Acceptance criteria
- README contains a short supported core workflow section.
- Every major surface is classified as `core`, `optional`, or `experimental`.
- Experimental paths are visibly marked in docs.
- New contributors can tell which path should be hardened first.

### Likely touch points
- `README.md`
- `crates/litkg-cli/src/main.rs`
- `docs/codex-setup.md`
- `Makefile`

---

## Issue 3 — Add a golden-corpus regression suite for TeX parsing fidelity

**Suggested labels:** `parser`, `quality`, `testing`, `high priority`

### Problem
The current TeX parser is a pragmatic extractor built around root discovery, include inlining, regex-based section/caption/citation extraction, and cleanup heuristics. That is useful, but fragile on macro-heavy, math-heavy, or structurally messy papers.

### Acceptance criteria
- A checked-in golden corpus exists with several structurally different papers.
- CI or local test flow exercises parser regression fixtures.
- Regressions in extracted sections, citations, or captions fail visibly.
- Parser limitations are documented next to the regression corpus.

### Likely touch points
- `crates/litkg-core/src/tex.rs`
- `crates/litkg-core/src/materialize.rs`
- parser fixtures/tests
- parser docs

---

## Issue 4 — Separate benchmark and autoresearch machinery from the default ingestion path

**Suggested labels:** `architecture`, `scope`, `refactor`

### Problem
The benchmark and autoresearch subsystem is already large relative to the age of the repo. It risks dominating maintenance before the literature pipeline itself has been proven as the everyday value path.

### Acceptance criteria
- The default onboarding path can ignore benchmark/autoresearch machinery completely.
- CLI and docs clearly separate ingestion from benchmark/autoresearch paths.
- It is easier to reason about the core crate boundaries.
- The benchmark layer no longer reads like a required part of first adoption.

### Likely touch points
- `crates/litkg-cli/src/main.rs`
- `crates/litkg-core/src/benchmark.rs`
- `crates/litkg-core/src/benchmark_runner.rs`
- `README.md`
- `Makefile`

---

## Issue 5 — Add a repo-local end-to-end smoke fixture that proves repo independence

**Suggested labels:** `testing`, `ux`, `portability`, `high priority`

### Problem
The repo claims to be repo-independent, but the main demonstration path currently centers on a downstream consumer repo. There is no obvious tiny, self-contained end-to-end fixture that proves the claim inside this repo alone.

### Acceptance criteria
- Fresh clone users can run one documented end-to-end smoke path with no external repo dependency.
- The smoke fixture covers registry load, parsing, materialization, and at least one export path.
- The path is deterministic and included in local verification guidance.
- README quick start uses or references this fixture.

### Likely touch points
- `examples/`
- `README.md`
- `Makefile`
- fixture data under `tests/fixtures/` or similar

---

## Prioritization recommendation

1. Replace workstation-bound example configs with portable templates.
2. Add a repo-local end-to-end smoke fixture that proves repo independence.
3. Add a golden-corpus regression suite for TeX parsing fidelity.
4. Define the supported core workflow and demote experimental surfaces.
5. Separate benchmark and autoresearch machinery from the default ingestion path.
