# Include shared container runtime detection
include ../containers.mk

DEV_IMAGE = "../.devcontainer/Containerfile"
REPO_ROOT := $(abspath $(CURDIR)/..)
RUST_DOCKER_IMAGE = docker.io/rust:1.95-slim

.PHONY: build-check-git build-check-nntp

build-check-git: BIN = check_git
build-check-git: _build-rs

build-check-nntp: BIN = check_nntp
build-check-nntp: _build-rs

_build-rs:
	@if command -v cargo >/dev/null 2>&1; then \
		echo "==> Found Rust toolchain, building natively..."; \
		cargo build --bin $(BIN) --release; \
	else \
		echo "==> Rust toolchain not found, building with Docker (Image: $(RUST_DOCKER_IMAGE))..."; \
		$(CONTAINER) run --rm -t \
			--network=host \
			-v $(REPO_ROOT):/usr/src/app:z \
			-w /usr/src/app \
			$(RUST_DOCKER_IMAGE) \
			cargo build --bin $(BIN) --release; \
	fi

PI_IMAGE ?= mlh-create-example-pi

USERN_MAP :=
ifeq ($(CONTAINER),podman)
USERN_MAP := --userns=keep-id
endif

.PHONY: create-example-pi
create-example-pi:
	@echo "==> Building container image for example public-inbox..."
	$(CONTAINER) build -t $(PI_IMAGE) --build-arg USER_ID=$(shell id -u) --build-arg GROUP_ID=$(shell id -g) -f ./create_example_pi/Containerfile $(REPO_ROOT)
	@echo "==> Creating output directory..."
	rm -rf $(REPO_ROOT)/data
	mkdir -p $(REPO_ROOT)/data
	@echo "==> Generating example public-inbox v2 repository..."
	$(CONTAINER) run --rm -t \
		$(USERN_MAP) \
		-w /data \
		-v $(REPO_ROOT)/data:/data:z \
		-v $(REPO_ROOT)/mlh_parser/tests/fixtures:/fixtures:ro \
		$(PI_IMAGE) \
		/fixtures /data
	@echo "==> Example public-inbox created at $(REPO_ROOT)/data/"

.PHONY: clean
clean:
	@echo "==> Cleaning up scripts artifacts..."
	@rm -rf .venv
	@rm -rf __pycache__
	@if command -v cargo >/dev/null 2>&1; then \
		cargo clean; \
	else \
		echo "==> Rust toolchain not found, removing target directory directly..."; \
		rm -rf $(REPO_ROOT)/target; \
	fi


.PHONY: peek
peek:
	@# 1. Validate PEEK_PATH
	@if [ -z "$(PEEK_PATH)" ]; then \
		echo "ERROR: PEEK_PATH is not defined. Usage: make peek PEEK_PATH=/path/to/files"; \
		exit 1; \
	fi
	@if command -v uv >/dev/null 2>&1; then \
		echo "==> Found python-uv toolchain, running natively..."; \
		uv run --project ./peek_parquet ./peek_parquet/peek_files.py $(PEEK_PATH) ;\
	else \
		echo "==> uv toolchain not found, running in container..."; \
		$(CONTAINER) run --rm -t \
			-v ./:/usr/src/app:z \
			-v $(PEEK_PATH):/target_path:z \
			-w /usr/src/app \
			$$($(CONTAINER) build -q --build-arg USER_ID=$(shell id -u) --build-arg GROUP_ID=$(shell id -g) -f $(DEV_IMAGE) . ) \
			bash -c "uv run --project ./peek_parquet ./peek_parquet/peek_files.py --dir /target_path" ; \
	fi
