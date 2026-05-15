REPO_ROOT := $(abspath $(CURDIR)/..)
PARSER_OUTPUT_DIR=$(REPO_ROOT)/output/parser/dataset
ANONYMIZER_OUTPUT_DIR=$(REPO_ROOT)/output/anonymizer
ANALYSIS_OUTPUT_DIR=$(REPO_ROOT)/output/analysis

# Env variables for analysis
LISTS_OF_INTEREST ?=
RUN_VALIDATION_SCRIPTS ?=

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
		$(CONTAINER) run --rm \
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
	@mkdir -p $(ANALYSIS_OUTPUT_DIR)

.PHONY: run
run: create-output-dir
	@if [ ! -d "../output/parser/dataset" ] || [ -z "$$(ls ../output/parser/dataset 2>/dev/null)" ]; then \
		echo "==> Error: '../output/parser/dataset' directory is missing or empty."; \
		echo "==> Please run the parser first to generate the dataset."; \
		exit 1; \
	else \
		if command -v uv >/dev/null 2>&1; then \
			echo "==> Found uv toolchain, running natively..."; \
			INPUT_DIR="../output/parser/dataset,../output/anonymizer/" OUTPUT_DIR="../output/analysis/" LISTS_OF_INTEREST="$(LISTS_OF_INTEREST)" RUN_VALIDATION_SCRIPTS="$(RUN_VALIDATION_SCRIPTS)" uv run src/main.py; \
		else \
			echo "==> uv toolchain not found, running with $(CONTAINER) (Image: $(PYTHON_DOCKER_IMAGE))..."; \
			$(CONTAINER) run --rm -it \
				--network=host \
				-e INPUT_DIR=/data/dataset,/data/anonymized \
				-e OUTPUT_DIR=/output \
				-e LISTS_OF_INTEREST="$(LISTS_OF_INTEREST)" \
				-e RUN_VALIDATION_SCRIPTS="$(RUN_VALIDATION_SCRIPTS)" \
				-e GIT_COMMIT=$${GIT_COMMIT:-unknown} \
				-e GIT_DATE=$${GIT_DATE:-unknown} \
				-e UV_CACHE_DIR=/tmp/.cache/uv \
				-e UV_PROJECT_ENVIRONMENT=/tmp/.venv \
				-v $(PARSER_OUTPUT_DIR):/data/dataset \
				-v $(ANONYMIZER_OUTPUT_DIR):/data/anonymized \
				-v $(ANALYSIS_OUTPUT_DIR):/output \
				-v $(CURDIR):/usr/src/app \
				-w /usr/src/app \
				$(PYTHON_DOCKER_IMAGE) \
				bash -c "uv sync --locked && uv run src/main.py"; \
		fi \
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
