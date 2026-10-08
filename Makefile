.PHONY: help all clean test test-ui build release coverage lint github-actions-lint fmt check-fmt \
	markdownlint nixie spelling test-workflow-contracts install-build-tools \
	install-cranelift check-build-tools check-rust-toolchain check-coverage-tools
.NOTPARALLEL:


TARGET ?= catnap

USER_WHITAKER := $(HOME)/.local/bin/whitaker
USER_BIN_PATH := $(HOME)/.cargo/bin:$(HOME)/.local/bin:$(HOME)/.bun/bin
CARGO ?= cargo
BUILD_JOBS ?=
RUST_FLAGS ?=
RUST_FLAGS := -D warnings $(RUST_FLAGS)
RUSTDOC_FLAGS ?=
RUSTDOC_FLAGS := -D warnings $(RUSTDOC_FLAGS)
CARGO_FLAGS ?= --all-targets --all-features
CLIPPY_FLAGS ?= $(CARGO_FLAGS) -- $(RUST_FLAGS)
TEST_FLAGS ?= $(CARGO_FLAGS)
TEST_CMD := $(if $(shell $(CARGO) nextest --version 2>/dev/null),nextest run,test)
COVERAGE_LINKER_FLAGS ?= -fuse-ld=lld
COVERAGE_RUST_FLAGS ?= $(RUST_FLAGS) -C link-arg=$(COVERAGE_LINKER_FLAGS)
MARKDOWNLINT_CLI2_VERSION ?= 0.23.3
MDLINT ?= bunx --silent markdownlint-cli2@$(MARKDOWNLINT_CLI2_VERSION)
# `make fmt` and `make check-fmt` call mdtablefix directly. `--git` selects the
# Markdown files Git tracks and `--include-untracked` adds the untracked files
# Git does not ignore, so a new document is formatted before it is staged.
# Both modes need mdtablefix 0.6.0 or later; CI pins the version at the
# install-mdtablefix step.
MDTABLEFIX ?= mdtablefix
MDTABLEFIX_SELECT = --git --include-untracked
MDTABLEFIX_RULES = --wrap --renumber --breaks --ellipsis --fences
NIXIE ?= nixie
YAMLLINT ?= yamllint
ACTIONLINT ?= actionlint
WHITAKER ?= $(or $(shell command -v whitaker 2>/dev/null),$(wildcard $(USER_WHITAKER)),whitaker)
UV ?= uv
UV_ENV = UV_CACHE_DIR=.uv-cache UV_TOOL_DIR=.uv-tools

# Local builds resolve the checksum-pinned mold before any distribution copy.
# setup-rust supplies CI's pinned mold on GITHUB_PATH; leave that PATH alone.
BUILD_TOOLS_PREFIX ?= $(HOME)/.local
BUILD_TOOLS_PATH := $(if $(filter true,$(GITHUB_ACTIONS)),$(PATH),$(BUILD_TOOLS_PREFIX)/bin:$(PATH))
BUILD_TOOL_TARGETS := \
	install-build-tools check-build-tools check-rust-toolchain check-coverage-tools build test test-ui lint \
	typecheck fmt check-fmt coverage target/debug/$(TARGET)
$(BUILD_TOOL_TARGETS): export BUILD_TOOLS_PREFIX := $(BUILD_TOOLS_PREFIX)
$(BUILD_TOOL_TARGETS): export PATH := $(BUILD_TOOLS_PATH)

# The CV-005 CodeScene contracts live in shared-actions and run from a full
# commit, so a fix is a pin bump. `.github/cv005.toml` holds this repository's
# only parameters.
CV005_CONTRACTS_REF ?= 88977798a5c3bae1549afb99642529488c665276
CV005_CONTRACTS = $(UV_ENV) $(UV) tool run --python 3.13 \
	--from 'git+https://github.com/leynos/shared-actions@$(CV005_CONTRACTS_REF)\#subdirectory=packages/cv005-contracts' \
	cv005-contracts

YAMLLINT_VERSION ?= 1.38.0
TYPOS_CONFIG_BUILDER_VERSION ?= v0.1.3
TYPOS_CONFIG_BUILDER = $(UV_ENV) $(UV) tool run --python 3.14 --from \
	"git+https://github.com/leynos/typos-config-builder.git@$(TYPOS_CONFIG_BUILDER_VERSION)" \
	typos-config-builder

test-workflow-contracts: ## Check the CV-005 CodeScene workflow contracts
	$(CV005_CONTRACTS) check --repository .

# The development build standard (concordat rule `rust-build-defaults`):
# Cranelift and the parallel rustc frontend, plus mold on Linux. An assigned
# RUSTFLAGS replaces every `rustflags` table in .cargo/config.toml, so each
# recipe that sets it composes these onto any inherited value (CI's
# setup-rust exports one), except coverage, which stays on LLVM and the
# platform linker.
BUILD_HOST_OS := $(shell uname -s)
STANDARD_RUSTFLAGS := -Zthreads=8 -Zcodegen-backend=cranelift$(if $(filter Linux,$(BUILD_HOST_OS)), -Clink-arg=-fuse-ld=mold)

install-build-tools: ## Install the pinned mold linker and nightly toolchain
	@scripts/install-build-tools.sh

