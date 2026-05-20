"""Configuration and environment variable handling for the anonymizer."""

import os
import multiprocessing
import math

from mlh_anonymizer.constants import N_PROC_DEFAULT_MAX

# ── Determine concurrency split ───────────────────────────────────
# Strategy: keep worker-process count low, give Polars a healthy
# share of threads per process for I/O and anative parallelism.
# Multiplier / N_PROC_DEFAULT_MAX control the balance.
# Both values are overridable via environment variables.

_cpu_count = multiprocessing.cpu_count()

_proc_multiplier_denominator = 12


def _parse_n_proc() -> int:
    """Parse N_PROC from environment variable.

    Returns:
        Number of processes to use
    """
    n_proc_env = os.getenv("N_PROC", "")
    if n_proc_env.isdecimal():
        return int(n_proc_env)
    return max(
        1, min(math.ceil(_cpu_count / _proc_multiplier_denominator), N_PROC_DEFAULT_MAX)
    )


# Set POLARS_MAX_THREADS in the environment *before* any polars import.
# Polars reads this env var at init time; if unset it defaults to all CPUs,
# which combined with multiprocessing workers exhausts OS thread limits.
_polars_threads_env = os.getenv("POLARS_MAX_THREADS", "")
if _polars_threads_env:
    os.environ["POLARS_MAX_THREADS"] = _polars_threads_env
else:
    _n_proc = _parse_n_proc()
    os.environ["POLARS_MAX_THREADS"] = str(max(4, _cpu_count // (_n_proc + 1)))


def _is_debug() -> bool:
    """Check if debug mode is enabled.

    Returns:
        True if DEBUG environment variable is set to "true"
    """
    return os.getenv("DEBUG", "false").lower() == "true"


# Runtime configuration
DEBUG: bool = _is_debug()
N_PROC: int = _parse_n_proc()

# Override N_PROC for debug mode
if DEBUG:
    N_PROC = 1
    print(f"Running in DEBUG mode. N_PROC {N_PROC}")

# List of specific mailing lists to parse (empty = parse all)
LISTS_TO_PARSE: list[str] = [
    item for item in os.getenv("LISTS_TO_PARSE", "").split(",") if item
]

# Directory paths (required at runtime, optional at import time for tests)
INPUT_DIR_PATH: str = os.getenv("INPUT_DIR", "")
OUTPUT_DIR_PATH: str = os.getenv("OUTPUT_DIR", "")
