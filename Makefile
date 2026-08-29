# GitHub Ranked
#
# Run `make` on its own for the target list.

SHELL := /usr/bin/env bash
.SHELLFLAGS := -eu -o pipefail -c
.DEFAULT_GOAL := help

# Ports live in the 10k range. 10090 rather than 10080 because browsers refuse
# to connect to 10080 (ERR_UNSAFE_PORT) — the only blocked port in that range.
PORT ?= 10090
WEB_PORT ?= 10173

CACHE_PATH ?= ./data/cache.db
WEB_ROOT ?= ./web/dist
RUST_LOG ?= info,github_ranked=debug

BIN := ./target/release/github-ranked
WASM_OUT := web/src/wasm/github_ranked_bg.wasm

# A GitHub credential, resolved at recipe time rather than parse time so it
# never lands in a make variable, an echoed command or an error message.
#
# `gh auth token` first, then GITHUB_TOKEN from the environment.
define resolve_token
	token="$$(gh auth token 2>/dev/null || true)"; \
	token_source="gh CLI"; \
	if [ -z "$$token" ]; then token="$${GITHUB_TOKEN:-}"; token_source="GITHUB_TOKEN"; fi; \
	if [ -z "$$token" ]; then \
		echo "error: no GitHub credential found." >&2; \
		echo "  Either run:  gh auth login" >&2; \
		echo "  or export:   GITHUB_TOKEN=ghp_..." >&2; \
		exit 1; \
	fi
endef

.PHONY: help
help: ## Show this help
	@echo "GitHub Ranked"
	@echo
	@grep -hE '^[a-zA-Z0-9_-]+:.*?## ' $(MAKEFILE_LIST) \
		| awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-16s\033[0m %s\n", $$1, $$2}'
	@echo
	@echo "  API on :$(PORT), dev frontend on :$(WEB_PORT)"

# --- running --------------------------------------------------------------

.PHONY: dev
dev: $(WASM_OUT) web/node_modules ## Run the API and the frontend dev server together
	@$(resolve_token); \
	echo "  api  http://localhost:$(PORT)"; \
	echo "  web  http://localhost:$(WEB_PORT)  <- open this one"; \
	echo; \
	trap 'kill 0' EXIT INT TERM; \
	GITHUB_TOKEN="$$token" PORT=$(PORT) CACHE_PATH=$(CACHE_PATH) \
		WEB_ROOT=$(WEB_ROOT) RUST_LOG=$(RUST_LOG) \
		cargo run --quiet -p github-ranked & \
	npm --prefix web run dev -- --port $(WEB_PORT) & \
	wait

.PHONY: serve
serve: build ## Build everything and run the release binary alone
	@$(resolve_token); \
	echo "  http://localhost:$(PORT)"; \
	GITHUB_TOKEN="$$token" PORT=$(PORT) CACHE_PATH=$(CACHE_PATH) \
		WEB_ROOT=$(WEB_ROOT) RUST_LOG=$(RUST_LOG) $(BIN)

.PHONY: api
api: ## Run only the API, against an already-built frontend
	@$(resolve_token); \
	GITHUB_TOKEN="$$token" PORT=$(PORT) CACHE_PATH=$(CACHE_PATH) \
		WEB_ROOT=$(WEB_ROOT) RUST_LOG=$(RUST_LOG) \
		cargo run -p github-ranked

.PHONY: token
token: ## Check that a GitHub credential can be resolved (prints nothing secret)
	@$(resolve_token); \
	echo "credential source: $$token_source"; \
	echo "length: $${#token} characters"

# --- building -------------------------------------------------------------

.PHONY: build
build: wasm web ## Release build of the server and frontend
	cargo build --release -p github-ranked

$(WASM_OUT): $(shell find crates/core/src crates/wasm/src -name '*.rs' 2>/dev/null)
	./crates/wasm/build.sh

.PHONY: wasm
wasm: ## Build the WebAssembly bundle the frontend imports
	./crates/wasm/build.sh

web/node_modules: web/package.json web/package-lock.json
	npm --prefix web ci
	@touch web/node_modules

.PHONY: web
web: web/node_modules $(WASM_OUT) ## Build the frontend into web/dist
	npm --prefix web run build

# --- testing --------------------------------------------------------------

.PHONY: test
test: test-rust test-e2e ## Everything

.PHONY: test-rust
test-rust: ## Rust tests: golden fixtures, properties, HTTP
	cargo test --workspace

.PHONY: test-features
test-features: ## Verify every feature combination builds and passes
	cargo test -p github-ranked --no-default-features
	cargo test -p github-ranked
	cargo test -p github-ranked --features pat-in-production

e2e/node_modules: e2e/package.json
	npm --prefix e2e ci
	@touch e2e/node_modules

.PHONY: test-e2e
test-e2e: $(BIN) web e2e/node_modules ## Playwright, against the real binary
	npm --prefix e2e exec -- playwright test

$(BIN):
	cargo build --release -p github-ranked

.PHONY: bench
bench: ## Criterion benchmarks (see docs/performance.md for baselines)
	cargo bench -p github-ranked-core -- --quick

.PHONY: check
check: ## Format, clippy and typecheck, without building artefacts
	cargo fmt --all --check
	cargo clippy --workspace --all-targets -- -D warnings
	npm --prefix web exec -- tsc --noEmit -p web/tsconfig.app.json

.PHONY: fmt
fmt: ## Format Rust and frontend sources
	cargo fmt --all
	npm --prefix web exec -- prettier --write "web/src/**/*.{ts,tsx,css}"

# --- container and cluster ------------------------------------------------

.PHONY: docker
docker: ## Build the stock image (GitHub App auth only in production)
	docker build -t github-ranked:latest .

.PHONY: docker-selfhost
docker-selfhost: ## Build an image that accepts a PAT in production
	docker build --build-arg CARGO_FEATURES=pat-in-production \
		-t github-ranked:selfhost .

.PHONY: docker-run
docker-run: docker-selfhost ## Run the self-host image locally
	@$(resolve_token); \
	echo "  http://localhost:$(PORT)"; \
	docker run --rm -p $(PORT):10090 -e GITHUB_TOKEN="$$token" github-ranked:selfhost

.PHONY: k8s-validate
k8s-validate: ## Render and validate the Kubernetes manifests
	kubectl kustomize deploy/k8s | kubectl apply --dry-run=client -f -

# --- maintenance ----------------------------------------------------------

.PHONY: fixtures
fixtures: ## Regenerate golden fixtures from the original TypeScript engine
	@echo "This re-pins the ranking algorithm. Only correct when deliberately"
	@echo "changing it — every existing user's rank moves."
	./tools/oracle/regenerate.sh

.PHONY: glyphs
glyphs: ## Regenerate the embedded glyph table
	./tools/fontgen/regenerate.sh

.PHONY: preview
preview: ## Render a tier x theme contact sheet to look at the cards
	cd tools/preview && cargo run --release
	@echo "wrote tools/preview/sheet.png"

.PHONY: clean
clean: ## Remove build artefacts
	cargo clean
	rm -rf web/dist web/src/wasm e2e/test-results e2e/playwright-report
	rm -rf tools/preview/target tools/preview/sheet.png

.PHONY: clean-cache
clean-cache: ## Drop the local rank cache
	rm -f $(CACHE_PATH) $(CACHE_PATH)-wal $(CACHE_PATH)-shm
