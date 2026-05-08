"""
TDD Unit Tests for Issue #17: Extend pseudonymization to raw_body.

These tests are written BEFORE implementation (Red phase of TDD).
They define the expected behaviour of the new `body_anonymizer` module,
which must:

  1. Detect and hash email addresses (and associated display names)
     inside a raw_body string.
  2. Produce the SAME hash as the existing header anonymizer for the
     same email address (hash consistency).
  3. Update an identity map with body_count / header_count / total_count.
  4. Handle plain text, quoted text (> >>), HTML, and code snippets
     according to the spec in Context_and_Behavior.md.

Modules expected to exist after implementation:
  mlh_anonymizer.body_anonymizer   – new module under test
  mlh_anonymizer.identity_map      – new module under test
  mlh_anonymizer.hasher            – existing (already passes)

Run with:
  pytest anonymizer/tests/test_body_anonymizer.py -v
"""

import pytest
import json
import threading

# ---------------------------------------------------------------------------
# Imports – will raise ImportError until implementation exists (RED phase)
# ---------------------------------------------------------------------------
from mlh_anonymizer.hasher import generate_sha1_hash
from mlh_anonymizer.anonymizer import mlh_anonymizer

from mlh_anonymizer.body_anonymizer import anonymize_body
from mlh_anonymizer.identity_map import (
    IdentityMap,
    IdentityRecord,
)

# ===========================================================================
# Fixtures
# ===========================================================================


@pytest.fixture()
def fresh_map() -> IdentityMap:
    """Return a brand-new, empty identity map."""
    return IdentityMap()


@pytest.fixture()
def known_email() -> str:
    return "alice@example.com"


@pytest.fixture()
def known_name() -> str:
    return "Alice Doe"


@pytest.fixture()
def known_email_hash(known_email) -> str:
    return generate_sha1_hash(known_email)


@pytest.fixture()
def known_name_hash(known_name) -> str:
    return generate_sha1_hash(known_name)


# ===========================================================================
# 1. REGEX / DETECTION TESTS
#    Verifies that the right email patterns are detected inside raw_body.
# ===========================================================================

class TestIntegrationWithHeaderAnonymizer:
    def test_body_hash_matches_header_anonymizer_output(self, fresh_map, known_email):
        """Hash from anonymize_body matches mlh_anonymizer() for same email."""
        header_hash = mlh_anonymizer(known_email)  # existing header path
        body = f"From: {known_email}"
        result, _ = anonymize_body(body, fresh_map)
        assert header_hash in result

class TestEmailDetection:
    """anonymize_body must detect every email pattern mandated by the spec."""

    def test_plain_email_detected(self, fresh_map, known_email, known_email_hash):
        """A bare email address in plain text is replaced with its hash."""
        body = f"Please contact {known_email} for details."
        result, _ = anonymize_body(body, fresh_map)
        assert known_email not in result
        assert known_email_hash in result

    def test_display_name_with_angle_brackets(
        self, fresh_map, known_email, known_name, known_email_hash, known_name_hash
    ):
        """'Display Name <email>' pattern: both name and email are hashed."""
        body = f"Sent by {known_name} <{known_email}>"
        result, _ = anonymize_body(body, fresh_map)
        assert known_email not in result
        assert known_name not in result
        assert known_email_hash in result
        assert known_name_hash in result

    def test_quoted_text_single_gt(self, fresh_map, known_email, known_email_hash):
        """Emails inside singly-quoted lines (> ...) are detected."""
        body = f"> From: {known_email}"
        result, _ = anonymize_body(body, fresh_map)
        assert known_email not in result
        assert known_email_hash in result

    def test_quoted_text_double_gt(self, fresh_map, known_email, known_email_hash):
        """Emails inside doubly-quoted lines (>> ...) are detected."""
        body = f">> On Mon, Jan 1 {known_email} wrote:"
        result, _ = anonymize_body(body, fresh_map)
        assert known_email not in result
        assert known_email_hash in result

    def test_quoted_text_deeply_nested(self, fresh_map, known_email, known_email_hash):
        """Emails inside deeply nested quoted lines (>>> ...) are detected."""
        body = f">>>>>> Original message from {known_email}"
        result, _ = anonymize_body(body, fresh_map)
        assert known_email not in result
        assert known_email_hash in result

    def test_html_mailto_attribute(self, fresh_map, known_email, known_email_hash):
        """Emails inside HTML mailto: hrefs are hashed; HTML structure preserved."""
        body = f'<a href="mailto:{known_email}">Contact</a>'
        result, _ = anonymize_body(body, fresh_map)
        assert known_email not in result
        assert known_email_hash in result
        # HTML structure must remain valid (tag still present)
        assert "<a " in result
        assert "</a>" in result

    def test_html_text_node(self, fresh_map, known_email, known_email_hash):
        """Emails appearing as plain text inside HTML tags are hashed."""
        body = f"<p>Reply to {known_email} directly.</p>"
        result, _ = anonymize_body(body, fresh_map)
        assert known_email not in result
        assert known_email_hash in result

    def test_multiple_distinct_emails_in_body(self, fresh_map):
        """All distinct email addresses in the body are hashed independently."""
        emails = ["alpha@example.com", "beta@example.org", "gamma@lists.net"]
        body = " ".join(emails)
        result, _ = anonymize_body(body, fresh_map)
        for email in emails:
            assert email not in result
            assert generate_sha1_hash(email) in result

    def test_same_email_appears_multiple_times(
        self, fresh_map, known_email, known_email_hash
    ):
        """Repeated occurrences of the same email are all replaced."""
        body = f"{known_email} cc'd {known_email} again."
        result, _ = anonymize_body(body, fresh_map)
        assert known_email not in result
        # The hash should appear where both occurrences were
        assert result.count(known_email_hash) == 2


