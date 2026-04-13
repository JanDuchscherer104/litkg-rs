.DEFAULT_GOAL := help

CPU_COUNT ?= $(shell getconf _NPROCESSORS_ONLN 2>/dev/null || sysctl -n hw.logicalcpu 2>/dev/null || nproc 2>/dev/null || python3 -c 'import os; print(os.cpu_count() or 1)' 2>/dev/null || echo 1)
CARGO ?= cargo
PYTHON ?= python3
AGENTS_DB ?= python3 .agents/scripts/agents_db.py
AGENTS_ARGS ?= rank
KG_CODE_REPO_ROOT ?=
KG_SRC_DIR ?=
KG_DOC_PATHS ?=
KG_SKIP_NEO4J_CHECK ?= 0
GRAPHITI_MODE ?= http
BENCHMARK_CATALOG ?= examples/benchmarks/kg.toml
BENCHMARK_INTEGRATIONS ?= examples/benchmarks/integrations.toml
BENCHMARK_RESULTS ?= examples/benchmarks/sample-results.toml
BENCHMARK_RESULTS_OUT ?= examples/benchmarks/latest-results.toml
BENCHMARK_RUN_PLAN ?=
AUTORESEARCH_TARGET_ID ?= kg_navigation_improvement
AUTORESEARCH_TARGET_FORMAT ?= markdown
GRAPH_CONFIG ?= examples/prml-vslam.toml
AUTORESEARCH_ISSUE_ARGS ?= --dry-run
LITKG_CONFIG ?= examples/prml-vslam.toml
LITKG_DOWNLOAD_ARGS ?=
LITKG_PIPELINE_ARGS ?=
LOC_ARGS ?=

.PHONY: help fmt test lint lint-check cargo-check clippy ci clean agents-db
.PHONY: loc loc-rs loc-py loc-sh loc-docs
.PHONY: kg-up kg-down kg-index kg-index-check kg-index-bootstrap kg-ingest-docs kg-enrich kg-update kg-graphiti kg-smoke
.PHONY: litkg-sync litkg-download litkg-parse litkg-materialize litkg-rebuild-graph litkg-pipeline litkg-export-neo4j
.PHONY: benchmark-validate benchmark-support benchmark-run autoresearch-target inspect-graph autoresearch-issue

fmt: ## Run rustfmt across the workspace
	$(CARGO) fmt --all

test: ## Run the Rust test suite
	$(CARGO) test

lint: ## Run formatting checks and tests
	$(CARGO) fmt --all --check
	$(CARGO) test

lint-check: ## Run non-mutating workspace verification checks
	$(CARGO) fmt --all --check
	$(CARGO) test

cargo-check: ## Run cargo check across the workspace
	$(CARGO) check --workspace

clippy: ## Run clippy across the workspace
	$(CARGO) clippy --workspace --all-targets

ci: ## Run the main local verification suite
	$(MAKE) lint-check
	$(MAKE) benchmark-validate
	$(MAKE) kg-smoke

clean: ## Remove Cargo build artifacts
	$(CARGO) clean

agents-db: ## Show the local .agents backlog (example: make agents-db AGENTS_ARGS="rank --kind todos")
	$(AGENTS_DB) $(AGENTS_ARGS)

loc-rs: ## Print Rust LOC statistics (pass extra flags with LOC_ARGS="--todo --fixme")
	$(PYTHON) scripts/loc_stats.py --roots crates --extensions .rs $(LOC_ARGS)

loc-py: ## Print Python LOC statistics for scripts and .agents helpers
	$(PYTHON) scripts/loc_stats.py --roots scripts .agents --extensions .py $(LOC_ARGS)

loc-sh: ## Print shell LOC statistics for repo scripts
	$(PYTHON) scripts/loc_stats.py --roots scripts --extensions .sh $(LOC_ARGS)

loc-docs: ## Print documentation/config LOC statistics
	$(PYTHON) scripts/loc_stats.py --roots docs examples README.md AGENTS.md CODEOWNER.md Cargo.toml Makefile --extensions .md .toml .yml .yaml .json .jsonl $(LOC_ARGS)

loc: ## Print overall repo LOC statistics
	$(PYTHON) scripts/loc_stats.py $(LOC_ARGS)

kg-up: ## Start the local Neo4j KG stack
	./scripts/kg/up.sh

kg-down: ## Stop the local Neo4j KG stack
	./scripts/kg/down.sh

kg-index: ## Reindex code graph for a repo path (set KG_SRC_DIR=<path>, default .)
	KG_CODE_REPO_ROOT="$(KG_CODE_REPO_ROOT)" ./scripts/kg/index_code.sh "$(if $(strip $(KG_SRC_DIR)),$(KG_SRC_DIR),.)"

kg-index-check: ## Validate indexing prerequisites for a repo path without mutating setup
	KG_SKIP_NEO4J_CHECK="$(KG_SKIP_NEO4J_CHECK)" KG_CODE_REPO_ROOT="$(KG_CODE_REPO_ROOT)" ./scripts/kg/index_code.sh --check "$(if $(strip $(KG_SRC_DIR)),$(KG_SRC_DIR),.)"

kg-index-bootstrap: ## Prepare/reuse the CodeGraphContext runtime without indexing code
	KG_CODE_REPO_ROOT="$(KG_CODE_REPO_ROOT)" ./scripts/kg/index_code.sh --bootstrap

kg-ingest-docs: ## Ingest repo docs into Graphiti (optional KG_DOC_PATHS="path1 path2")
	./scripts/kg/ingest_docs.sh $(KG_DOC_PATHS)

