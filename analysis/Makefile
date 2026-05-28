REPO_ROOT := $(abspath $(CURDIR)/..)
PARSER_OUTPUT_DIR=$(REPO_ROOT)/output/parser/dataset
LINEAGE_OUTPUT_DIR=$(REPO_ROOT)/output/parser/lineage
ANONYMIZER_OUTPUT_DIR=$(REPO_ROOT)/output/anonymizer
ANALYSIS_OUTPUT_DIR=$(REPO_ROOT)/output/analysis

# Env variables for analysis
LISTS_OF_INTEREST ?=
# see list of scripts in analysis/src/main.py
# leave empty to run default scripts, use a specific script name
# or "all" to run non default too (require passing LISTS_OF_INTEREST)
ANALYSIS_SCRIPT ?=

# Include shared container runtime detection
include ../containers.mk

# Define the Docker image to use for building
ANALYSIS_IMAGE := mlh_analysis
IMAGE_EXISTS := $(shell $(CONTAINER) image inspect $(ANALYSIS_IMAGE) >/dev/null 2>&1 && echo yes || echo no)


.PHONY: test
test:
	@if command -v nox >/dev/null 2>&1; then \
		echo "==> Found Python Testing toolchain, running natively..."; \
		nox; \
	else \
		echo "==> Python Testing toolchain not found, running with $(CONTAINER) (Image: $(ANALYSIS_IMAGE))..."; \
		$(CONTAINER) run --rm \
			--network=host \
			-v $(CURDIR):/app \
			-w /app \
			$(ANALYSIS_IMAGE) \
			uvx nox; \
	fi

.PHONY: create-output-dir
create-output-dir:
	@mkdir -p $(ANALYSIS_OUTPUT_DIR)

.PHONY: run
run: create-output-dir
	@found=0; \
	if [ -d "../output/parser/dataset" ] && [ -n "$$(ls -A ../output/parser/dataset 2>/dev/null)" ]; then found=1; fi; \
	if [ -d "../output/parser/lineage" ] && [ -n "$$(ls -A ../output/parser/lineage 2>/dev/null)" ]; then found=1; fi; \
	if [ -d "../output/anonymizer/dataset" ] && [ -n "$$(ls -A ../output/anonymizer/dataset 2>/dev/null)" ]; then found=1; fi; \
	if [ $$found -eq 0 ]; then \
		echo "==> Error: No input data found."; \
		echo "==> Expected data in at least one of:"; \
		echo "==>   ../output/parser/dataset"; \
		echo "==>   ../output/parser/lineage"; \
		echo "==>   ../output/anonymizer/dataset/"; \
		echo "==> Please run the parser or anonymizer first to generate input data."; \
		exit 1; \
	fi; \
	mkdir -p ../output/parser/dataset ../output/parser/lineage ../output/anonymizer/dataset; \
	if command -v uv >/dev/null 2>&1; then \
		echo "==> Found uv toolchain, running natively..."; \
		INPUT_DIR="../output/parser/dataset,../output/parser/lineage,../output/anonymizer/" OUTPUT_DIR="../output/analysis/" LISTS_OF_INTEREST="$(LISTS_OF_INTEREST)" ANALYSIS_SCRIPT="$(ANALYSIS_SCRIPT)" uv run src/main.py; \
	else \
		echo "==> uv toolchain not found, running with $(CONTAINER) (Image: $(ANALYSIS_IMAGE))..."; \
		if [ "$(IMAGE_EXISTS)" = "no" ]; then \
			echo "==> Image $(ANALYSIS_IMAGE) not found, building..."; \
			$(MAKE) rebuild; \
		fi; \
		$(CONTAINER) run --rm -it \
			--network=host \
			-e INPUT_DIR=/data/dataset,/data/lineage,/data/anonymized \
			-e OUTPUT_DIR=/output \
			-e LISTS_OF_INTEREST="$(LISTS_OF_INTEREST)" \
			-e ANALYSIS_SCRIPT="$(ANALYSIS_SCRIPT)" \
			-e GIT_COMMIT=$${GIT_COMMIT:-unknown} \
			-e GIT_DATE=$${GIT_DATE:-unknown} \
			-e MPLCONFIGDIR=/tmp \
			-v $(PARSER_OUTPUT_DIR):/data/dataset \
			-v $(LINEAGE_OUTPUT_DIR):/data/lineage \
			-v $(ANONYMIZER_OUTPUT_DIR):/data/anonymized \
			-v $(ANALYSIS_OUTPUT_DIR):/output \
			-v $(CURDIR):/app \
			-w /app \
			$(ANALYSIS_IMAGE) \
			uv run src/main.py; \
	fi


.PHONY: rebuild
rebuild:
	$(CONTAINER) build --build-arg USER_ID=$(shell id -u) --build-arg GROUP_ID=$(shell id -g) -t $(ANALYSIS_IMAGE) -f Containerfile .

.PHONY: clean
clean:
	@echo "==> Cleaning up anonymizer artifacts..."
	@rm -rf .venv .cache
	@rm -rf .nox
	@rm -rf .ruff_cache
	@rm -rf .pytest_cache
	@rm -rf __pycache__
	@rm -rf **/__pycache__
