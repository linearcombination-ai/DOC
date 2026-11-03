import re

from stet.domain import document_generator

KEY_PATTERN = re.compile(r"^[a-z0-9_-]+_stet_[0-9a-f]{12}$")


def key(lang0: str = "en", lang1: str = "fr", spacing: bool = False) -> str:
    return document_generator.stet_request_key(lang0, lang1, spacing)


def test_key_is_deterministic_and_encodes_languages() -> None:
    assert key() == key()
    assert key().startswith("en-fr_stet_")
    assert KEY_PATTERN.match(key())


def test_line_spacing_and_languages_are_part_of_the_key() -> None:
    assert key(spacing=True) != key(spacing=False)
    assert key("en", "fr") != key("fr", "en")


def test_path_characters_in_language_codes_are_neutralized() -> None:
    for bad in ("../../etc/passwd", "a/b", "..", "/abs"):
        k = key(bad, bad)
        assert KEY_PATTERN.match(k), k
        assert "/" not in k and ".." not in k


def test_very_long_language_codes_keep_the_file_name_short() -> None:
    assert len(key("x" * 1000, "y" * 1000)) < 100


def test_codes_that_slugify_alike_still_get_distinct_keys() -> None:
    assert key("a/b") != key("a-b")
