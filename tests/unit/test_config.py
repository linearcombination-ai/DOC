"""Tests for the cached settings accessor and how settings are located."""

from pathlib import Path
from typing import Iterator

import pytest
from doc import config
from doc.config import Settings, get_settings
from doc.utils import file_utils


@pytest.fixture(autouse=True)
def fresh_settings() -> Iterator[None]:
    """Never leak a settings object built under a test's environment."""
    get_settings.cache_clear()
    yield
    get_settings.cache_clear()


def test_repeated_calls_return_the_same_object() -> None:
    assert get_settings() is get_settings()


def test_settings_are_built_once(monkeypatch: pytest.MonkeyPatch) -> None:
    constructed: list[Settings] = []

    def counting_settings() -> Settings:
        settings = Settings()  # type: ignore[call-arg]
        constructed.append(settings)
        return settings

    monkeypatch.setattr(config, "Settings", counting_settings)
    first = get_settings()
    for _ in range(5):
        assert get_settings() is first
    assert constructed == [first]


def test_cache_clear_rebuilds_settings_from_the_environment(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    before = get_settings()
    monkeypatch.setenv("DOCUMENT_OUTPUT_DIR", "/overridden/output")
    # The cached object is untouched by the environment change.
    assert get_settings() is before
    get_settings.cache_clear()
    after = get_settings()
    assert after is not before
    assert after.DOCUMENT_OUTPUT_DIR == "/overridden/output"


def test_functions_read_settings_when_called_not_when_imported(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setenv("DOCUMENT_OUTPUT_DIR", "/overridden/output")
    get_settings.cache_clear()
    assert file_utils.html_filepath("key") == "/overridden/output/key.html"
    assert file_utils.pdf_filepath("key") == "/overridden/output/key.pdf"
    assert file_utils.epub_filepath("key") == "/overridden/output/key.epub"
    assert file_utils.docx_filepath("key", "p_") == "/overridden/output/p_key.docx"


def test_file_needs_update_follows_the_caching_setting(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    fresh_file = tmp_path / "asset.txt"
    fresh_file.write_text("content")
    monkeypatch.setenv("ASSET_CACHING_ENABLED", "true")
    monkeypatch.setenv("ASSET_CACHING_PERIOD", "60")
    get_settings.cache_clear()
    assert not file_utils.file_needs_update(fresh_file)
    monkeypatch.setenv("ASSET_CACHING_ENABLED", "false")
    get_settings.cache_clear()
    assert file_utils.file_needs_update(fresh_file)


def test_settings_do_not_depend_on_the_working_directory(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    expected = get_settings()
    get_settings.cache_clear()
    monkeypatch.chdir(tmp_path)
    from_elsewhere = get_settings()
    assert from_elsewhere == expected
    assert config.ENV_FILE_PATH.is_absolute()
    assert config.ENV_FILE_PATH.name == ".env"
    assert config.ENV_FILE_PATH.parent == config.PROJECT_ROOT


def test_logging_config_path_is_anchored_not_cwd_relative(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.chdir(tmp_path)
    # Whether the value is the default or a project root relative path such
    # as the one in .env, it resolves to the real file from any directory.
    assert get_settings().logging_config_file_path().is_file()
    monkeypatch.setenv("LOGGING_CONFIG_FILE_PATH", "backend/logging_config.yaml")
    get_settings.cache_clear()
    path = get_settings().logging_config_file_path()
    assert path.is_absolute()
    assert path == config.BACKEND_DIR / "logging_config.yaml"
    assert path.is_file()


def test_never_read_required_fields_have_defaults() -> None:
    for name in ("LOCK_TIMEOUT_SECONDS", "PORT", "IMAGE_TAG"):
        assert not Settings.model_fields[name].is_required(), name