# ===========================================================================
# 2. NEGATIVE / NON-MATCH TESTS
#    Things that must NOT be hashed.
# ===========================================================================


class TestNegativeCases:
    """Patterns that must NOT trigger anonymization."""

    def test_standalone_domain_not_hashed(self, fresh_map):
        """A domain without '@' must not be hashed (e.g. 'example.com')."""
        body = "Visit example.com for more info."
        result, _ = anonymize_body(body, fresh_map)
        assert "example.com" in result

    def test_name_without_email_not_hashed(self, fresh_map, known_name):
        """A display name with no accompanying email must not be hashed."""
        body = f"Greetings, {known_name}. How are you?"
        result, _ = anonymize_body(body, fresh_map)
        assert known_name in result

    def test_empty_body_returns_unchanged(self, fresh_map):
        """An empty string returns empty string without error."""
        result, updated_map = anonymize_body("", fresh_map)
        assert result == ""

    def test_body_with_no_emails_unchanged(self, fresh_map):
        """A body containing no emails is returned verbatim."""
        body = "This message has no personal information whatsoever."
        result, _ = anonymize_body(body, fresh_map)
        assert result == body

    def test_at_sign_without_domain_not_hashed(self, fresh_map):
        """A bare '@' or partial pattern like '@foo' must not be treated as email."""
        body = "Tag me @foo if needed."
        result, _ = anonymize_body(body, fresh_map)
        assert "@foo" in result

    def test_url_with_at_sign_not_hashed(self, fresh_map):
        """URLs like 'http://user@host/path' should not be treated as email addresses."""
        body = "Repo at http://git@github.com/org/repo.git"
        result, _ = anonymize_body(body, fresh_map)
        # The URL-embedded pattern should NOT be treated as email
        assert generate_sha1_hash("git@github.com") not in result


# ===========================================================================
# 3. HASH CONSISTENCY TESTS
#    The hash produced for a raw_body email MUST equal the hash produced
#    by the existing header anonymizer for the same address.
# ===========================================================================


class TestHashConsistency:
    """Hash consistency between header and body anonymization."""

    def test_email_hash_matches_header_hash(
        self, fresh_map, known_email, known_email_hash
    ):
        """
        The hash of an email in raw_body must be identical to what
        generate_sha1_hash(email) returns – the same function used for headers.
        """
        body = f"Contact {known_email} for info."
        result, _ = anonymize_body(body, fresh_map)
        assert known_email_hash in result

    def test_name_hash_matches_header_hash(
        self, fresh_map, known_email, known_name, known_name_hash
    ):
        """
        The hash of a display name in raw_body must equal
        generate_sha1_hash(name) – same function used for header names.
        """
        body = f"{known_name} <{known_email}>"
        result, _ = anonymize_body(body, fresh_map)
        assert known_name_hash in result

    def test_consistency_across_multiple_calls(self, known_email, known_email_hash):
        """
        Calling anonymize_body multiple times with the same email always
        produces the same hash (deterministic / stateless hashing).
        """
        body = f"From: {known_email}"
        map1, map2 = IdentityMap(), IdentityMap()
        result1, _ = anonymize_body(body, map1)
        result2, _ = anonymize_body(body, map2)
        assert result1 == result2
        assert known_email_hash in result1


