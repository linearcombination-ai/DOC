"""
Tests of the (book, resource type) availability computation. These
build tiny fake resource repos on disk and don't touch the network.
"""

from pathlib import Path
from typing import Sequence, TypedDict

import pytest
from doc.config import settings
from doc.domain import resource_availability as ra
from doc.domain import resource_lookup
from doc.domain.model import (
    Content,
    LangDirEnum,
    Language,
    RepoEntry,
    ResourceAvailability,
    SourceData,
)
from doc.entrypoints.app import app
from fastapi.testclient import TestClient
from pydantic import HttpUrl

BASE_URL = "https://content.bibletranslationtools.org/WA-Catalog"


class FakeEnvironment(TypedDict):
    clones: int
    fetches: int
    empty: bool


def url(name: str) -> HttpUrl:
    return HttpUrl(f"{BASE_URL}/{name}")


def make_dir(path: Path, *children: str) -> str:
    path.mkdir(parents=True, exist_ok=True)
    for child in children:
        child_path = path / child
        if child.endswith((".usfm", ".txt", ".md")):
            child_path.parent.mkdir(parents=True, exist_ok=True)
            child_path.write_text("\\id GEN\n")
        else:
            child_path.mkdir(parents=True, exist_ok=True)
    return str(path)


def books_of(availability: ResourceAvailability, resource_type: str) -> list[str]:
    return [book.code for book in availability.books if resource_type in book.resources]


def test_resource_category() -> None:
    assert ra.resource_category("ulb") == "scripture"
    assert ra.resource_category("reg") == "scripture"
    assert ra.resource_category("blv") == "scripture"
    assert ra.resource_category("tn") == "notes"
    assert ra.resource_category("tn-condensed") == "notes"
    assert ra.resource_category("tq") == "questions"
    assert ra.resource_category("bc") == "commentary"
    assert ra.resource_category("tw") == "words"


def test_scan_whole_language_repos(tmp_path: Path) -> None:
    tn = make_dir(tmp_path / "xx_tn", "gen", "mat", ".git", "front")
    tq = make_dir(tmp_path / "xx_tq", "mat")
    ulb = make_dir(
        tmp_path / "xx_ulb", "01-GEN.usfm", "41-MAT.usfm", "00-ULB_front&back"
    )
    tw = make_dir(tmp_path / "xx_tw", "bible")
    bc = make_dir(tmp_path / "xx_bc", "41-mat", "42-mrk", "articles")
    repo_clone_list = [
        (url("xx_tn"), tn, "tn"),
        (url("xx_tq"), tq, "tq"),
        (url("xx_ulb"), ulb, "ulb"),
        (url("xx_tw"), tw, "tw"),
        (url("xx_bc"), bc, "bc"),
    ]
    book_scoped, language_wide = ra.scan_repos("xx", repo_clone_list)
    assert book_scoped == {
        "tn": {"gen", "mat"},
        "tq": {"mat"},
        "ulb": {"gen", "mat"},
        "bc": {"mat", "mrk"},
    }
    assert language_wide == {"tw"}


def test_scan_per_book_repos_use_repo_name_for_book(tmp_path: Path) -> None:
    gen = make_dir(tmp_path / "xx_gen_text_ulb", "01-GEN.usfm")
    mat = make_dir(tmp_path / "xx_mat_text_ulb", "41-MAT.usfm")
    empty = make_dir(tmp_path / "xx_rev_text_ulb")  # cloned, but no USFM
    repo_clone_list = [
        (url("xx_gen_text_ulb"), gen, "ulb"),
        (url("xx_mat_text_ulb"), mat, "ulb"),
        (url("xx_rev_text_ulb"), empty, "ulb"),
    ]
    book_scoped, language_wide = ra.scan_repos("xx", repo_clone_list)
    assert book_scoped == {"ulb": {"gen", "mat"}}
    assert language_wide == set()


def test_scan_ignores_repos_that_were_not_provisioned(tmp_path: Path) -> None:
    repo_clone_list = [(url("xx_tn"), str(tmp_path / "does_not_exist"), "tn")]
    assert ra.scan_repos("xx", repo_clone_list) == ({}, set())


def test_scan_whole_language_usfm_with_txt_suffix_in_book_directories(
    tmp_path: Path,
) -> None:
    ulb = make_dir(tmp_path / "xx_ulb", "mat/01.txt", "mrk/01.txt")
    book_scoped, _ = ra.scan_repos("xx", [(url("xx_ulb"), ulb, "ulb")])
    assert book_scoped == {"ulb": {"mat", "mrk"}}


def test_build_is_sparse_ordered_and_scoped() -> None:
    availability = ra.build_resource_availability(
        "xx",
        {
            "ulb": {"gen", "rev"},  # no NT book between gen and rev
            "tn": {"rev", "gen", "mat"},
            "bc": {"mat"},
        },
        {"tw"},
        {"gen": "Génesis"},
    )
    assert availability.lang_code == "xx"
    # Types ordered by display name: Bible Commentary, Translation Notes,
    # Translation Words, Unlocked Literal Bible
    assert [t.code for t in availability.resource_types] == ["bc", "tn", "tw", "ulb"]
    scopes = {t.code: t.scope for t in availability.resource_types}
    assert scopes == {
        "bc": "book",
        "tn": "book",
        "tw": "language",
        "ulb": "book",
    }
    # Books in canonical order
    assert [b.code for b in availability.books] == ["gen", "mat", "rev"]
    assert availability.books[0].name == "Génesis"  # localized
    assert availability.books[1].name == "Matthew"  # English fallback
    assert [b.testament for b in availability.books] == ["OT", "NT", "NT"]
    assert availability.books[1].resources == ["bc", "tn"]  # no ulb for mat
    assert books_of(availability, "ulb") == ["gen", "rev"]
    assert availability.language_resources == ["tw"]
    # tw never shows up as a per book resource
    assert all("tw" not in book.resources for book in availability.books)


