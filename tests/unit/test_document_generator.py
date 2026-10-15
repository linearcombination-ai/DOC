import re
from typing import Mapping

from doc.domain import document_generator, model
from doc.domain.bible_books import BOOK_NAMES

KEY_PATTERN = re.compile(r"^[a-z0-9_-]+_[0-9a-f]{12}$")


def document_request(
    components: list[tuple[str, str, str]] | None = None,
    overrides: Mapping[str, object] | None = None,
) -> model.DocumentRequest:
    components = components or [
        ("en", "ulb", "mat"),
        ("en", "tn", "mat"),
        ("en", "tw", "mat"),
    ]
    request = model.DocumentRequest(
        assembly_strategy_kind=model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
        assembly_layout_kind=model.AssemblyLayoutEnum.ONE_COLUMN,
        resource_requests=[
            model.ResourceRequest(
                lang_code=lang_code, resource_type=resource_type, book_code=book_code
            )
            for lang_code, resource_type, book_code in components
        ],
    )
    return request.model_copy(update=dict(overrides or {}))


def test_every_document_request_field_is_classified() -> None:
    """
    A new DocumentRequest field must be deliberately classified as
    hashed into the document request key or ignored by it.
    """
    hashed = document_generator.HASHED_DOCUMENT_REQUEST_FIELDS
    ignored = document_generator.IGNORED_DOCUMENT_REQUEST_FIELDS
    assert not hashed & ignored
    assert hashed | ignored == set(model.DocumentRequest.model_fields)


def test_document_request_key_slug_encodes_resources_not_settings() -> None:
    key = document_generator.document_request_key(document_request())
    assert KEY_PATTERN.match(key)
    assert key.startswith("en_ulb-tn-tw_mat_")


def test_document_request_key_is_deterministic() -> None:
    assert document_generator.document_request_key(
        document_request()
    ) == document_generator.document_request_key(document_request())


def test_document_request_key_differs_for_each_hashed_setting() -> None:
    base_key = document_generator.document_request_key(document_request())
    variations: dict[str, object] = {
        "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_CHAPTER,
        "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN_COMPACT,
        "layout_for_print": True,
        "limit_words": False,
        "use_chapter_labels": True,
        "use_prince": True,
        "use_two_column_layout_for_tn_notes": True,
        "use_two_column_layout_for_tq_notes": True,
        "use_section_visual_separator": True,
        "show_tn_book_intro": False,
        "show_bc_book_intro": False,
        "show_tn_chapter_intro": False,
        "show_bc_chapter_commentary": False,
    }
    assert set(variations) == set(
        document_generator.HASHED_DOCUMENT_REQUEST_FIELDS - {"resource_requests"}
    )
    keys = {base_key}
    for field, value in variations.items():
        key = document_generator.document_request_key(
            document_request(overrides={field: value})
        )
        # Same resources, so same slug, but a different hash.
        assert key.rsplit("_", 1)[0] == base_key.rsplit("_", 1)[0], field
        assert key not in keys, field
        keys.add(key)


def test_document_request_key_ignores_delivery_settings() -> None:
    base_key = document_generator.document_request_key(document_request())
    for overrides in (
        {"email_address": "someone@example.com"},
        {"generate_pdf": False},
        {"generate_epub": True},
        {"generate_docx": True},
        {"document_request_source": model.DocumentRequestSourceEnum.UI},
    ):
        assert (
            document_generator.document_request_key(
                document_request(overrides=overrides)
            )
            == base_key
        ), overrides


def test_document_request_key_limit_words_only_matters_with_tw() -> None:
    without_tw = [("en", "ulb", "mat"), ("en", "tn", "mat")]
    assert document_generator.document_request_key(
        document_request(without_tw, {"limit_words": True})
    ) == document_generator.document_request_key(
        document_request(without_tw, {"limit_words": False})
    )
    assert document_generator.document_request_key(
        document_request(overrides={"limit_words": True})
    ) != document_generator.document_request_key(
        document_request(overrides={"limit_words": False})
    )


def test_document_request_key_depends_on_resources_and_their_order() -> None:
    first = [("en", "ulb", "mat"), ("fr", "ulb", "mat")]
    second = [("fr", "ulb", "mat"), ("en", "ulb", "mat")]
    assert document_generator.document_request_key(
        document_request(first)
    ) != document_generator.document_request_key(document_request(second))
    assert document_generator.document_request_key(
        document_request(first)
    ) != document_generator.document_request_key(
        document_request([("en", "ulb", "mrk"), ("fr", "ulb", "mrk")])
    )


def test_document_request_key_is_short_and_deterministic_for_many_resources() -> None:
    """
    Use enough resource requests that the readable slug has to be
    summarized. The key must still be short, filename safe, and stable
    (previously a timestamp, and so never a cache hit, was used).
    """
    components = [
        ("bdf", "reg", "mat"),
        ("bdf", "reg", "mrk"),
        ("pt-br", "ulb", "mat"),
        ("pt-br", "tw", "mat"),
        ("pt-br", "tq", "mat"),
        ("pt-br", "tn", "mat"),
        ("pt-br", "ulb", "mrk"),
        ("pt-br", "tw", "mrk"),
        ("pt-br", "tq", "mrk"),
        ("pt-br", "tn", "mrk"),
        ("fr", "ulb", "mat"),
        ("fr", "tw", "mat"),
        ("fr", "tq", "mat"),
        ("fr", "tn", "mat"),
        ("fr", "f10", "mat"),
        ("fr", "ulb", "mrk"),
        ("fr", "tw", "mrk"),
        ("fr", "tq", "mrk"),
        ("fr", "tn", "mrk"),
        ("fr", "f10", "mrk"),
    ] + [("fr", "ulb", book_code) for book_code in list(BOOK_NAMES)[:30]]
    key = document_generator.document_request_key(document_request(components))
    assert KEY_PATTERN.match(key)
    assert len(key) < 100
    assert "books" in key
    assert key == document_generator.document_request_key(document_request(components))


def test_document_request_key_is_filename_safe_for_odd_lang_codes() -> None:
    key = document_generator.document_request_key(
        document_request([("../../Etc/passwd", "ulb", "mat")])
    )
    assert KEY_PATTERN.match(key)
    assert "/" not in key and ".." not in key
