# Variables for the analysis script
LISTS_OF_INTEREST ?=
ANALYSIS_SCRIPT ?=
N_PROC ?=
MAX_TOTAL_THREADS ?=
COMPRESSION_LEVEL ?=
export LISTS_OF_INTEREST
export ANALYSIS_SCRIPT
export N_PROC
export MAX_TOTAL_THREADS
export COMPRESSION_LEVEL
# end of variables for the analysis script

# By default, 'make' will run the 'all' target
.PHONY: all
all: create-output-dirs rebuild run


# create output directories for all pipeline steps
.PHONY: create-output-dirs
create-output-dirs:
	$(MAKE) -C mlh_archiver create-output-dir
	$(MAKE) -C mlh_parser create-output-dir
	$(MAKE) -C anonymizer create-output-dir
	$(MAKE) -C analysis create-output-dir


# run all targets in order, with timing
.PHONY: run
run:
	@echo "==> Starting sequential pipeline..."; \
	total=0; \
	\
	start=$$(date +%s); \
	$(MAKE) -C mlh_archiver run; \
	end=$$(date +%s); \
	archiver_dur=$$((end - start)); \
	total=$$((total + archiver_dur)); \
	\
	start=$$(date +%s); \
	$(MAKE) -C mlh_parser run; \
	end=$$(date +%s); \
	parser_dur=$$((end - start)); \
	total=$$((total + parser_dur)); \
	\
	start=$$(date +%s); \
	$(MAKE) -C anonymizer run; \
	end=$$(date +%s); \
	anonymizer_dur=$$((end - start)); \
	total=$$((total + anonymizer_dur)); \
	\
	start=$$(date +%s); \
	$(MAKE) -C analysis run; \
	end=$$(date +%s); \
	analysis_dur=$$((end - start)); \
	total=$$((total + analysis_dur)); \
	\
	echo "=============================="; \
	echo "  Pipeline timing summary:"; \
	echo "  archiver:    $${archiver_dur}s"; \
	echo "  parser:      $${parser_dur}s"; \
	echo "  anonymizer:  $${anonymizer_dur}s"; \
	echo "  analysis:    $${analysis_dur}s"; \
	echo "  ----------------------------"; \
	echo "  Total:       $${total}s"; \
	echo "=============================="

# ------------------------------------------------------------------------------
# APPLICATION TARGETS
# ------------------------------------------------------------------------------

.PHONY: build-archiver
build-archiver:
	$(MAKE) -C mlh_archiver build

.PHONY: run-archiver
run-archiver:
	$(MAKE) -C mlh_archiver run

.PHONY: run-parser
run-parser:
	$(MAKE) -C mlh_parser run

.PHONY: run-anonymizer
run-anonymizer:
	$(MAKE) -C anonymizer run

.PHONY: run-analysis
run-analysis:
	$(MAKE) -C analysis run

# scripts
.PHONY: build-check-git
build-check-git:
	$(MAKE) -C scripts build-check-git

.PHONY: build-check-nntp
build-check-nntp:
	$(MAKE) -C scripts build-check-nntp

# ------------------------------------------------------------------------------
# REBUILD TARGETS
# ------------------------------------------------------------------------------

.PHONY: rebuild
rebuild: rebuild-parser rebuild-anonymizer rebuild-analysis build-archiver

.PHONY: rebuild-archiver
rebuild-archiver: build-archiver

.PHONY: rebuild-parser
rebuild-parser:
	$(MAKE) -C mlh_parser rebuild

.PHONY: rebuild-anonymizer
rebuild-anonymizer:
	$(MAKE) -C anonymizer rebuild

.PHONY: rebuild-analysis
rebuild-analysis:
	$(MAKE) -C analysis rebuild

# ------------------------------------------------------------------------------
# DEBUG TARGETS
# ------------------------------------------------------------------------------

.PHONY: debug-archiver
debug-archiver:
	$(MAKE) -C mlh_archiver debug

