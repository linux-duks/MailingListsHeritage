# Include shared container runtime detection
include ../containers.mk

# Define the name of your binary
BINARY_NAME = mlh_parser

# CURDIR is the directory where make is running (after any -C changes)
# Repository root is parent of current directory
REPO_ROOT := $(abspath $(CURDIR)/..)
# When called with "make -C mlh_parser", CURDIR = mlh_parser/
# TARGET_PATH is the absolute path to the binary
TARGET_PATH := $(REPO_ROOT)/target/release/$(BINARY_NAME)


# Define the Docker image to use for building
RUST_DOCKER_IMAGE = docker.io/rust:1.95-slim


# ------------------------------------------------------------------------------
# BUILD LOGIC
# ------------------------------------------------------------------------------
.PHONY: build
build:
	@if command -v cargo >/dev/null 2>&1; then \
		echo "==> Found Rust toolchain, building natively..."; \
		cargo build --release; \
	else \
		echo "==> Rust toolchain not found, building with Docker (Image: $(RUST_DOCKER_IMAGE))..."; \
		$(CONTAINER) run --rm \
			-it -u $(shell id -u):$(shell id -g) \
			--network=host \
			-v ../:/usr/src/app:z \
			-w /usr/src/app \
			$(RUST_DOCKER_IMAGE) \
			cargo build --release; \
	fi

.PHONY: rebuild
rebuild: build

.PHONY: create-output-dir
create-output-dir:
	@mkdir -p $(REPO_ROOT)/output/parser

.PHONY: run
run: create-output-dir
	@if [ ! -f $(TARGET_PATH) ]; then \
		echo "==> Binary not found. Building first..."; \
		$(MAKE) build; \
	fi
	@echo "==> Running application from repository root..."
	@cd $(REPO_ROOT) && $(TARGET_PATH)


.PHONY: debug
debug:
	@if command -v cargo >/dev/null 2>&1; then \
		echo "==> Found Rust toolchain, building natively..."; \
		cd $(REPO_ROOT) && RUST_LOG=debug cargo run --bin $(BINARY_NAME); \
	else \
		echo "==> Rust toolchain not found, building with Docker (Image: $(RUST_DOCKER_IMAGE))..."; \
		$(CONTAINER) run --rm \
			-it -u $(shell id -u):$(shell id -g) \
			-e RUST_LOG=debug \
			--network=host \
			-v $(REPO_ROOT):/usr/src/app:z \
			-w /usr/src/app \
			$(RUST_DOCKER_IMAGE) \
			cargo run --bin $(BINARY_NAME); \
	fi


.PHONY: test
test:
	@if command -v cargo >/dev/null 2>&1; then \
		echo "==> Found Rust toolchain, running tests natively..."; \
		cargo test; \
	else \
		echo "==> Rust toolchain not found, running tests with Docker (Image: $(RUST_DOCKER_IMAGE))..."; \
		$(CONTAINER) run --rm \
			-it -u $(shell id -u):$(shell id -g) \
			--network=host \
			-v $(REPO_ROOT):/usr/src/app:z \
			-w /usr/src/app \
			$(RUST_DOCKER_IMAGE) \
			cargo test; \
	fi

.PHONY: doc
doc:
	@if command -v cargo >/dev/null 2>&1; then \
		echo "==> Found Rust toolchain, generating docs natively..."; \
		cargo doc --document-private-items --open; \
	else \
		echo "==> Rust toolchain not found, generating docs with Docker (Image: $(RUST_DOCKER_IMAGE))..."; \
		$(CONTAINER) run --rm \
			-it -u $(shell id -u):$(shell id -g) \
			--network=host \
			-v $(REPO_ROOT):/usr/src/app:z \
			-w /usr/src/app \
			$(RUST_DOCKER_IMAGE) \
			cargo doc --document-private-items --open; \
	fi

.PHONY: clean
clean:
	@echo "==> Cleaning up parser build artifacts..."
	@if command -v cargo >/dev/null 2>&1; then \
		cargo clean; \
	else \
		echo "==> Rust toolchain not found, removing target directory directly..."; \
		rm -rf $(REPO_ROOT)/target; \
	fi
	@rm -f ./$(BINARY_NAME).out


