"""Identity map for tracking pseudonymization occurrences.

Stores per-(email, name) occurrence counts split by location
(header vs body), enabling fine-grained privacy audits.

Thread-safety guarantee
-----------------------
All mutations go through ``add_or_update``, which holds ``_lock`` for
the duration of the read-modify-write cycle.  Readers (``get_records_for_email``,
``to_dict``) also acquire the lock so they always see a consistent snapshot.

Schema
------
Each *unique (email, name)* pair is stored as one ``IdentityRecord``:

    email         : str            – original e-mail address
    name          : str | None     – display name, or None when absent
    header_count  : int            – times seen in a message header
    body_count    : int            – times seen in raw_body text
    total_count   : int (property) – header_count + body_count (derived)

Design decisions
----------------
* The compound key is ``(email, name)`` rather than ``email`` alone because
  the spec requires "same email with two different names → two rows".
* ``total_count`` is a ``@property`` (not stored) so it is always
  consistent with the component counts – no sync bug possible.
* Serialisation uses plain dicts so the map can be written to JSON or
  Parquet without extra dependencies.
"""

from __future__ import annotations

import json
import os
import tempfile
import threading
from dataclasses import dataclass, field
from typing import Literal

# Sentinel used as the dict key when no display name is present.
_NO_NAME: str = "\x00NO_NAME\x00"


@dataclass
class IdentityRecord:
    """Occurrence counts for a single (email, name) identity pair."""

    email: str
    name: str | None
    header_count: int = field(default=0)
    body_count: int = field(default=0)

    @property
    def total_count(self) -> int:
        """Derived count – always equal to header_count + body_count."""
        return self.header_count + self.body_count

    # ------------------------------------------------------------------
    # Serialisation helpers
    # ------------------------------------------------------------------

    def to_dict(self) -> dict:
        return {
            "email": self.email,
            "name": self.name,
            "header_count": self.header_count,
            "body_count": self.body_count,
            "total_count": self.total_count,
        }

    @classmethod
    def from_dict(cls, data: dict) -> "IdentityRecord":
        return cls(
            email=data["email"],
            name=data.get("name"),
            header_count=data.get("header_count", 0),
            body_count=data.get("body_count", 0),
        )


class IdentityMap:
    """Thread-safe store mapping (email, name) pairs to IdentityRecord objects.

    Usage
    -----
    >>> imap = IdentityMap()
    >>> imap.add_or_update("alice@example.com", name="Alice", location="body")
    >>> records = imap.get_records_for_email("alice@example.com")
    >>> records[0].body_count
    1
    """

    def __init__(self) -> None:
        # Keyed by (email, name_key) where name_key is _NO_NAME when name is None.
        self._records: dict[tuple[str, str], IdentityRecord] = {}
        self._lock = threading.Lock()

    # ------------------------------------------------------------------
    # Public mutation API
    # ------------------------------------------------------------------

    def add_or_update(
        self,
        email: str,
        *,
        name: str | None,
        location: Literal["header", "body"],
    ) -> None:
        """Increment the appropriate count for the (email, name) pair.

        Creates the record if it does not exist yet.

        Args:
            email:    The email address being recorded.
            name:     Associated display name, or ``None`` if absent.
            location: ``"header"`` or ``"body"`` – which counter to bump.
        """
        if location not in ("header", "body"):
            raise ValueError(f"location must be 'header' or 'body', got {location!r}")

        name_key = name if name is not None else _NO_NAME
        key = (email, name_key)

        with self._lock:
            if key not in self._records:
                self._records[key] = IdentityRecord(email=email, name=name)
            record = self._records[key]
            if location == "header":
                record.header_count += 1
            else:
                record.body_count += 1

    # ------------------------------------------------------------------
    # Public query API
    # ------------------------------------------------------------------

    def get_records_for_email(self, email: str) -> list[IdentityRecord]:
        """Return all records for a given email address (any name variant).

        Returns an empty list when the email has never been seen.
        """
        with self._lock:
            return [r for (e, _), r in self._records.items() if e == email]

    # ------------------------------------------------------------------
    # Serialisation
    # ------------------------------------------------------------------

    def to_dict(self) -> list[dict]:
        """Serialise to a JSON-compatible list of record dicts."""
        with self._lock:
            return [r.to_dict() for r in self._records.values()]

    @classmethod
    def from_dict(cls, data: list[dict]) -> "IdentityMap":
        """Deserialise from the format produced by ``to_dict``."""
        instance = cls()
        for item in data:
            record = IdentityRecord.from_dict(item)
            name_key = record.name if record.name is not None else _NO_NAME
            key = (record.email, name_key)
            instance._records[key] = record
        return instance

    # ------------------------------------------------------------------
    # Disk persistence
    # ------------------------------------------------------------------

    def save(self, path: str) -> None:
        """Atomically write the map to *path* as a JSON file.

        Uses a write-to-temp-then-rename strategy so that a crash mid-write
        never leaves a truncated file at the target path.

        Args:
            path: Destination file path (e.g. ``output/my_list/identity_map.json``).
        """
        data = self.to_dict()  # acquires lock, produces a snapshot
        dir_name = os.path.dirname(path) or "."
        os.makedirs(dir_name, exist_ok=True)

        # Write to a sibling temp file, then atomically rename
        fd, tmp_path = tempfile.mkstemp(dir=dir_name, suffix=".tmp")
        try:
            with os.fdopen(fd, "w", encoding="utf-8") as fh:
                json.dump(data, fh, ensure_ascii=False, indent=2)
            os.replace(tmp_path, path)  # atomic on POSIX; best-effort on Windows
        except Exception:
            # Clean up orphaned temp file on error
            try:
                os.unlink(tmp_path)
            except OSError:
                pass
            raise

    @classmethod
    def load(cls, path: str) -> "IdentityMap":
        """Load an identity map from a JSON file previously written by :meth:`save`.

        Returns an empty map when the file does not exist yet (first run).

        Args:
            path: Source file path.

        Returns:
            Populated :class:`IdentityMap` instance.
        """
        if not os.path.exists(path):
            return cls()
        with open(path, encoding="utf-8") as fh:
            data = json.load(fh)
        return cls.from_dict(data)
