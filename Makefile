.DEFAULT_GOAL := help

AGENTS_DB ?= python3 .agents/scripts/agents_db.py
AGENTS_ARGS ?= rank
KG_SRC_DIR ?=
KG_DOC_PATHS ?=
BENCHMARK_CATALOG ?= examples/benchmarks/kg.toml
BENCHMARK_RESULTS ?= examples/benchmarks/sample-results.toml
AUTORESEARCH_TARGET_ID ?= kg_navigation_improvement
AUTORESEARCH_TARGET_FORMAT ?= markdown
GRAPH_CONFIG ?= examples/prml-vslam.toml

.PHONY: help fmt test lint agents-db
.PHONY: kg-up kg-down kg-index kg-ingest-docs kg-enrich kg-update
.PHONY: benchmark-validate autoresearch-target inspect-graph

fmt: ## Run rustfmt across the workspace
	cargo fmt --all

test: ## Run the Rust test suite
	cargo test

lint: ## Run formatting checks and tests
	cargo fmt --all --check
	cargo test

agents-db: ## Show the local .agents backlog (example: make agents-db AGENTS_ARGS="rank --kind todos")
	$(AGENTS_DB) $(AGENTS_ARGS)

kg-up: ## Start the local Neo4j KG stack
	./scripts/kg/up.sh

kg-down: ## Stop the local Neo4j KG stack
	./scripts/kg/down.sh

kg-index: ## Reindex code graph for a repo path (set KG_SRC_DIR=<path>, default .)
	./scripts/kg/index_code.sh "$(if $(strip $(KG_SRC_DIR)),$(KG_SRC_DIR),.)"

kg-ingest-docs: ## Ingest repo docs into Graphiti (optional KG_DOC_PATHS="path1 path2")
	./scripts/kg/ingest_docs.sh $(KG_DOC_PATHS)

kg-enrich: ## Refresh embeddings and code↔doc links (optional KG_SRC_DIR=<path> scopes code refresh)
	KG_CODE_PATH_PREFIX="$(KG_SRC_DIR)" python3 scripts/kg/enrich_embeddings.py

kg-update: ## Reindex KG code for a repo path and refresh scoped code↔doc links (set KG_SRC_DIR=<path>)
	@if [ -z "$(strip $(KG_SRC_DIR))" ]; then \
		echo "KG_SRC_DIR is required, e.g. make kg-update KG_SRC_DIR=crates/litkg-core"; \
		exit 2; \
	fi
	./scripts/kg/index_code.sh "$(KG_SRC_DIR)"
	KG_CODE_PATH_PREFIX="$(KG_SRC_DIR)" python3 scripts/kg/enrich_embeddings.py

benchmark-validate: ## Validate benchmark catalog and sample results
	cargo run -p litkg-cli -- validate-benchmarks --catalog "$(BENCHMARK_CATALOG)" --results "$(BENCHMARK_RESULTS)"

autoresearch-target: ## Render an autoresearch target from the benchmark catalog/results
	cargo run -p litkg-cli -- render-autoresearch-target --catalog "$(BENCHMARK_CATALOG)" --results "$(BENCHMARK_RESULTS)" --target-id "$(AUTORESEARCH_TARGET_ID)" --format "$(AUTORESEARCH_TARGET_FORMAT)"

inspect-graph: ## Launch the native graph inspector for one litkg config
	cargo run -p litkg-cli -- inspect-graph --config "$(GRAPH_CONFIG)"

help: ## Show this help message
	@echo "Usage: make <target>"
	@awk 'BEGIN {FS = ":.*?## "}; /^[a-zA-Z0-9_.-]+:.*?## / {printf "  %-12s %s\n", $$1, $$2}' $(MAKEFILE_LIST)
