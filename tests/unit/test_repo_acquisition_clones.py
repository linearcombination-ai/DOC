"""Repo acquisition is by git clone only; batch download no longer exists."""

from pathlib import Path

import pytest
from doc.config import Settings
from doc.domain import resource_lookup
from pydantic import HttpUrl


def _patch_lookup(
    monkeypatch: pytest.MonkeyPatch, cloned: list[list[tuple[HttpUrl, str]]]
) -> None:
    class _Data:
        git_repo: list[object] = [object()]

    url = HttpUrl("https://content.bibletranslationtools.org/WycliffeAssociates/en_ulb")
    monkeypatch.setattr(resource_lookup, "fetch_source_data", lambda: _Data())
    monkeypatch.setattr(
        resource_lookup, "filter_excluded_resource_types", lambda repos: repos
    )
    monkeypatch.setattr(
        resource_lookup, "add_data_not_supplied_by_data_api", lambda repos: repos
    )
    monkeypatch.setattr(
        resource_lookup,
        "repos_to_clone",
        lambda lang_code, repos: [(url, "/tmp/en_ulb", "ulb"), (url, "/tmp/rg", "rg")],
    )
    monkeypatch.setattr(
        resource_lookup, "batch_clone_git_repos", lambda repos: cloned.append(repos)
    )
    monkeypatch.setattr(resource_lookup, "get_resource_types", lambda *a, **k: [])
    monkeypatch.setattr(resource_lookup, "get_book_codes_for_lang_", lambda *a, **k: [])


def test_batch_download_repos_removed() -> None:
    assert not hasattr(resource_lookup, "batch_download_repos")


def test_resource_types_clones_repos(monkeypatch: pytest.MonkeyPatch) -> None:
    cloned: list[list[tuple[HttpUrl, str]]] = []
    _patch_lookup(monkeypatch, cloned)
    resource_lookup.resource_types("en", "gen")
    assert len(cloned) == 1
    assert [path for _, path in cloned[0]] == ["/tmp/en_ulb"]


def test_get_book_codes_for_lang_clones_repos(monkeypatch: pytest.MonkeyPatch) -> None:
    cloned: list[list[tuple[HttpUrl, str]]] = []
    _patch_lookup(monkeypatch, cloned)
    resource_lookup.get_book_codes_for_lang("en")
    assert len(cloned) == 1
    assert [path for _, path in cloned[0]] == ["/tmp/en_ulb"]


def test_leftover_download_assets_setting_is_tolerated(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """Deployed .env files or environments may still set DOWNLOAD_ASSETS."""
    env_file = tmp_path / ".env"
    env_file.write_text(Path(".env").read_text() + "\nDOWNLOAD_ASSETS=true\n")
    monkeypatch.setenv("DOWNLOAD_ASSETS", "true")
    loaded = Settings(_env_file=str(env_file))  # type: ignore[call-arg]
    assert not hasattr(loaded, "DOWNLOAD_ASSETS")