# ===========================================================================
# 4. NAME HANDLING EDGE CASES
# ===========================================================================


class TestNameHandling:
    """Edge cases around display name + email combinations."""

    def test_same_name_two_different_emails_two_rows(self, fresh_map, known_name):
        """Same name with two different emails → separate identity records."""
        email_a = "alice@company.com"
        email_b = "alice@personal.com"
        body = f"{known_name} <{email_a}>\n{known_name} <{email_b}>"
        _, updated_map = anonymize_body(body, fresh_map)
        records_a = updated_map.get_records_for_email(email_a)
        records_b = updated_map.get_records_for_email(email_b)
        assert len(records_a) >= 1
        assert len(records_b) >= 1

    def test_email_without_name_stored_separately(self, fresh_map, known_email, known_name):
        """
        An email appearing bare (no name) and the same email with a name
        are treated as separate identity records.
        """
        body = f"{known_name} <{known_email}>\n{known_email}"
        _, updated_map = anonymize_body(body, fresh_map)
        records = updated_map.get_records_for_email(known_email)
        # One record with name, one without
        assert any(r.name == known_name for r in records)
        assert any(r.name is None for r in records)

    def test_unicode_display_name(self, fresh_map, known_email, known_email_hash):
        """Display names with non-ASCII characters are handled without errors."""
        body = f"Ångström Müller <{known_email}>"
        result, _ = anonymize_body(body, fresh_map)
        assert known_email not in result
        assert known_email_hash in result
    
    def test_email_with_plus_tag(self, fresh_map):
        """Emails with '+' tags (e.g., user+tag@example.com) are detected."""
        body = "Contact user+tag@example.com for info."
        result, _ = anonymize_body(body, fresh_map)
        assert "user+tag@example.com" not in result
        assert generate_sha1_hash("user+tag@example.com") in result

    def test_email_with_dots_in_local_part(self, fresh_map):
        """Emails like first.last@example.com are detected."""
        body = "Reach first.last@example.com"
        result, _ = anonymize_body(body, fresh_map)
        assert "first.last@example.com" not in result

    def test_email_case_sensitivity(self, fresh_map):
        """Email hashing should normalize case (or not) — define expected behavior."""
        body = "Contact Alice@Example.COM"
        result, _ = anonymize_body(body, fresh_map)
        # Decide: should the hash be for "Alice@Example.COM" as-is,
        # or normalized to "alice@example.com"?
        # The spec says "same email = same hash" — clarify case handling.


# ===========================================================================
# 5. IDENTITY MAP SCHEMA TESTS
# ===========================================================================


class TestIdentityMapSchema:
    """IdentityRecord must track header_count, body_count, total_count."""

    def test_first_body_occurrence_sets_counts_correctly(self, fresh_map, known_email):
        """A freshly added record starts with all counts at zero."""
        fresh_map.add_or_update(known_email, name=None, location="body")
        record = fresh_map.get_records_for_email(known_email)[0]
        assert record.body_count == 1
        assert record.header_count == 0
        assert record.total_count == 1

    def test_body_count_incremented_on_body_occurrence(self, fresh_map, known_email):
        """body_count increments each time the email appears in a body."""
        body = f"{known_email} and again {known_email}"
        _, updated_map = anonymize_body(body, fresh_map)
        record = updated_map.get_records_for_email(known_email)[0]
        assert record.body_count == 2

    def test_header_count_not_incremented_by_body_call(self, fresh_map, known_email):
        """Processing raw_body must NOT modify header_count."""
        body = f"from: {known_email}"
        _, updated_map = anonymize_body(body, fresh_map)
        record = updated_map.get_records_for_email(known_email)[0]
        assert record.header_count == 0

    def test_total_count_equals_header_plus_body(self, known_email):
        """total_count must always equal header_count + body_count."""
        imap = IdentityMap()
        # Simulate header processing
        imap.add_or_update(known_email, name=None, location="header")
        imap.add_or_update(known_email, name=None, location="header")
        # Simulate body processing
        imap.add_or_update(known_email, name=None, location="body")
        record = imap.get_records_for_email(known_email)[0]
        assert record.total_count == record.header_count + record.body_count

    def test_identity_record_has_required_fields(self, fresh_map, known_email):
        """IdentityRecord exposes header_count, body_count, total_count, email, name."""
        fresh_map.add_or_update(known_email, name=None, location="body")
        record = fresh_map.get_records_for_email(known_email)[0]
        assert hasattr(record, "email")
        assert hasattr(record, "name")
        assert hasattr(record, "header_count")
        assert hasattr(record, "body_count")
        assert hasattr(record, "total_count")

    def test_identity_map_returns_empty_list_for_unknown_email(self, fresh_map):
        """Querying an unregistered email returns an empty list, not an error."""
        result = fresh_map.get_records_for_email("nobody@nowhere.com")
        assert result == []


