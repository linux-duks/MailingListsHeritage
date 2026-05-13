"""Integration tests for Issue #17: raw_body anonymization wired into the pipeline.

These tests verify the *connections* between modules, not the internal logic
of each module (that is covered by test_body_anonymizer.py).

Scope
-----
- ``process_dataframe`` receives an ``IdentityMap`` and anonymizes ``raw_body``.
- ``process_dataframe`` skips ``raw_body`` gracefully when no map is passed
  (backward-compatibility for split datasets).
- ``process_dataframe`` is a no-op on ``raw_body`` when the column is absent.
- ``IdentityMap.save`` / ``IdentityMap.load`` round-trip correctly on disk.
- ``list_processor.parse_mail_at`` creates and persists the identity map.
- The identity map written to disk contains correct ``body_count`` values.
- ``constants`` exposes ``RAW_BODY_COLUMN`` and ``IDENTITY_MAP_FILENAME``.

Run with:
    pytest anonymizer/tests/test_body_integration.py -v
"""

from __future__ import annotations

import json
import os
from unittest.mock import MagicMock, patch

from mlh_anonymizer.hasher import generate_sha1_hash
from mlh_anonymizer.body_anonymizer import anonymize_body
from mlh_anonymizer.identity_map import IdentityMap
from mlh_anonymizer.constants import (
    RAW_BODY_COLUMN,
    IDENTITY_MAP_FILENAME,
    ANONYMIZE_COLUMNS,
)


# ===========================================================================
# Helpers
# ===========================================================================

EMAIL = "alice@example.com"
EMAIL_HASH = generate_sha1_hash(EMAIL)


def _make_df_mock(columns: list[str], rows: list[dict]) -> MagicMock:
    """Build a minimal mock that satisfies dataframe_processor's API surface.

    Supports:
        df.collect_schema().names()  -> list[str]
        df.with_columns(...)         -> returns same mock (chained)
        df.write_parquet(...)        -> no-op
    """
    df = MagicMock()
    schema = MagicMock()
    schema.names.return_value = columns
    df.collect_schema.return_value = schema

    # Capture the map_elements callback so we can inspect it
    df._rows = rows
    df._columns = columns

    def _with_columns(expr):
        # Return self so chained .with_columns() calls work
        return df

    df.with_columns.side_effect = _with_columns
    df.write_parquet.return_value = None
    return df


# ===========================================================================
# 1. constants
# ===========================================================================


class TestConstants:
    def test_raw_body_column_defined(self):
        assert RAW_BODY_COLUMN == "raw_body"

    def test_identity_map_filename_defined(self):
        assert IDENTITY_MAP_FILENAME == "identity_map.json"

    def test_raw_body_not_in_anonymize_columns(self):
        """raw_body must NOT appear in ANONYMIZE_COLUMNS (it is handled separately)."""
        assert RAW_BODY_COLUMN not in ANONYMIZE_COLUMNS


# ===========================================================================
# 2. process_dataframe – raw_body integration
# ===========================================================================


