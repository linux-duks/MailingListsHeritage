# Include shared container runtime detection
include ../containers.mk

DEV_IMAGE = "../.devcontainer/Containerfile"

.PHONY: clean
clean:
	@echo "==> Cleaning up scripts artifacts..."
	@rm -rf .venv
	@rm -rf __pycache__


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
		$(CONTAINER) run --rm \
			-it -u $(id -u):$(id -g) \
			-v ./:/usr/src/app:z \
			-v $(PEEK_PATH):/target_path:z \
			-w /usr/src/app \
			$$($(CONTAINER) build -q -f $(DEV_IMAGE) . ) \
			bash -c "uv run --project ./peek_parquet ./peek_parquet/peek_files.py --dir /target_path" ; \
	fi