# ===========================================================================
# 6. MAP PERSISTENCE / THREAD SAFETY
# ===========================================================================


class TestMapPersistence:
    """Identity map updates must be atomic / thread-safe."""

    def test_identity_map_serialises_to_dict(self, known_email):
        """IdentityMap.to_dict() returns a plain-Python structure (JSON-serialisable)."""
        imap = IdentityMap()
        imap.add_or_update(known_email, name=None, location="body")
        data = imap.to_dict()
        # Must be JSON-serialisable without error
        json.dumps(data)

    def test_identity_map_round_trips(self, known_email):
        """IdentityMap can be serialised then deserialised with no data loss."""
        imap = IdentityMap()
        imap.add_or_update(known_email, name="Test User", location="body")
        data = imap.to_dict()
        restored = IdentityMap.from_dict(data)
        records = restored.get_records_for_email(known_email)
        assert len(records) == 1
        assert records[0].body_count == 1

    def test_concurrent_updates_no_data_loss(self, known_email):
        """
        Concurrent body_count increments from multiple threads must not
        produce a race condition (total must equal number of threads).
        """

        imap = IdentityMap()
        n_threads = 50

        def increment():
            imap.add_or_update(known_email, name=None, location="body")

        threads = [threading.Thread(target=increment) for _ in range(n_threads)]
        for t in threads:
            t.start()
        for t in threads:
            t.join()

        record = imap.get_records_for_email(known_email)[0]
        assert record.body_count == n_threads


# ===========================================================================
# 7. OUTPUT FORMAT TESTS
#    The replacement token format must follow the spec:
#    name -> <HASH_NAME>  email -> <HASH_EMAIL>
# ===========================================================================


class TestOutputFormat:
    """Replacement tokens must follow spec: <HASH_NAME> <HASH_EMAIL>."""

    def test_bare_email_replaced_with_hash_email_token(
        self, fresh_map, known_email, known_email_hash
    ):
        """Bare email is replaced with the raw hash string of the email."""
        body = known_email
        result, _ = anonymize_body(body, fresh_map)
        # The hash (hex string) must be present; original must not
        assert known_email_hash in result
        assert known_email not in result

    def test_named_email_replaced_with_name_hash_and_email_hash(
        self,
        fresh_map,
        known_email,
        known_name,
        known_email_hash,
        known_name_hash,
    ):
        """'Name <email>' becomes '<name_hash> <email_hash>' in the output."""
        body = f"{known_name} <{known_email}>"
        result, _ = anonymize_body(body, fresh_map)
        assert known_name not in result
        assert known_email not in result
        assert known_name_hash in result
        assert known_email_hash in result

    def test_output_is_string(self, fresh_map, known_email):
        """anonymize_body always returns a string as its first element."""
        body = f"hello {known_email}"
        result, _ = anonymize_body(body, fresh_map)
        assert isinstance(result, str)

    def test_output_map_is_identity_map_instance(self, fresh_map, known_email):
        """anonymize_body returns an IdentityMap as its second element."""
        body = f"hello {known_email}"
        _, updated_map = anonymize_body(body, fresh_map)
        assert isinstance(updated_map, IdentityMap)

    def test_bare_email_replacement_format(self, fresh_map, known_email, known_email_hash):
        body = f"Contact {known_email}"
        result, _ = anonymize_body(body, fresh_map)
        # Verify the exact replacement format matches the spec
        assert f"<{known_email_hash}>" in result  # if angle-bracket wrapping is required