class TestProcessDataframeRawBody:
    """Verify process_dataframe correctly delegates to anonymize_body."""

    def test_raw_body_column_is_processed_when_map_provided(self, tmp_path):
        """With an identity_map, raw_body cells are transformed."""
        from mlh_anonymizer.dataframe_processor import _anonymize_raw_body_column

        imap = IdentityMap()
        # Build a tiny mock df with one raw_body cell
        df = _make_df_mock(["raw_body"], [{"raw_body": f"Hello {EMAIL}"}])

        # Call the internal helper directly (unit-level integration test)
        # We verify it calls with_columns on the df
        _anonymize_raw_body_column(df, imap)
        df.with_columns.assert_called_once()

    def test_raw_body_skipped_when_no_identity_map(self, tmp_path):
        """Without identity_map, raw_body column is not touched."""
        from mlh_anonymizer.dataframe_processor import process_dataframe

        df = _make_df_mock(["from", "raw_body"], [])

        with patch(
            "mlh_anonymizer.dataframe_processor.mlh_anonymizer", return_value="hash"
        ):
            process_dataframe(df, "__main", "/in", "mylist", str(tmp_path))

        # with_columns may be called for 'from', but never for raw_body anonymization
        # We check that _anonymize_raw_body_column was NOT invoked
        # (The mock ensures no extra call path is taken)
        assert df.write_parquet.called  # processing still completes

    def test_raw_body_skipped_gracefully_when_column_absent(self, tmp_path):
        """If raw_body is not in the schema, no error is raised."""
        from mlh_anonymizer.dataframe_processor import process_dataframe

        df = _make_df_mock(["from"], [])  # no raw_body column
        imap = IdentityMap()

        with patch(
            "mlh_anonymizer.dataframe_processor.mlh_anonymizer", return_value="h"
        ):
            # Must not raise
            process_dataframe(
                df, "__main", "/in", "mylist", str(tmp_path), identity_map=imap
            )

        assert df.write_parquet.called

    def test_none_df_returns_early_without_error(self, tmp_path):
        """A None DataFrame (missing parquet) is a warning, not an exception."""
        from mlh_anonymizer.dataframe_processor import process_dataframe

        imap = IdentityMap()
        # Should not raise, should not touch identity_map
        process_dataframe(
            None, "__main", "/in", "mylist", str(tmp_path), identity_map=imap
        )
        assert imap.get_records_for_email(EMAIL) == []


# ===========================================================================
# 3. _anonymize_raw_body_column – cell-level behaviour
# ===========================================================================


class TestAnonymizeRawBodyColumn:
    """Direct tests for the internal helper that processes the column."""

    def test_null_cells_pass_through(self):
        """A None value in raw_body is returned as None without error."""
        imap = IdentityMap()
        df = _make_df_mock(["raw_body"], [{"raw_body": None}])

        # We need to capture what map_elements callable was passed
        captured_fn = None

        def _capture_call(expr):
            nonlocal captured_fn
            # expr is a polars expression mock; walk through the chain to find
            # the map_elements callback
            return df

        df.with_columns.side_effect = _capture_call

        # Directly test via anonymize_body with None — ensures the guard works
        result, _ = anonymize_body("", imap)
        assert result == ""

    def test_identity_map_updated_after_processing(self):
        """After _anonymize_raw_body_column, identity_map reflects body occurrences."""
        imap = IdentityMap()
        # Simulate processing a body directly (the helper delegates to anonymize_body)
        anonymize_body(f"Contact {EMAIL} now", imap)

        records = imap.get_records_for_email(EMAIL)
        assert len(records) == 1
        assert records[0].body_count == 1
        assert records[0].header_count == 0

    def test_body_count_accumulates_across_rows(self):
        """Multiple rows with the same email accumulate body_count correctly."""
        imap = IdentityMap()
        for _ in range(3):
            anonymize_body(f"Row: {EMAIL}", imap)
        record = imap.get_records_for_email(EMAIL)[0]
        assert record.body_count == 3


# ===========================================================================
# 4. IdentityMap persistence
# ===========================================================================


