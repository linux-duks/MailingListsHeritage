"""Configuration and environment variable handling for the anonymizer."""

import os
import multiprocessing
import math

"""
MAX_TOTAL_THREADS = total thread budget (workers + polars threads per worker).
  - Defaults to half the available CPU cores.
  - N_PROC env var: explicit override for worker process count.
  - POLARS_MAX_THREADS env var: explicit override for polars threads per worker.
  - If neither is set, split MAX_TOTAL_THREADS automatically:
      * total <= 1  → 1 worker, 2 polars threads (minimum)
      * total <= 10 → 1 worker, rest to polars (min 2)
      * total > 10  → 40% workers, 60% polars threads
"""

cpu_count = multiprocessing.cpu_count()


def _parse_max_total_threads() -> int:
    """Parse MAX_TOTAL_THREADS from environment variable.

    Returns:
        Total thread budget (defaults to half the available CPU cores).
    """
    total_env = os.getenv("MAX_TOTAL_THREADS", "")
    if total_env.isdecimal():
        return int(total_env)
    return max(1, cpu_count // 2)


def split_workers(total: int) -> tuple[int, int]:
    """Split total thread budget into (n_proc, polars_threads).

    Args:
        total: Maximum total threads to allocate.

    Returns:
        Tuple of (worker processes, polars threads per worker).
    """
    if total <= 1:
        return 1, 2
    if total <= 10:
        return 1, max(2, total - 1)
    n_proc = max(1, math.ceil(total * 0.4))
    polars = max(2, total - n_proc)
    return n_proc, polars


def compute_concurrency() -> tuple[int, int]:
    """Resolve final N_PROC and POLARS_MAX_THREADS.

    Explicit env vars take precedence over the auto-split.

    Returns:
        Tuple of (n_proc, polars_threads).
    """
    n_proc_env = os.getenv("N_PROC", "")
    polars_env = os.getenv("POLARS_MAX_THREADS", "")
    total = _parse_max_total_threads()

    auto_n_proc, auto_polars = split_workers(total)

    n_proc = int(n_proc_env) if n_proc_env.isdecimal() else auto_n_proc
    polars_threads = int(polars_env) if polars_env.isdecimal() else auto_polars

    return n_proc, polars_threads


# Set POLARS_MAX_THREADS in the environment *before* any polars import.
# Polars reads this env var at init time; if unset it defaults to all CPUs,
# which combined with multiprocessing workers exhausts OS thread limits.
_N_PROC, _POLARS_THREADS = compute_concurrency()
os.environ["POLARS_MAX_THREADS"] = str(_POLARS_THREADS)


def _is_debug() -> bool:
    """Check if debug mode is enabled.

    Returns:
        True if DEBUG environment variable is set to "true"
    """
    return os.getenv("DEBUG", "false").lower() == "true"


# Runtime configuration
DEBUG: bool = _is_debug()
N_PROC: int = _N_PROC

# Override for debug mode: single-worker, single polars thread
if DEBUG:
    N_PROC = 1
    os.environ["POLARS_MAX_THREADS"] = "1"
    print(f"Running in DEBUG mode. N_PROC={N_PROC}")

# List of specific mailing lists to parse (empty = parse all)
LISTS_TO_PARSE: list[str] = [
    item for item in os.getenv("LISTS_TO_PARSE", "").split(",") if item
]

# Directory paths (required environment variables)
INPUT_DIR_PATH: str = os.environ["INPUT_DIR"]
OUTPUT_DIR_PATH: str = os.environ["OUTPUT_DIR"]

# defaults to maximum compression for efficient storage
# this is very resource intensive, and high levels come with diminishing returns
COMPRESSION_LEVEL: int = int(os.getenv("COMPRESSION_LEVEL", "22"))