# ===========================================================================
# 8. HTML-SPECIFIC TESTS
# ===========================================================================


class TestHtmlHandling:
    """Email addresses embedded in HTML must be hashed; markup must survive."""

    def test_email_in_html_attribute_hashed(self, fresh_map, known_email, known_email_hash):
        """Email inside an HTML attribute value is detected and hashed."""
        body = f'<input type="email" value="{known_email}" />'
        result, _ = anonymize_body(body, fresh_map)
        assert known_email not in result
        assert known_email_hash in result

    def test_html_tags_survive_anonymization(self, fresh_map, known_email):
        """HTML tags themselves are not mangled by the anonymizer."""
        body = f"<p>Contact <strong>{known_email}</strong> now.</p>"
        result, _ = anonymize_body(body, fresh_map)
        assert "<p>" in result
        assert "<strong>" in result
        assert "</strong>" in result
        assert "</p>" in result

    def test_multipart_html_body(self, fresh_map, known_email, known_email_hash):
        """A realistic multipart HTML body with headers and email links is handled."""
        body = (
            "Content-Type: text/html\r\n\r\n"
            "<html><body>"
            f'<p>Please reply to <a href="mailto:{known_email}">{known_email}</a></p>'
            "</body></html>"
        )
        result, _ = anonymize_body(body, fresh_map)
        assert known_email not in result
        assert known_email_hash in result


# ===========================================================================
# 9. QUOTED TEXT (THREADING) TESTS
# ===========================================================================


class TestQuotedText:
    """Emails in threaded email quotes must be pseudonymized."""

    def test_quoted_from_line(self, fresh_map, known_email, known_email_hash):
        """> From: email@example.com lines are anonymized."""
        body = f"> From: {known_email}"
        result, _ = anonymize_body(body, fresh_map)
        assert known_email not in result
        assert known_email_hash in result

    def test_quoted_on_wrote_pattern(self, fresh_map, known_email, known_email_hash):
        """Common '> On <date>, <Name> <email> wrote:' pattern is handled."""
        body = f"> On Mon 1 Jan, {known_email} wrote:\n> Thanks for the patch."
        result, _ = anonymize_body(body, fresh_map)
        assert known_email not in result
        assert known_email_hash in result

    def test_quoted_and_non_quoted_same_email(
        self, fresh_map, known_email, known_email_hash
    ):
        """
        When the same email appears in both a quoted section and the non-quoted
        body, both occurrences are replaced and counted in body_count.
        """
        body = (
            f"> Original sender: {known_email}\n"
            f"I agree with {known_email}'s point."
        )
        result, updated_map = anonymize_body(body, fresh_map)
        assert result.count(known_email_hash) == 2
        record = updated_map.get_records_for_email(known_email)[0]
        assert record.body_count == 2


# ===========================================================================
# 10. BACKFILL / BATCH-READINESS TESTS
# ===========================================================================


class TestBackfillReadiness:
    """anonymize_body must behave correctly when called for large batch workloads."""

    def test_large_body_processed_without_error(self, fresh_map):
        """A very large body string (10 000 lines) is processed without error."""
        emails = [f"user{i}@domain{i}.com" for i in range(100)]
        lines = []
        for i in range(10_000):
            lines.append(f"Line {i}: contact {emails[i % 100]} for info.")
        body = "\n".join(lines)
        result, updated_map = anonymize_body(body, fresh_map)  # must not raise
        assert isinstance(result, str)

    def test_idempotent_on_already_anonymized_body(self, fresh_map, known_email):
        """
        Calling anonymize_body on an already-pseudonymized body must not
        double-hash or corrupt the output (important for backfill re-runs).
        """
        body = f"Contact {known_email}"
        result1, map1 = anonymize_body(body, fresh_map)
        # Run again on the already-anonymized output
        result2, _ = anonymize_body(result1, map1)
        # The known email hash is a 40-char hex string; it should NOT be hashed again
        email_hash = generate_sha1_hash(known_email)
        # Second pass: the hash string itself should still be present (not double-hashed)
        assert email_hash in result2