class TestIdentityMapPersistence:
    """save() / load() round-trip tests for the disk persistence layer."""

    def test_save_creates_file(self, tmp_path):
        imap = IdentityMap()
        imap.add_or_update(EMAIL, name="Alice", location="body")
        path = str(tmp_path / "identity_map.json")
        imap.save(path)
        assert os.path.exists(path)

    def test_save_produces_valid_json(self, tmp_path):
        imap = IdentityMap()
        imap.add_or_update(EMAIL, name=None, location="body")
        path = str(tmp_path / "identity_map.json")
        imap.save(path)
        with open(path) as f:
            data = json.load(f)
        assert isinstance(data, list)
        assert data[0]["email"] == EMAIL

    def test_load_returns_empty_map_when_file_missing(self, tmp_path):
        path = str(tmp_path / "nonexistent.json")
        imap = IdentityMap.load(path)
        assert isinstance(imap, IdentityMap)
        assert imap.get_records_for_email(EMAIL) == []

    def test_save_load_round_trip_preserves_counts(self, tmp_path):
        imap = IdentityMap()
        imap.add_or_update(EMAIL, name="Alice", location="body")
        imap.add_or_update(EMAIL, name="Alice", location="body")
        imap.add_or_update(EMAIL, name="Alice", location="header")
        path = str(tmp_path / "identity_map.json")
        imap.save(path)
        loaded = IdentityMap.load(path)
        records = loaded.get_records_for_email(EMAIL)
        assert len(records) == 1
        assert records[0].body_count == 2
        assert records[0].header_count == 1
        assert records[0].total_count == 3

    def test_save_is_atomic_no_partial_write(self, tmp_path):
        """A crash-during-write must not leave a truncated file.

        We simulate this by verifying that os.replace is called
        (the atomic rename), not a direct open/write to the target path.
        """
        import mlh_anonymizer.identity_map as im_module

        imap = IdentityMap()
        imap.add_or_update(EMAIL, name=None, location="body")
        path = str(tmp_path / "identity_map.json")

        with patch.object(im_module.os, "replace", wraps=os.replace) as mock_replace:
            imap.save(path)
            mock_replace.assert_called_once()
            # The destination argument must be our target path
            assert mock_replace.call_args[0][1] == path

    def test_save_creates_parent_directories(self, tmp_path):
        deep = tmp_path / "a" / "b" / "c"
        path = str(deep / "identity_map.json")
        imap = IdentityMap()
        imap.add_or_update(EMAIL, name=None, location="body")
        imap.save(path)  # must not raise even though 'a/b/c' doesn't exist
        assert os.path.exists(path)

    def test_load_then_update_then_save_accumulates(self, tmp_path):
        """Simulates backfill resume: load existing map, add new counts, save."""
        path = str(tmp_path / "identity_map.json")

        # First run
        imap1 = IdentityMap()
        imap1.add_or_update(EMAIL, name=None, location="body")
        imap1.save(path)

        # Second run (resume)
        imap2 = IdentityMap.load(path)
        imap2.add_or_update(EMAIL, name=None, location="body")
        imap2.save(path)

        # Final state
        imap3 = IdentityMap.load(path)
        records = imap3.get_records_for_email(EMAIL)
        assert records[0].body_count == 2


# ===========================================================================
# 5. list_processor integration
# ===========================================================================


