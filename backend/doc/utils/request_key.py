"""
Helpers for building document request keys.

A document request key is used as the cache key and as the filename (and
therefore the download URL) of a generated document. It has the shape:

    <resources-slug>_<hash>

The slug is human readable and encodes only the resources requested. The
hash is derived from a canonical form of the entire request, including
settings, so that requests sharing resources but differing in settings
get distinct keys while identical requests always get the same key.
"""

import hashlib
import json
import re
from enum import Enum
from typing import Iterable, Mapping, Sequence, TypeAlias

# Bump this to invalidate every previously cached document, e.g., after a
# change to templates or rendering that the request itself doesn't capture.
KEY_SCHEMA_VERSION: int = 1

# 12 hex characters is 48 bits of the SHA-256 digest. A collision would
# serve the wrong document, so don't shorten this.
HASH_HEX_LENGTH: int = 12

# Per group limits on slug length. A group that would be longer than its
# limit is summarized by count instead, e.g., "27books".
MAX_LANGS_SLUG_LEN: int = 20
MAX_TYPES_SLUG_LEN: int = 20
MAX_BOOKS_SLUG_LEN: int = 30

JsonScalar: TypeAlias = str | int | float | bool | None

# What may appear in the payload that is hashed: JSON values, plus enums
# which are canonicalized as their values.
CanonicalValue: TypeAlias = (
    JsonScalar | Enum | Sequence["CanonicalValue"] | Mapping[str, "CanonicalValue"]
)

_NON_SLUG_CHARS = re.compile(r"[^a-z0-9]+")


def slugify(value: object) -> str:
    """
    Reduce value to lower case ASCII letters and digits separated by
    single hyphens so that it is always safe to use in a filename.
    Never returns an empty string.
    """
    slug = _NON_SLUG_CHARS.sub("-", str(value).lower()).strip("-")
    return slug or "x"


def unique_in_order(values: Iterable[str]) -> list[str]:
    """Return values with duplicates removed, keeping first appearances."""
    return list(dict.fromkeys(values))


def slug_group(values: Iterable[object], noun: str, max_len: int) -> str:
    """
    Join the unique slugified values with hyphens. If the result is
    longer than max_len summarize it as a count of noun instead.
    """
    unique = unique_in_order(slugify(value) for value in values)
    joined = "-".join(unique)
    if len(joined) <= max_len:
        return joined
    return f"{len(unique)}{noun}"


def _json_default(value: object) -> JsonScalar:
    if isinstance(value, Enum):
        enum_value = value.value
        if enum_value is None or isinstance(enum_value, (str, int, float, bool)):
            return enum_value
    raise TypeError(f"Cannot canonicalize value of type {type(value).__name__}")


def canonical_json(payload: CanonicalValue) -> str:
    """
    Serialize payload to a string that is identical for equal payloads:
    sorted object keys, no insignificant whitespace, enums as their
    values. Sequences keep their order, so order is significant.
    """
    return json.dumps(
        payload,
        sort_keys=True,
        separators=(",", ":"),
        ensure_ascii=True,
        default=_json_default,
    )


def short_hash(
    payload: CanonicalValue,
    version: int = KEY_SCHEMA_VERSION,
    length: int = HASH_HEX_LENGTH,
) -> str:
    """
    Return the first length hex characters of the SHA-256 of the
    canonical form of payload together with the key schema version.
    Deterministic across processes and restarts.
    """
    canonical = canonical_json({"version": version, "request": payload})
    return hashlib.sha256(canonical.encode("utf-8")).hexdigest()[:length]


def request_key(slug: str, payload: CanonicalValue) -> str:
    """Return the key: the resources slug followed by the request hash."""
    return f"{slug}_{short_hash(payload)}"


def resources_slug(
    lang_codes: Sequence[object],
    resource_types: Sequence[object],
    book_codes: Sequence[object],
) -> str:
    """Return the slug for the languages, resource types, and books requested."""
    return "_".join(
        [
            slug_group(lang_codes, "langs", MAX_LANGS_SLUG_LEN),
            slug_group(resource_types, "types", MAX_TYPES_SLUG_LEN),
            slug_group(book_codes, "books", MAX_BOOKS_SLUG_LEN),
        ]
    )