kg-enrich: ## Refresh embeddings and code↔doc links (optional KG_SRC_DIR=<path> scopes code refresh)
	KG_CODE_REPO_ROOT="$(KG_CODE_REPO_ROOT)" KG_CODE_PATH_PREFIX="$(KG_SRC_DIR)" python3 scripts/kg/enrich_embeddings.py

kg-update: ## Reindex KG code for a repo path and refresh scoped code↔doc links (set KG_SRC_DIR=<path>, optional KG_CODE_REPO_ROOT=<repo>)
	@if [ -z "$(strip $(KG_SRC_DIR))" ]; then \
		echo "KG_SRC_DIR is required, e.g. make kg-update KG_SRC_DIR=crates/litkg-core"; \
		exit 2; \
	fi
	KG_CODE_REPO_ROOT="$(KG_CODE_REPO_ROOT)" ./scripts/kg/index_code.sh "$(KG_SRC_DIR)"
	KG_CODE_REPO_ROOT="$(KG_CODE_REPO_ROOT)" KG_CODE_PATH_PREFIX="$(KG_SRC_DIR)" python3 scripts/kg/enrich_embeddings.py

kg-graphiti: ## Start the upstream Graphiti MCP server (set GRAPHITI_MODE=http|stdio)
	./scripts/kg/start_graphiti.sh "$(GRAPHITI_MODE)"

kg-smoke: ## Run static checks for the local KG helper surface
	docker compose -f infra/neo4j/docker-compose.yml config >/dev/null
	bash -n scripts/kg/up.sh
	bash -n scripts/kg/down.sh
	bash -n scripts/kg/index_code.sh
	bash -n scripts/kg/start_graphiti.sh
	bash -n scripts/kg/ingest_docs.sh
	$(PYTHON) -m py_compile scripts/kg/enrich_embeddings.py scripts/loc_stats.py

litkg-sync: ## Merge manifest and BibTeX into the normalized registry for a config
	$(CARGO) run -p litkg-cli -- sync-registry --config "$(LITKG_CONFIG)"

litkg-download: ## Download literature assets for a config
	$(CARGO) run -p litkg-cli -- download --config "$(LITKG_CONFIG)" $(LITKG_DOWNLOAD_ARGS)

litkg-parse: ## Parse downloaded papers into normalized structured records
	$(CARGO) run -p litkg-cli -- parse --config "$(LITKG_CONFIG)"

litkg-materialize: ## Materialize graph-oriented output for a config
	$(CARGO) run -p litkg-cli -- materialize --config "$(LITKG_CONFIG)"

litkg-rebuild-graph: ## Run the configured graph rebuild step for a config
	$(CARGO) run -p litkg-cli -- rebuild-graph --config "$(LITKG_CONFIG)"

litkg-pipeline: ## Run the full sync/download/parse/materialize pipeline for a config
	$(CARGO) run -p litkg-cli -- pipeline --config "$(LITKG_CONFIG)" $(LITKG_PIPELINE_ARGS)

litkg-export-neo4j: ## Emit the Neo4j export bundle for a config
	$(CARGO) run -p litkg-cli -- export-neo4j --config "$(LITKG_CONFIG)"

benchmark-validate: ## Validate benchmark catalog and sample results
	$(CARGO) run -p litkg-cli -- validate-benchmarks --catalog "$(BENCHMARK_CATALOG)" --results "$(BENCHMARK_RESULTS)"

benchmark-support: ## Inspect benchmark support and local readiness
	$(CARGO) run -p litkg-cli -- benchmark-support --catalog "$(BENCHMARK_CATALOG)" --integrations "$(BENCHMARK_INTEGRATIONS)" $(if $(strip $(BENCHMARK_RUN_PLAN)),--plan "$(BENCHMARK_RUN_PLAN)",)

benchmark-run: ## Run configured benchmark integrations and write a results bundle
	$(CARGO) run -p litkg-cli -- run-benchmarks --catalog "$(BENCHMARK_CATALOG)" --integrations "$(BENCHMARK_INTEGRATIONS)" $(if $(strip $(BENCHMARK_RUN_PLAN)),--plan "$(BENCHMARK_RUN_PLAN)",) --output "$(BENCHMARK_RESULTS_OUT)"

autoresearch-target: ## Render an autoresearch target from the benchmark catalog/results
	$(CARGO) run -p litkg-cli -- render-autoresearch-target --catalog "$(BENCHMARK_CATALOG)" --results "$(BENCHMARK_RESULTS)" --target-id "$(AUTORESEARCH_TARGET_ID)" --format "$(AUTORESEARCH_TARGET_FORMAT)"

inspect-graph: ## Launch the native graph inspector for one litkg config
	cargo run -p litkg-cli -- inspect-graph --config "$(GRAPH_CONFIG)"

autoresearch-issue: ## Create or preview a GitHub issue from an autoresearch target (set AUTORESEARCH_ISSUE_ARGS)
	cargo run -p litkg-cli -- sync-autoresearch-target-issue --catalog "$(BENCHMARK_CATALOG)" --results "$(BENCHMARK_RESULTS)" --target-id "$(AUTORESEARCH_TARGET_ID)" $(AUTORESEARCH_ISSUE_ARGS)

help: ## Show this help message
	@echo "Usage: make <target>"
	@awk 'BEGIN {FS = ":.*?## "}; /^[a-zA-Z0-9_.-]+:.*?## / {printf "  %-18s %s\n", $$1, $$2}' $(MAKEFILE_LIST)