install-cranelift: ## Install the Cranelift component for the pinned nightly
	@scripts/install-build-tools.sh --cranelift-only

check-build-tools: ## Check the development build prerequisites
	@scripts/check-build-tools.sh

check-rust-toolchain: ## Check the pinned Rust toolchain and its components
	@scripts/check-build-tools.sh --toolchain-only

check-coverage-tools: ## Check the coverage toolchain, clang driver, and lld linker
	@scripts/check-build-tools.sh --coverage-only

build: check-build-tools target/debug/$(TARGET) ## Build debug binary
target/debug/$(TARGET): check-build-tools
release: target/release/$(TARGET) ## Build release binary

all: check-build-tools check-fmt lint test spelling test-workflow-contracts ## Perform a comprehensive check of code

clean: ## Remove build artefacts
	$(CARGO) clean

test: check-build-tools ## Run tests with warnings treated as errors
	RUSTFLAGS="$${RUSTFLAGS:+$$RUSTFLAGS }$(RUST_FLAGS) $(STANDARD_RUSTFLAGS)" $(CARGO) $(TEST_CMD) $(TEST_FLAGS) $(BUILD_JOBS)
	$(if $(filter nextest run,$(TEST_CMD)),\
		RUSTFLAGS="$${RUSTFLAGS:+$$RUSTFLAGS }$(RUST_FLAGS) $(STANDARD_RUSTFLAGS)" \
		$(CARGO) test --workspace --doc --all-features,:)

test-ui: check-build-tools ## Run public error UI contract tests
	RUSTFLAGS="$${RUSTFLAGS:+$$RUSTFLAGS }$(RUST_FLAGS) $(STANDARD_RUSTFLAGS)" \
		$(CARGO) $(TEST_CMD) --test ui $(BUILD_JOBS)


target/%/$(TARGET): ## Build binary in debug or release mode
	$(if $(findstring release,$(@)),RUSTFLAGS="$${RUSTFLAGS-}",RUSTFLAGS="$${RUSTFLAGS:+$$RUSTFLAGS }$(STANDARD_RUSTFLAGS)") $(CARGO) build $(BUILD_JOBS) $(if $(findstring release,$(@)),--release) --bin $(TARGET)

coverage: check-coverage-tools ## Generate lcov coverage with lld for llvm-tools compatibility
	@echo "coverage linker flags: $(COVERAGE_LINKER_FLAGS)"
	CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=clang RUSTFLAGS="$(COVERAGE_RUST_FLAGS)" \
		CFLAGS="$(COVERAGE_LINKER_FLAGS)" LDFLAGS="$(COVERAGE_LINKER_FLAGS)" \
		$(CARGO) llvm-cov --lcov --output-path lcov.info $(TEST_FLAGS)

lint: check-build-tools ## Run Rust and GitHub Actions linters with warnings denied
	RUSTDOCFLAGS="$(RUSTDOC_FLAGS)" RUSTFLAGS="$${RUSTFLAGS:+$$RUSTFLAGS }$(STANDARD_RUSTFLAGS)" $(CARGO) doc --no-deps
	RUSTFLAGS="$${RUSTFLAGS:+$$RUSTFLAGS }$(STANDARD_RUSTFLAGS)" $(CARGO) clippy $(CLIPPY_FLAGS)
	@echo "Whitaker binary: $(WHITAKER)"
	PATH="$(USER_BIN_PATH):$(PATH)" RUSTFLAGS="$(RUST_FLAGS)" $(WHITAKER) --all -- $(CARGO_FLAGS)
	$(MAKE) github-actions-lint

github-actions-lint: ## Validate GitHub Actions workflows
	$(YAMLLINT) .github/workflows
	$(ACTIONLINT)

typecheck: check-build-tools ## Type-check without building
	RUSTFLAGS="$${RUSTFLAGS:+$$RUSTFLAGS }$(RUST_FLAGS) $(STANDARD_RUSTFLAGS)" $(CARGO) check $(CARGO_FLAGS)

fmt: check-rust-toolchain ## Format Rust and Markdown sources
	$(CARGO) fmt --all
	$(MDTABLEFIX) --in-place $(MDTABLEFIX_SELECT) $(MDTABLEFIX_RULES)
	$(MDLINT) --fix "**/*.md"

check-fmt: check-rust-toolchain ## Verify formatting
	$(CARGO) fmt --all -- --check
	$(MDTABLEFIX) --check $(MDTABLEFIX_SELECT) $(MDTABLEFIX_RULES)

markdownlint: spelling ## Lint Markdown files and enforce spelling
	find . -type f -name '*.md' -not -path './target/*' \
		-not -path './.uv-cache/*' -not -path './.uv-tools/*' -print0 | \
		xargs -0 $(MDLINT)

spelling: ## Enforce en-GB-oxendict spelling
	$(TYPOS_CONFIG_BUILDER) gate --repository .

nixie: ## Validate Mermaid diagrams
	$(NIXIE) --no-sandbox

help: ## Show available targets
	@grep -E '^[a-zA-Z_-]+:.*?##' $(MAKEFILE_LIST) | \
	awk 'BEGIN {FS=":"; printf "Available targets:\n"} {printf "  %-20s %s\n", $$1, $$2}'
