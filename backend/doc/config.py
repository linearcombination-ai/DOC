"""This module provides configuration values used by the application."""

import logging
from logging import config as lc
from typing import final

import yaml
from pydantic import EmailStr, HttpUrl
from pydantic_settings import BaseSettings, SettingsConfigDict


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

    LOCK_TIMEOUT_SECONDS: int

    USER_AGENT_STR: str
    X_REQUESTED_WITH_VALUE: str

    DATA_API_CACHE_TTL_SECONDS: int

    def logger(self, name: str) -> logging.Logger:
        """
        Return a Logger for scope named by name, e.g., module, that can be
        used for logging.
        """
        with open(self.LOGGING_CONFIG_FILE_PATH, "r") as fin:
            logging_config = yaml.safe_load(fin.read())
            lc.dictConfig(logging_config)
        return logging.getLogger(name)

    def api_test_url(self) -> str:
        """Non-secure local URL for running the Fastapi server for testing."""
        return "{}:{}".format(self.API_TEST_BASE_URL, self.API_LOCAL_PORT)

    API_TEST_BASE_URL: str = "http://localhost"

    API_LOCAL_PORT: int

    USE_GIT_CLI: bool = False

    LOGGING_CONFIG_FILE_PATH: str = "backend/logging_config.yaml"

    # Location where resource assets will be cloned.
    RESOURCE_ASSETS_DIR: str = "assets_download"
    EN_RG_DIR: str = "en_rg"

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

    # Used by gunicorn
    PORT: int
    # Used by docker
    IMAGE_TAG: str

    model_config = SettingsConfigDict(env_file=".env", case_sensitive=True, frozen=True)


# mypy with pydantic v2 doesn't understand that defaults will be
# picked up from .env file as they had been in v1, thus the type
# ignore directive
settings = Settings()  # type: ignore
# Could also use:
# settings = Settings(_env_file=".env", _env_file_encoding="utf-8")
