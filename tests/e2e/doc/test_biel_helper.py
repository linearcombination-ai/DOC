from doc.config import get_settings
from doc.entrypoints.app import app
from fastapi.testclient import TestClient


def test_get_language_codes_and_names_returns_ok() -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.get("/language_codes_and_names")
        assert response.status_code == 200, response.text
        assert response.json()
