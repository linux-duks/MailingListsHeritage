REPO_ROOT := $(abspath $(CURDIR)/..)
PARSER_OUTPUT_DIR=$(REPO_ROOT)/output/parser/dataset
ANONYMIZER_OUTPUT_DIR=$(REPO_ROOT)/output/anonymizer

N_PROC ?=
MAX_TOTAL_THREADS ?=
COMPRESSION_LEVEL ?=
LISTS_OF_INTEREST ?=
export N_PROC
export MAX_TOTAL_THREADS
export COMPRESSION_LEVEL
export LISTS_OF_INTEREST

# Include shared container runtime detection
include ../containers.mk

# Define the Docker image to use for building
PYTHON_DOCKER_IMAGE="ghcr.io/astral-sh/uv:python3.14-trixie-slim"


.PHONY: test
test:
	@if command -v nox >/dev/null 2>&1; then \
		echo "==> Found Python Testing toolchain, running natively..."; \
		nox; \
	else \
		echo "==> Python Testing toolchain not found, running with $(CONTAINER) (Image: $(PYTHON_DOCKER_IMAGE))..."; \
		$(CONTAINER) run --rm -it \
			-u $$(id -u):$$(id -g) \
			--network=host \
			-e UV_CACHE_DIR=/tmp/.cache/uv \
			-e UV_PROJECT_ENVIRONMENT=/tmp/.venv \
			-v $(CURDIR):/usr/src/app \
			-w /usr/src/app \
			$(PYTHON_DOCKER_IMAGE) \
			bash -c "uv sync --locked && uvx nox"; \
	fi


.PHONY: create-output-dir
create-output-dir:
	@mkdir -p $(REPO_ROOT)/output/anonymizer

.PHONY: run
run: create-output-dir
	@if [ ! -d "../output/parser/dataset" ] || [ -z "$$(ls ../output/parser/dataset 2>/dev/null)" ]; then \
		echo "==> Error: '../output/parser/dataset' directory is missing or empty."; \
		echo "==> Please run the parser first to generate the dataset."; \
		exit 1; \
	else \
		if command -v uv >/dev/null 2>&1; then \
			echo "==> Found uv toolchain, running natively..."; \
			INPUT_DIR="../output/parser/dataset" OUTPUT_DIR="../output/anonymizer" N_PROC="$${N_PROC:-}" MAX_TOTAL_THREADS="$${MAX_TOTAL_THREADS:-}" COMPRESSION_LEVEL="$${COMPRESSION_LEVEL:-}" LISTS_OF_INTEREST="$${LISTS_OF_INTEREST:-}" uv run src/main.py; \
		else \
			echo "==> uv toolchain not found, running with $(CONTAINER) (Image: $(PYTHON_DOCKER_IMAGE))..."; \
			echo "==> Host UID:GID = $$(id -u):$$(id -g)"; \
			$(CONTAINER) run --rm -it \
				--network=host \
				-e INPUT_DIR=/data/dataset \
				-e OUTPUT_DIR=/data/output \
				-e GIT_COMMIT=$${GIT_COMMIT:-unknown} \
				-e GIT_DATE=$${GIT_DATE:-unknown} \
				-e N_PROC=$${N_PROC:-} \
				-e MAX_TOTAL_THREADS=$${MAX_TOTAL_THREADS:-} \
				-e COMPRESSION_LEVEL=$${COMPRESSION_LEVEL:-} \
				-e LISTS_OF_INTEREST=$${LISTS_OF_INTEREST:-} \
				-e UV_CACHE_DIR=/tmp/.cache/uv \
				-e UV_PROJECT_ENVIRONMENT=/tmp/.venv \
				-v $(PARSER_OUTPUT_DIR):/data/dataset \
				-v $(ANONYMIZER_OUTPUT_DIR):/data/output \
				-v $(CURDIR):/usr/src/app \
				-w /usr/src/app \
				$(PYTHON_DOCKER_IMAGE) \
				bash -c "echo '==> Container UID:GID = $$(id -u):$$(id -g)'; echo '==> /data/output perms:'; ls -ld /data/output; echo '==> /data/dataset perms:'; ls -ld /data/dataset; echo '==> /usr/src/app perms:'; ls -ld /usr/src/app; uv sync --locked && uv run src/main.py"; \
		fi \
	fi


.PHONY: debug
debug:
	@if command -v uv >/dev/null 2>&1; then \
		echo "==> Found uv toolchain, running natively..."; \
		INPUT_DIR="../output/parser/dataset" OUTPUT_DIR="../output/anonymizer" N_PROC="$${N_PROC:-}" MAX_TOTAL_THREADS="$${MAX_TOTAL_THREADS:-}" COMPRESSION_LEVEL="1" LISTS_OF_INTEREST="$${LISTS_OF_INTEREST:-}" uv run src/main.py; \
	else \
		echo "==> uv toolchain not found, running with $(CONTAINER) (Image: $(PYTHON_DOCKER_IMAGE))..."; \
		$(CONTAINER) run --rm  -it\
			-u $$(id -u):$$(id -g) \
			--network=host \
			-e DEBUG=true \
			-e INPUT_DIR=/data/dataset \
			-e OUTPUT_DIR=/data/output \
			-e N_PROC=$${N_PROC:-} \
			-e MAX_TOTAL_THREADS=$${MAX_TOTAL_THREADS:-} \
			-e COMPRESSION_LEVEL="1" \
			-e LISTS_OF_INTEREST=$${LISTS_OF_INTEREST:-} \
			-e UV_CACHE_DIR=/tmp/.cache/uv \
			-e UV_PROJECT_ENVIRONMENT=/tmp/.venv \
			-v $(PARSER_OUTPUT_DIR):/data/dataset \
			-v $(ANONYMIZER_OUTPUT_DIR):/data/output \
			-v $(CURDIR):/usr/src/app \
			-w /usr/src/app \
			$(PYTHON_DOCKER_IMAGE) \
			bash -c "uv sync --locked && uv run src/main.py"; \
	fi


.PHONY: rebuild
rebuild:
	$(CONTAINER) pull $(PYTHON_DOCKER_IMAGE)


.PHONY: clean
clean:
	@echo "==> Cleaning up anonymizer artifacts..."
	@rm -rf .venv .cache
	@rm -rf .nox
	@rm -rf .ruff_cache
	@rm -rf .pytest_cache
	@rm -rf __pycache__
	@rm -rf **/__pycache__
