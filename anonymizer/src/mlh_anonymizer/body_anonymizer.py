"""Pseudonymization of email addresses found inside raw email bodies.

Public API
----------
    anonymize_body(body: str, identity_map: IdentityMap) -> tuple[str, IdentityMap]

Algorithm
---------
Two regex passes are applied to ``body`` in order:

1. **Named pattern** – ``[context text] <local@domain>``
   Captures up to 100 chars of context before ``<email>``, then
   :func:`_extract_name` post-processes that context to isolate the
   genuine display name (stripping prose like "Sent by", header labels
   like "From:", calendar tokens like "Mon 1 Jan", timezone tokens, etc.).
   Both the cleaned name and the email are replaced with SHA-1 hashes.
   When no genuine name can be extracted the angle-bracket form is
   replaced with just the email hash token.

2. **Bare email pattern** – ``local@domain``
   Replaces standalone addresses not consumed by pass 1.

Both passes skip anything preceded by ``://`` (URL guard), satisfying
the spec constraint "do not hash URLs containing @".

Hash consistency
----------------
Both patterns call ``generate_sha1_hash`` – the same function used for
header fields – so ``hash(email_in_body) == hash(email_in_header)``.

Output token format
-------------------
* Bare email           →  ``<EMAIL_HASH>``
* Named email          →  ``<NAME_HASH> <EMAIL_HASH>``
* Angle-bracket email  →  ``<EMAIL_HASH>``  (when no clean name found)

Idempotency
-----------
A SHA-1 hex digest is 40 lowercase hex characters.  The email regex
requires ``@`` with a valid local-part and domain, so a hash token
``<abcdef…>`` cannot match the email pattern on a second pass.

Design decisions
----------------
* ``re.sub`` with callbacks (single pass per stage) avoids index-shift bugs
  and runs in O(n) time relative to body length.
* Name extraction is done in a pure Python helper rather than in the regex
  itself, keeping the regex simple and the logic testable in isolation.
* URL exclusion uses a negative lookbehind (``(?<!://)``), preserving HTML.
* ``re.UNICODE`` is active so Unicode display names match correctly.
* ``_extract_name`` returns ``(name_or_None, prefix_to_keep)`` so the
  callback can preserve non-name leading text (header labels, quote markers,
  prose) without a separate ``find()`` call that could mis-anchor on a
  repeated word.
"""

from __future__ import annotations

import re
from typing import TYPE_CHECKING

from mlh_anonymizer.hasher import generate_sha1_hash

if TYPE_CHECKING:
    from mlh_anonymizer.identity_map import IdentityMap

# ---------------------------------------------------------------------------
# Compiled regular expressions
# ---------------------------------------------------------------------------

_LOCAL = r"[a-zA-Z0-9._%+\-]+"
_DOMAIN = r"[a-zA-Z0-9.\-]+\.[a-zA-Z]{2,}"

# Bare email – skips URLs (://user@host) via negative lookbehind.
_EMAIL_RE = re.compile(
    r"(?<!://)"
    r"(?<![a-zA-Z0-9])"
    rf"({_LOCAL}@{_DOMAIN})"
    r"(?![a-zA-Z0-9])",
    re.UNICODE,
)

# Named-email: captures up to 100 chars of context before <email>.
# [^<>] (without \r\n exclusion) allows names that appear on the line
# immediately above the angle-bracket form (e.g. "> Name\n> <email>").
# Post-processing in _extract_name extracts the real display name.
_NAMED_EMAIL_RE = re.compile(
    r"([^<>]{1,100}?)"       # group 1: context (non-greedy, allows newlines)
    r"\s*<"
    rf"({_LOCAL}@{_DOMAIN})" # group 2: email
    r">",
    re.UNICODE,
)

# ---------------------------------------------------------------------------
# Name-extraction helpers
# ---------------------------------------------------------------------------