def test_build_language_with_only_regular_scripture() -> None:
    availability = ra.build_resource_availability("xx", {"reg": {"jhn"}}, set(), {})
    assert [t.code for t in availability.resource_types] == ["reg"]
    assert availability.resource_types[0].category == "scripture"
    assert availability.language_resources == []


@pytest.fixture
def fake_environment(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> FakeEnvironment:
    """
    Language xx has whole language tn and tw and ulb for only gen, via a
    per book repo. Nothing touches the network or git.
    """
    ra.clear_availability_cache()
    state: FakeEnvironment = {"clones": 0, "fetches": 0, "empty": False}
    language = Language(
        english_name="Xx", ietf_code="xx", national_name="Xx", direction=LangDirEnum.LTR
    )
    entries = [
        RepoEntry(
            repo_url=url(name), content=Content(resource_type=rtype, language=language)
        )
        for name, rtype in (
            ("xx_tn", "tn"),
            ("xx_tw", "tw"),
            ("xx_gen_text_ulb", "ulb"),
        )
    ]
    paths = {
        "xx_tn": make_dir(tmp_path / "xx_tn", "gen", "mat"),
        "xx_tw": make_dir(tmp_path / "xx_tw", "bible"),
        "xx_gen_text_ulb": make_dir(tmp_path / "xx_gen_text_ulb", "01-GEN.usfm"),
    }

    def fake_fetch_source_data() -> SourceData:
        state["fetches"] += 1
        return SourceData(git_repo=[] if state["empty"] else entries)

    def fake_repos_to_clone(
        lang_code: str, repos: Sequence[RepoEntry]
    ) -> list[tuple[HttpUrl, str, str]]:
        return [
            (
                e.repo_url,
                paths[str(e.repo_url).rsplit("/", 1)[-1]],
                e.content.resource_type,
            )
            for e in repos
            if e.content.language.ietf_code == lang_code
            and str(e.repo_url).rsplit("/", 1)[-1] in paths
        ]

    def fake_clone(
        repos: Sequence[tuple[HttpUrl, str]], *args: object, **kwargs: object
    ) -> None:
        state["clones"] += 1

    monkeypatch.setattr(resource_lookup, "fetch_source_data", fake_fetch_source_data)
    monkeypatch.setattr(resource_lookup, "repos_to_clone", fake_repos_to_clone)
    monkeypatch.setattr(resource_lookup, "batch_clone_git_repos", fake_clone)
    monkeypatch.setattr(
        resource_lookup,
        "add_data_not_supplied_by_data_api",
        lambda repos_info: repos_info,
    )
    return state


def test_resource_availability_end_to_end(fake_environment: FakeEnvironment) -> None:
    availability = ra.resource_availability("xx", asset_caching_enabled=True)
    assert [t.code for t in availability.resource_types] == ["tn", "tw", "ulb"]
    assert books_of(availability, "tn") == ["gen", "mat"]
    assert books_of(availability, "ulb") == ["gen"]
    assert availability.language_resources == ["tw"]


def test_resource_availability_is_cached_per_language(
    fake_environment: FakeEnvironment,
) -> None:
    first = ra.resource_availability("xx", asset_caching_enabled=True)
    second = ra.resource_availability("xx", asset_caching_enabled=True)
    assert first == second
    assert fake_environment["clones"] == 1


def test_resource_availability_not_cached_when_asset_caching_disabled(
    fake_environment: FakeEnvironment,
) -> None:
    ra.resource_availability("xx", asset_caching_enabled=False)
    ra.resource_availability("xx", asset_caching_enabled=False)
    assert fake_environment["clones"] == 2


def test_resource_availability_failures_are_not_cached(
    fake_environment: FakeEnvironment,
) -> None:
    fake_environment["empty"] = True  # data API down
    availability = ra.resource_availability("xx", asset_caching_enabled=True)
    assert availability.books == [] and availability.resource_types == []
    fake_environment["empty"] = False
    availability = ra.resource_availability("xx", asset_caching_enabled=True)
    assert availability.books  # recovered, wasn't stuck on the failure


def test_unknown_language_is_empty(fake_environment: FakeEnvironment) -> None:
    availability = ra.resource_availability("nope", asset_caching_enabled=True)
    assert availability.books == []
    assert availability.resource_types == []
    assert availability.language_resources == []


def test_resource_availability_route(fake_environment: FakeEnvironment) -> None:
    with TestClient(app=app, base_url=settings.api_test_url()) as client:
        response = client.get("/resource_availability/xx")
    assert response.status_code == 200
    body = response.json()
    assert body["lang_code"] == "xx"
    assert body["language_resources"] == ["tw"]
    assert [t["code"] for t in body["resource_types"]] == ["tn", "tw", "ulb"]
    gen = next(book for book in body["books"] if book["code"] == "gen")
    assert gen == {
        "code": "gen",
        "name": "Genesis",
        "testament": "OT",
        "resources": ["tn", "ulb"],
    }
    mat = next(book for book in body["books"] if book["code"] == "mat")
    assert mat["resources"] == ["tn"]  # the sparse case: no ulb for mat
