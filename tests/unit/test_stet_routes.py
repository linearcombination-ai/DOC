from typing import Iterator

import pytest
from doc.config import settings
from fastapi import FastAPI
from fastapi.testclient import TestClient
from stet.domain import resource_lookup
from stet.entrypoints import routes


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
def client() -> TestClient:
    app = FastAPI()
    app.include_router(routes.router)
    return TestClient(app)


@pytest.mark.stet
def test_source_languages_ignores_client_supplied_params(
    client: TestClient, scanned_dirs: list[str]
) -> None:
    attacker_dir = "/tmp/attacker_controlled"
    response = client.get(
        "/stet/source_languages",
        params={"stet_dir": attacker_dir, "production_lang_codes": ["xx"]},
    )
    assert response.status_code == 200
    assert scanned_dirs == [settings.STET_DIR]

    response = client.request(
        "GET",
        "/stet/source_languages",
        json={"stet_dir": attacker_dir, "production_lang_codes": ["xx"]},
    )
    assert response.status_code == 200
    assert scanned_dirs == [settings.STET_DIR, settings.STET_DIR]


@pytest.mark.stet
def test_source_languages_declares_no_client_params() -> None:
    app = FastAPI()
    app.include_router(routes.router)
    operation = app.openapi()["paths"]["/stet/source_languages"]["get"]
    assert "parameters" not in operation
    assert "requestBody" not in operation


def test_production_lang_codes_is_immutable() -> None:
    assert routes.PRODUCTION_LANG_CODES == ("en", "es-419", "pt-br")