# Words that appear in email prose / headers but are NOT display-name tokens.
# NOTE: calendar month names (jan, feb, may, …) are intentionally EXCLUDED
# so that names like "May Lee" or "Jun Kim" are not wrongly truncated.
_STOPWORDS: frozenset[str] = frozenset(
    {
        "by",
        "at",
        "from",
        "on",
        "to",
        "in",
        "via",
        "sent",
        "cc",
        "the",
        "a",
        "an",
        "for",
        "and",
        "or",
        "with",
        "reply",
        "re",
        "original",
        "message",
        "wrote",
        "said",
        "writes",
        # Day-of-week abbreviations and full names only
        # (months removed to protect names like May, June, August, etc.)
        "mon",
        "tue",
        "wed",
        "thu",
        "fri",
        "sat",
        "sun",
        "monday",
        "tuesday",
        "wednesday",
        "thursday",
        "friday",
        "saturday",
        "sunday",
        "am",
        "pm",
        # Timezone abbreviations
        "utc", 
        "gmt",
        "pst", 
        "pdt", 
        "est", 
        "edt", 
        "cst", 
        "cdt", 
        "mst", 
        "mdt",
        # Other prose words that bleed into names
        "no",
    }
)

_ALPHA_RE = re.compile(r"[^a-zA-Z\u00C0-\u024F]")

# Matches tokens that are purely numeric/temporal and not part of a name:
#   ^[\d,.\-]+$          pure digits / date separators  (e.g. "2024", "1,")
#   ^\d{1,2}:\d{2}…$     time token                     (e.g. "10:00", "10:00:00")
#   ^[+\-]\d{4},?$       UTC offset with optional comma  (e.g. "+0000", "-0500,")
_NUMERIC_RE = re.compile(
    r"^[\d,.\-]+$"
    r"|^\d{1,2}:\d{2}(:\d{2})?$"
    r"|^[+\-]\d{4},?$"
)


def _word_alpha(word: str) -> str:
    """Return only the alphabetic characters of *word*, lower-cased."""
    return _ALPHA_RE.sub("", word).lower()


def _extract_name(raw_context: str) -> tuple[str | None, str]:
    """Extract a genuine display name from the text that precedes ``<email>``.

    Returns a ``(name, prefix)`` pair:

    * ``name``   – the cleaned display name, or ``None`` when no plausible
                   name can be found.
    * ``prefix`` – the portion of *raw_context* that precedes the name and
                   should be preserved verbatim in the output (header labels,
                   quote markers, prose sentences, etc.).

    Stripping order
    ---------------
    1. Leading quote characters and whitespace (``> ``).
    2. Leading header-label prefix (``From:``, ``Reply-To:``, ``Signed-off-by:``, …).
    3. Leading stopwords and purely-numeric/temporal tokens.
    4. Trailing purely-numeric/temporal tokens.
    5. Name is taken as the **last** 1–5 meaningful words that remain,
       working backwards from the token immediately before ``<email>``.
       This ensures prose sentences that precede the name are recognised
       as prefix rather than accidentally included in the name.

    Args:
        raw_context: The text captured by group 1 of ``_NAMED_EMAIL_RE``.

    Returns:
        ``(name_or_None, prefix_string)``
    """
    # ── Step 1: peel off leading quote markers ────────────────────────────
    quote_match = re.match(r"^([\s>]*)(.*)", raw_context, re.DOTALL)
    leading_quotes: str = quote_match.group(1)
    ctx: str = quote_match.group(2)

    # ── Step 2: peel off header label ────────────────────────────────────
    header_match = re.match(r"^([\w\-]+:\s*)(.*)", ctx, re.DOTALL)
    if header_match:
        header_label: str = header_match.group(1)
        ctx = header_match.group(2)
    else:
        header_label = ""

    # ── Step 3: strip surrounding punctuation ────────────────────────────
    ctx = ctx.strip(" \t,;:()")
    if not ctx:
        return None, raw_context

    # ADD THIS LINE:
    ctx = re.sub(r'\s*\([^)]*\)\s*$', '', ctx).rstrip()

    words = ctx.split()
    if not words:
        return None, raw_context

    # ── Step 4: drop leading stopwords / numeric tokens ───────────────────
    while words and (
        _word_alpha(words[0]) in _STOPWORDS or _NUMERIC_RE.match(words[0])
    ):
        words = words[1:]

    if not words:
        return None, raw_context

    # ── Step 5: drop trailing numeric tokens ─────────────────────────────
    while words and _NUMERIC_RE.match(words[-1]):
        words = words[:-1]

    if not words:
        return None, raw_context

    # ── Step 6: take name from the END of what remains ───────────────────
    # Display names sit directly adjacent to <email>.  Prose sentences
    # (commit messages, forwarding text, etc.) come first.  Collecting
    # words from the right — and stopping at the first stopword or numeric
    # token encountered while scanning backwards — isolates the name from
    # surrounding prose without losing it.
    name_words: list[str] = []
    for word in reversed(words):
        if len(name_words) >= 5:
            break
        # Once we have at least one name word, stop at stopwords / numerics
        if name_words and (
            _word_alpha(word) in _STOPWORDS or _NUMERIC_RE.match(word)
        ):
            break
        name_words.insert(0, word)

    if not name_words:
        return None, raw_context

    # Require at least one word with ≥2 alphabetic chars not in stopwords
    meaningful = [
        w
        for w in name_words
        if len(_word_alpha(w)) >= 2 and _word_alpha(w) not in _STOPWORDS
    ]
    if not meaningful:
        return None, raw_context

    has_proper = any(w[0].isupper() for w in name_words if w)
    if not has_proper:
        return None, raw_context

    name = " ".join(name_words)

    # ── Step 7: compute the prefix to preserve ───────────────────────────
    # Find where the first word of the name last appears in *ctx* so we can
    # reconstruct the prefix = everything before the name in raw_context.
    first_name_word = name_words[0]
    name_pos_in_ctx = ctx.rfind(first_name_word)
    if name_pos_in_ctx < 0:
        # Fallback: name not literally findable (shouldn't happen) → no prefix
        prefix = leading_quotes + header_label
    else:
        prose_before_name = ctx[:name_pos_in_ctx].rstrip(" \t")
        prefix = leading_quotes + header_label + prose_before_name

    return name, prefix


