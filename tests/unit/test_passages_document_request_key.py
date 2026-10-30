import re
from typing import Optional

from passages.domain import document_generator
from passages.domain.model import BibleReferenceWithAvailability
from passages.reviewers_guide.model import BibleReference

KEY_PATTERN = re.compile(r"^[a-z0-9_-]+_[0-9a-f]{12}$")


def passage(
    book_code: str = "mat",
    start_chapter: int = 3,
    start_chapter_verse_ref: str = "1-3",
    end_chapter: Optional[int] = None,
    end_chapter_verse_ref: Optional[str] = None,
    is_available: bool = True,
) -> BibleReferenceWithAvailability:
    return BibleReferenceWithAvailability(
        reference=BibleReference(
            lang_code="en",
            book_code=book_code,
            book_name=book_code.title(),
            start_chapter=start_chapter,
            start_chapter_verse_ref=start_chapter_verse_ref,
            end_chapter=end_chapter,
            end_chapter_verse_ref=end_chapter_verse_ref,
        ),
        is_available=is_available,
    )


def key(
    passages: list[BibleReferenceWithAvailability],
    lang0_code: str = "en",
    lang1_code: Optional[str] = None,
    lang0_name: str = "English",
    lang1_name: Optional[str] = None,
) -> str:
    return document_generator.document_request_key(
        lang0_code, lang1_code, passages, lang0_name, lang1_name
    )


def test_key_slug_encodes_languages_and_books() -> None:
    assert re.match(
        r"en_mat-mrk_2passages_[0-9a-f]{12}$", key([passage(), passage("mrk")])
    )
    assert re.match(
        r"en-fr_mat_1passages_[0-9a-f]{12}$",
        key([passage()], lang1_code="fr", lang1_name="French"),
    )


def test_key_is_deterministic() -> None:
    assert key([passage()]) == key([passage()])


def test_verse_references_that_used_to_collide_get_distinct_keys() -> None:
    """The old key mapped each of ':;,-' to '_' so these were the same file."""
    refs = ["1-3", "1,3", "1;3", "1_3"]
    keys = {key([passage(start_chapter_verse_ref=ref)]) for ref in refs}
    assert len(keys) == len(refs)


def test_end_chapter_and_verse_are_part_of_the_key() -> None:
    assert key([passage(start_chapter_verse_ref="5")]) != key(
        [passage(start_chapter_verse_ref="5", end_chapter=4, end_chapter_verse_ref="3")]
    )
    assert key([passage(end_chapter=4, end_chapter_verse_ref="3")]) != key(
        [passage(end_chapter=4, end_chapter_verse_ref="4")]
    )


def test_language_names_availability_and_order_are_part_of_the_key() -> None:
    base = key([passage(), passage("mrk")])
    assert base != key([passage(), passage("mrk")], lang0_name="Anglais")
    assert base != key([passage(), passage("mrk", is_available=False)])
    assert base != key([passage("mrk"), passage()])


def test_key_is_short_and_stable_for_a_very_long_passage_list() -> None:
    passages = [
        passage(book_code, chapter, "1-10")
        for book_code in ("gen", "exo", "lev", "num", "deu", "jos", "jdg", "rut")
        for chapter in range(1, 25)
    ]
    long_key = key(passages)
    assert KEY_PATTERN.match(long_key)
    assert len(long_key) < 100
    assert long_key == key(passages)


def test_key_golden_value() -> None:
    """Pin an exact key so that changing the hashed payload is deliberate."""
    assert key([passage()]) == "en_mat_1passages_b3cd5734a8fa"