.PHONY: debug-parser
debug-parser:
	$(MAKE) -C mlh_parser debug

.PHONY: debug-anonymizer
debug-anonymizer:
	$(MAKE) -C anonymizer debug

# ------------------------------------------------------------------------------
# TEST TARGETS
# ------------------------------------------------------------------------------

.PHONY: test
test: test-archiver test-parser test-anonymizer

.PHONY: test-archiver
test-archiver:
	$(MAKE) -C mlh_archiver test

.PHONY: test-parser
test-parser:
	$(MAKE) -C mlh_parser test

.PHONY: test-anonymizer
test-anonymizer:
	$(MAKE) -C anonymizer test

# ------------------------------------------------------------------------------
# UTILITY TARGETS
# ------------------------------------------------------------------------------


.PHONY: doc
doc:
	cargo doc --open

.PHONY: clean
clean:
	@echo "==> Cleaning up build artifacts..."
	$(MAKE) -C mlh_parser clean
	$(MAKE) -C anonymizer clean
	$(MAKE) -C analysis clean
	$(MAKE) -C scripts clean
	$(MAKE) -C mlh_archiver clean


CONFIG_FILES := archiver_config.yaml anonymizer_config.yaml parser_config.yaml
DEMO_CONFIG_DIR := scripts/create_example_pi

.PHONY: check_config_files
check_config_files:
	@echo "==> Checking config files..."
	@missing=0; \
	for f in $(CONFIG_FILES); do \
		if [ -f "$$f" ]; then \
			echo "  [OK] $$f"; \
		else \
			echo "  [MISSING] $$f"; \
			missing=1; \
		fi; \
	done; \
	if [ $$missing -eq 1 ]; then \
		echo ""; \
		echo "Some config files are missing. Run 'make create-default-configs' to create them."; \
	fi

.PHONY: create-default-configs
create-default-configs:
	@echo "==> Creating default config files from examples..."
	@for f in $(CONFIG_FILES); do \
		if [ -f "$$f" ]; then \
			echo "  $$f already exists, skipping."; \
		elif [ -f "example_$$f" ]; then \
			cp "example_$$f" "$$f"; \
			echo "  Created $$f from example_$$f"; \
		else \
			echo "  No example found for $$f"; \
		fi; \
	done
	@echo ""
	@echo "==> IMPORTANT: edit archiver_config.yaml before running the pipeline."
	@echo "    It requires source server details that must be configured manually."

.PHONY: setup-demo
setup-demo:
	@echo "==> Setting up demo environment..."
	@existing=""; \
	for f in $(CONFIG_FILES); do \
		if [ -f "$$f" ]; then \
			existing="$$existing $$f"; \
		fi; \
	done; \
	if [ -n "$$existing" ]; then \
		echo ""; \
		echo "WARNING: The following config files already exist:$$existing"; \
		echo "Running setup-demo will replace them."; \
		echo ""; \
		read -p "Create backups and continue? [Y/n] " answer; \
		case $$answer in \
			[Nn]*) echo "Cancelled."; exit 0;; \
			*) \
				for f in $$existing; do \
					mv "$$f" "backup.$$f"; \
					echo "  Backed up $$f -> backup.$$f"; \
				done;; \
		esac; \
	fi
	@echo "==> Copying demo config files..."
	@for f in $(CONFIG_FILES); do \
		demo="$(DEMO_CONFIG_DIR)/demo_$$f"; \
		if [ -f "$$demo" ]; then \
			cp "$$demo" "$$f"; \
			echo "  $$f"; \
		fi; \
	done
	@$(MAKE) create-example-pi
	@echo ""
	@echo "=============================="
	@echo "  Demo environment is ready."
	@echo "  Run 'make run' to start the pipeline,"
	@echo "  or run components individually."
	@echo "=============================="

.PHONY: create-example-pi
create-example-pi:
	$(MAKE) -C scripts create-example-pi

.PHONY: peek
peek:
	$(MAKE) -C scripts peek
