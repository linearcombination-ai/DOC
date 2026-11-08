"""This module provides configuration values used by the application."""

import logging
from functools import lru_cache
from logging import config as lc
from pathlib import Path
from typing import final

import yaml
from pydantic import EmailStr, HttpUrl
from pydantic_settings import BaseSettings, SettingsConfigDict

# The .env file and the logging config are located relative to this file,
# not to the current working directory, so that Settings can be built from
# any working directory (tests, scripts, a different launch directory).
# This file lives at <project root>/backend/doc/config.py.
BACKEND_DIR = Path(__file__).resolve().parents[1]
PROJECT_ROOT = BACKEND_DIR.parent
ENV_FILE_PATH = PROJECT_ROOT / ".env"
DEFAULT_LOGGING_CONFIG_FILE_PATH = BACKEND_DIR / "logging_config.yaml"


@final
class Settings(BaseSettings):
    """
    BaseSettings subclasses like this one allow values of constants to
    be overridden by environment variables like those defined in env
    files, e.g., ../../.env or by system level environment variables
    (which have higher priority).
    """

    DATA_API_URL: HttpUrl

    SHOW_TN_BOOK_INTRO: bool = True

    CHECK_USFM: bool
    USE_LOCALIZED_BOOK_NAME: bool
    CHECK_ALL_BOOKS_FOR_LANGUAGE: bool

    LINK_RATHER_THAN_INCLUDE_TW_DEFINITIONS: bool = True

    DOWNLOAD_ASSETS: bool = False  # If true then download assets, else clone assets

    # Not currently read by any code, hence the default.
    LOCK_TIMEOUT_SECONDS: int = 300

    USER_AGENT_STR: str
    X_REQUESTED_WITH_VALUE: str

    DATA_API_CACHE_TTL_SECONDS: int

    def logger(self, name: str) -> logging.Logger:
        """
        Return a Logger for scope named by name, e.g., module, that can be
        used for logging.
        """
        with open(self.logging_config_file_path(), "r") as fin:
            logging_config = yaml.safe_load(fin.read())
            lc.dictConfig(logging_config)
        return logging.getLogger(name)

    def logging_config_file_path(self) -> Path:
        """
        Return the absolute path of the logging config file. A relative
        LOGGING_CONFIG_FILE_PATH is interpreted relative to the project
        root rather than the current working directory.
        """
        path = Path(self.LOGGING_CONFIG_FILE_PATH)
        return path if path.is_absolute() else PROJECT_ROOT / path

    def api_test_url(self) -> str:
        """Non-secure local URL for running the Fastapi server for testing."""
        return "{}:{}".format(self.API_TEST_BASE_URL, self.API_LOCAL_PORT)

    API_TEST_BASE_URL: str = "http://localhost"

    API_LOCAL_PORT: int

    USE_GIT_CLI: bool = False

    # Relative paths are relative to the project root.
    LOGGING_CONFIG_FILE_PATH: str = str(DEFAULT_LOGGING_CONFIG_FILE_PATH)

    # Location where resource assets will be cloned.
    RESOURCE_ASSETS_DIR: str = "assets_download"
    # Optional override of the Reviewer's Guide .docx directory. When unset
    # the passages package supplies it, see passages.domain.survey_files.
    EN_RG_DIR: str | None = None

    # Location where intermediate generated document parts are saved.
    WORKING_DIR: str = "working_temp"

    # Location where generated PDFs are written.
    DOCUMENT_OUTPUT_DIR: str = "document_output"

    # Location where stet source Docx document(s) are stored
    STET_DIR: str = "stet"

    BACKEND_CORS_ORIGINS: list[str]

    DOCX_TEMPLATE_PATH: str = "template.docx"
    DOCX_COMPACT_TEMPLATE_PATH: str = "template_compact.docx"

    # Indicate if generated documents should be cached.
    # default to False for dev, prod will set via env var
    ASSET_CACHING_ENABLED: bool = False

    # Caching window of time in which asset
    # files on disk are considered fresh rather than re-acquiring (in
    # the case of resource asset files) or re-generating them (in the
    # case of the final PDF). In minutes.
    ASSET_CACHING_PERIOD: int

    EMAIL_SEND_SUBJECT: str
    TO_EMAIL_ADDRESS: EmailStr

    # Provided by system env vars (fake values provided so github action can run):
    FROM_EMAIL_ADDRESS: EmailStr = "foo@example.com"
    SMTP_HOST: str = "https://example.com"
    SMTP_PORT: int = 111
    SMTP_PASSWORD: str = "fakepass"
    SEND_EMAIL: bool = False

    # Used by gunicorn, which reads the PORT environment variable itself,
    # so no Python code reads this field, hence the default.
    PORT: int = 80
    # Used by docker compose, which reads the environment variable itself,
    # so no Python code reads this field, hence the default.
    IMAGE_TAG: str = "local"

    model_config = SettingsConfigDict(
        env_file=ENV_FILE_PATH, case_sensitive=True, frozen=True
    )


@lru_cache(maxsize=1)
def get_settings() -> Settings:
    """
    Return the application's Settings.

    Settings are read from the environment and the .env file once, on the
    first call, and the same object is returned thereafter. Each process
    (e.g., the API and the Celery worker) reads its own settings, they are
    never passed between processes.

    Read settings inside the function that needs them, not at module level
    and not as a parameter default, as those are evaluated at import time
    and so can not be changed afterward. To make a test see different
    settings, change the environment and call get_settings.cache_clear().
    """
    # mypy with pydantic v2 doesn't understand that required fields will
    # be picked up from the environment or the .env file, thus the type
    # ignore directive
    return Settings()  # type: ignore[call-arg]
