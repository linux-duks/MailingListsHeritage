# Variables for the analysis script
LISTS_OF_INTEREST ?=
RUN_VALIDATION_SCRIPTS ?=
export LISTS_OF_INTEREST
export RUN_VALIDATION_SCRIPTS
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

.PHONY: peek
peek:
	$(MAKE) -C scripts peek