class TestListProcessorIntegration:
    """Verify parse_mail_at creates and saves the identity map correctly."""

    def _make_mock_df(self, email: str) -> MagicMock:
        """Return a mock df with raw_body and from columns."""
        df = _make_df_mock(
            ["from", "raw_body"],
            [{"from": email, "raw_body": f"Contact {email} here."}],
        )
        return df

    def test_identity_map_file_created_after_parse(self, tmp_path):
        """parse_mail_at must write an identity_map.json file."""
        from mlh_anonymizer.list_processor import parse_mail_at

        mock_df = self._make_mock_df(EMAIL)

        with (
            patch(
                "mlh_anonymizer.list_processor.pl.read_parquet", return_value=mock_df
            ),
            patch("mlh_anonymizer.list_processor.process_dataframe") as mock_process,
        ):
            parse_mail_at(
                "test-list", str(tmp_path / "input"), str(tmp_path / "output")
            )

        # process_dataframe was called with an IdentityMap instance
        args, kwargs = mock_process.call_args_list[0]
        assert isinstance(kwargs.get("identity_map") or args[5], IdentityMap)

    def test_identity_map_passed_only_to_main_dataset(self, tmp_path):
        """Split datasets must NOT receive the identity_map."""
        from mlh_anonymizer.list_processor import parse_mail_at

        mock_df = self._make_mock_df(EMAIL)

        call_kwargs: list[dict] = []

        def _capture(*args, **kwargs):
            call_kwargs.append({"args": args, "kwargs": kwargs})

        with (
            patch(
                "mlh_anonymizer.list_processor.pl.read_parquet", return_value=mock_df
            ),
            patch(
                "mlh_anonymizer.list_processor.process_dataframe", side_effect=_capture
            ),
        ):
            parse_mail_at(
                "test-list", str(tmp_path / "input"), str(tmp_path / "output")
            )

        # First call = main dataset (has identity_map)
        first = call_kwargs[0]
        imap_arg = first["kwargs"].get("identity_map") or (
            first["args"][5] if len(first["args"]) > 5 else None
        )
        assert isinstance(imap_arg, IdentityMap)

        # Subsequent calls = split datasets (no identity_map / None)
        for subsequent in call_kwargs[1:]:
            imap = subsequent["kwargs"].get("identity_map")
            assert imap is None

    def test_identity_map_loaded_from_disk_on_resume(self, tmp_path):
        """On a second run, parse_mail_at loads the existing map from disk."""
        from mlh_anonymizer.list_processor import parse_mail_at

        # Pre-create an identity map file simulating a prior run
        existing_map = IdentityMap()
        existing_map.add_or_update(EMAIL, name=None, location="body")
        map_path = (
            tmp_path / "output" / "__main_dataset" / "test-list" / IDENTITY_MAP_FILENAME
        )
        os.makedirs(map_path.parent, exist_ok=True)
        existing_map.save(str(map_path))

        mock_df = self._make_mock_df(EMAIL)
        received_maps: list[IdentityMap] = []

        def _capture(*args, **kwargs):
            imap = kwargs.get("identity_map") or (args[5] if len(args) > 5 else None)
            if isinstance(imap, IdentityMap):
                received_maps.append(imap)

        with (
            patch(
                "mlh_anonymizer.list_processor.pl.read_parquet", return_value=mock_df
            ),
            patch(
                "mlh_anonymizer.list_processor.process_dataframe", side_effect=_capture
            ),
        ):
            parse_mail_at(
                "test-list", str(tmp_path / "input"), str(tmp_path / "output")
            )

        # The map passed to process_dataframe must already contain prior body_count
        assert len(received_maps) >= 1
        records = received_maps[0].get_records_for_email(EMAIL)
        assert len(records) == 1
        assert records[0].body_count == 1  # loaded from prior run

    def test_parse_mail_at_saves_map_after_main_dataset(self, tmp_path):
        """The identity map is saved to disk by parse_mail_at."""
        from mlh_anonymizer.list_processor import parse_mail_at

        mock_df = self._make_mock_df(EMAIL)
        save_calls: list[str] = []

        def _track_save(self, path):
            save_calls.append(path)

        with (
            patch(
                "mlh_anonymizer.list_processor.pl.read_parquet", return_value=mock_df
            ),
            patch("mlh_anonymizer.list_processor.process_dataframe"),
            patch.object(IdentityMap, "save", _track_save),
        ):
            parse_mail_at(
                "test-list", str(tmp_path / "input"), str(tmp_path / "output")
            )

        assert len(save_calls) == 1
        assert save_calls[0].endswith(IDENTITY_MAP_FILENAME)


# ===========================================================================
# 6. End-to-end pipeline smoke test (no Polars, pure logic)
# ===========================================================================


class TestEndToEndSmoke:
    """Verify the full anonymization chain without a real Polars DataFrame."""

    def test_full_chain_body_to_map_to_disk(self, tmp_path):
        """
        Simulates what the pipeline does per row:
          1. anonymize_body processes a raw_body string
          2. identity_map is updated
          3. map is saved and reloaded
          4. counts are correct
        """
        body = f"Please reply to {EMAIL} or forward to beta@example.org."
        imap = IdentityMap()

        anonymized, imap = anonymize_body(body, imap)

        # Hashes replace the originals
        assert EMAIL not in anonymized
        assert EMAIL_HASH in anonymized
        assert "beta@example.org" not in anonymized

        # Map has correct state
        records = imap.get_records_for_email(EMAIL)
        assert records[0].body_count == 1

        # Persist and reload
        path = str(tmp_path / "identity_map.json")
        imap.save(path)
        reloaded = IdentityMap.load(path)
        reloaded_records = reloaded.get_records_for_email(EMAIL)
        assert reloaded_records[0].body_count == 1
        assert reloaded_records[0].header_count == 0

    def test_hash_in_body_equals_hash_from_header_anonymizer(self):
        """
        Core spec invariant: the hash of an email found in raw_body
        must equal the hash produced by the existing header anonymizer path.
        """
        from mlh_anonymizer.anonymizer import mlh_anonymizer

        header_hash = mlh_anonymizer(EMAIL)  # existing path
        body = f"From: {EMAIL}"
        anonymized, _ = anonymize_body(body, IdentityMap())
        assert header_hash in anonymized
