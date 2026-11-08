from fastapi.testclient import TestClient
import pytest
from doc.config import settings
from doc.entrypoints.app import app


def test_chapters_in_books() -> None:
    with TestClient(app=app, base_url=settings.api_test_url()) as client:
        response = client.get("/chapters_in_books")
        assert response.status_code == 200


def test_resource_availability_for_language_with_notes_and_scripture() -> None:
    with TestClient(app=app, base_url=settings.api_test_url()) as client:
        response = client.get("/resource_availability/es-419")
        assert response.status_code == 200
        availability = response.json()
        assert availability["lang_code"] == "es-419"
        resource_type_codes = [t["code"] for t in availability["resource_types"]]
        for expected in ("ulb", "tn", "tq", "tw"):
            assert expected in resource_type_codes
        scopes = {t["code"]: t["scope"] for t in availability["resource_types"]}
        assert scopes["tw"] == "language"
        assert scopes["ulb"] == "book"
        assert availability["language_resources"] == ["tw"]
        mat = next(b for b in availability["books"] if b["code"] == "mat")
        assert mat["testament"] == "NT"
        assert {"ulb", "tn", "tq"} <= set(mat["resources"])
        assert all("tw" not in b["resources"] for b in availability["books"])


def test_resource_availability_does_not_offer_resources_that_do_not_exist() -> None:
    # Russian has ULB for Old Testament books only but TN and TQ for every
    # book, so ULB must not be offered for New Testament books.
    with TestClient(app=app, base_url=settings.api_test_url()) as client:
        response = client.get("/resource_availability/ru")
        assert response.status_code == 200
        books = {b["code"]: b["resources"] for b in response.json()["books"]}
        assert "ulb" in books["gen"]
        assert "ulb" not in books["mat"]
        assert "tn" in books["mat"]


def test_resource_availability_for_unknown_language_is_empty() -> None:
    with TestClient(app=app, base_url=settings.api_test_url()) as client:
        response = client.get("/resource_availability/not-a-language")
        assert response.status_code == 200
        availability = response.json()
        assert availability["books"] == []
        assert availability["resource_types"] == []