# ---------------------------------------------------------------------------
# Public function
# ---------------------------------------------------------------------------


def anonymize_body(
    body: str,
    identity_map: "IdentityMap",
) -> tuple[str, "IdentityMap"]:
    """Replace all email addresses (and display names) in *body* with SHA-1 hashes.

    Args:
        body:         Raw email body string (plain text, quoted text, or HTML).
        identity_map: Existing :class:`~mlh_anonymizer.identity_map.IdentityMap`
                      instance.  Mutated in-place **and** returned.

    Returns:
        ``(anonymized_body, identity_map)``
    """
    if not body:
        return body, identity_map

    # Pass 1 – angle-bracket form: "[context] <email>"
    body = _NAMED_EMAIL_RE.sub(
        lambda m: _replace_named(m, identity_map),
        body,
    )

    # Pass 2 – bare emails not consumed by pass 1
    body = _EMAIL_RE.sub(
        lambda m: _replace_bare(m, identity_map),
        body,
    )

    return body, identity_map


# ---------------------------------------------------------------------------
# Replacement callbacks
# ---------------------------------------------------------------------------


def _replace_named(match: re.Match, identity_map: "IdentityMap") -> str:
    """Callback for ``_NAMED_EMAIL_RE``.

    Extracts the display name and its preceding prefix from the context group.
    When a genuine name is found, both name and email are hashed and the
    structural prefix (header label, quote markers, prose sentences) is
    preserved.  When no name is found, only the email is hashed and the
    full context is kept.
    """
    raw_context: str = match.group(1)
    email: str = match.group(2)
    email_hash = generate_sha1_hash(email)

    name, prefix = _extract_name(raw_context)

    if name is None:
        identity_map.add_or_update(email, name=None, location="body")
        return f"{raw_context}<{email_hash}>"

    name_hash = generate_sha1_hash(name)
    identity_map.add_or_update(email, name=name, location="body")
    return f"{prefix}<{name_hash}> <{email_hash}>"


def _replace_bare(match: re.Match, identity_map: "IdentityMap") -> str:
    """Callback for ``_EMAIL_RE``: hash bare email address."""
    email: str = match.group(1)
    email_hash = generate_sha1_hash(email)
    identity_map.add_or_update(email, name=None, location="body")
    return f"<{email_hash}>"