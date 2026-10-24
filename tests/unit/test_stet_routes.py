from typing import Iterator

import pytest
from doc.config import settings
from fastapi import FastAPI
from fastapi.testclient import TestClient
from stet.domain import resource_lookup
from stet.entrypoints import routes

OVERRIDE_STET_DIR = "/override/stet_dir"


@pytest.fixture
def scanned_dirs(monkeypatch: pytest.MonkeyPatch) -> list[str]:
    """Record the directories the handler scans instead of touching disk."""
    dirs: list[str] = []

    def fake_scandir(path: str) -> Iterator[object]:
        dirs.append(path)
        return iter(())

    monkeypatch.setattr(routes, "scandir", fake_scandir)
    monkeypatch.setattr(
        resource_lookup, "lang_codes_and_names_having_usfm", lambda: []
    )
    return dirs


@pytest.fixture
def app() -> FastAPI:
    app = FastAPI()
    app.include_router(routes.router)
    return app


def test_get_stet_dir_reads_settings() -> None:
    assert routes.get_stet_dir() == settings.STET_DIR


def test_get_production_lang_codes() -> None:
    assert routes.get_production_lang_codes() == ("en", "es-419", "pt-br")


@pytest.mark.stet
def test_source_languages_uses_provided_stet_dir(
    app: FastAPI, scanned_dirs: list[str]
) -> None:
    app.dependency_overrides[routes.get_stet_dir] = lambda: OVERRIDE_STET_DIR
    response = TestClient(app).get("/stet/source_languages")
    assert response.status_code == 200
    assert scanned_dirs == [OVERRIDE_STET_DIR]


@pytest.mark.stet
def test_source_languages_uses_settings_stet_dir_by_default(
    app: FastAPI, scanned_dirs: list[str]
) -> None:
    response = TestClient(app).get("/stet/source_languages")
    assert response.status_code == 200
    assert scanned_dirs == [settings.STET_DIR]


@pytest.mark.stet
def test_source_languages_ignores_client_supplied_params(
    app: FastAPI, scanned_dirs: list[str]
) -> None:
    app.dependency_overrides[routes.get_stet_dir] = lambda: OVERRIDE_STET_DIR
    client = TestClient(app)
    attacker_dir = "/tmp/attacker_controlled"
    response = client.get(
        "/stet/source_languages",
        params={"stet_dir": attacker_dir, "production_lang_codes": ["xx"]},
    )
    assert response.status_code == 200
    response = client.request(
        "GET",
        "/stet/source_languages",
        json={"stet_dir": attacker_dir, "production_lang_codes": ["xx"]},
    )
    assert response.status_code == 200
    assert scanned_dirs == [OVERRIDE_STET_DIR, OVERRIDE_STET_DIR]


@pytest.mark.stet
def test_source_languages_declares_no_client_params(app: FastAPI) -> None:
    operation = app.openapi()["paths"]["/stet/source_languages"]["get"]
    assert "parameters" not in operation
    assert "requestBody" not in operation
