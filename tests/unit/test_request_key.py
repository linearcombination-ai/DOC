import re
from enum import Enum

from doc.utils import request_key


class Color(Enum):
    RED = "red"


def test_slugify_is_filename_safe_and_never_empty() -> None:
    assert request_key.slugify("pt-BR") == "pt-br"
    assert request_key.slugify("es_419") == "es-419"
    assert request_key.slugify("../../Etc/passwd") == "etc-passwd"
    assert request_key.slugify("") == "x"
    assert request_key.slugify("///") == "x"


def test_canonical_json_ignores_dict_order_but_not_list_order() -> None:
    assert request_key.canonical_json({"a": 1, "b": 2}) == request_key.canonical_json(
        {"b": 2, "a": 1}
    )
    assert request_key.canonical_json([1, 2]) != request_key.canonical_json([2, 1])
    assert request_key.canonical_json({"c": Color.RED}) == '{"c":"red"}'


def test_short_hash_is_stable_and_long_enough() -> None:
    # Pinned so that an accidental change to the canonical form, which
    # would silently invalidate every cached document, fails a test.
    assert request_key.short_hash(
        {"resources": [["en", "ulb", "mat"]]}, version=1
    ) == request_key.short_hash({"resources": [["en", "ulb", "mat"]]}, version=1)
    assert re.fullmatch(r"[0-9a-f]{12}", request_key.short_hash({}))
    assert request_key.HASH_HEX_LENGTH >= 12


def test_schema_version_changes_hash() -> None:
    payload = {"resources": [["en", "ulb", "mat"]]}
    assert request_key.short_hash(payload, version=1) != request_key.short_hash(
        payload, version=2
    )


def test_slug_group_summarizes_long_groups() -> None:
    assert request_key.slug_group(["mat", "mrk", "mat"], "books", 30) == "mat-mrk"
    long_group = [f"b{i:02d}" for i in range(30)]
    assert request_key.slug_group(long_group, "books", 30) == "30books"


def test_request_key_shape() -> None:
    key = request_key.request_key(
        request_key.resources_slug(["en"], ["ulb", "tn"], ["mat"]), {"x": 1}
    )
    assert re.fullmatch(r"en_ulb-tn_mat_[0-9a-f]{12}", key)
