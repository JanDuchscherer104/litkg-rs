# Local Repo QA Benchmark

This example benchmark creates deterministic exact-match repository-QA trials
for two local client repos:

- `ARIA-NBV`
- `NBV`
- `prml-vslam`

The ARIA-NBV dataset is hand-curated to cover the agent-critical retrieval
surfaces litkg-rs must route correctly: RRI, VIN, rollouts, cache/offline
stores, LRZ, docs, and KG workflows. The legacy client-repo datasets contain 25
questions generated from stable local repo surfaces:

- top-level Python symbol definitions
- `Makefile` target descriptions
- `README.md` command examples with inline comments

The benchmark is intentionally local and example-scoped. It lives under
`examples/benchmarks/` so client-repo assumptions do not leak into the core
benchmark catalog.

## Files

- `catalog.toml`: benchmark catalog with one benchmark entry per client repo
- `integrations.toml`: local harness descriptions
- `run-plan.toml`: local run plan for this machine
- `aria-nbv.jsonl`: curated 14-trial ARIA-NBV dataset for `/home/jd/repos/ARIA-NBV`
- `nbv.jsonl`: generated 25-trial dataset for `~/repos/NBV`
- `prml-vslam.jsonl`: generated 25-trial dataset for `~/repos/prml-vslam`

## Regenerate Datasets

```bash
python3 scripts/benchmarks/generate_repo_qa_dataset.py \
  --repo-id nbv \
  --repo-root /Users/jd/repos/NBV \
  --output examples/benchmarks/local_repo_qa/nbv.jsonl \
  --trials 25

python3 scripts/benchmarks/generate_repo_qa_dataset.py \
  --repo-id prml-vslam \
  --repo-root /Users/jd/repos/prml-vslam \
  --output examples/benchmarks/local_repo_qa/prml-vslam.jsonl \
  --trials 25
```

## Run

```bash
cargo run -p litkg-cli -- run-benchmarks \
  --catalog examples/benchmarks/local_repo_qa/catalog.toml \
  --integrations examples/benchmarks/local_repo_qa/integrations.toml \
  --plan examples/benchmarks/local_repo_qa/run-plan.toml \
  --output examples/benchmarks/local_repo_qa/latest-results.toml
```
