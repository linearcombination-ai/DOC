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

    # Bounds on external calls. All values are env-tunable and are kept
    # well below the Celery soft time limit (assumed to be 20 minutes =
    # 1200 seconds) so that Python kills a stalled child process itself
    # rather than leaving it orphaned when Celery kills the pool child.
    HTTP_CONNECT_TIMEOUT_SECONDS: float = 10.0
    HTTP_READ_TIMEOUT_SECONDS: float = 60.0
    HTTP_RETRY_TOTAL: int = 3
    HTTP_RETRY_BACKOFF_FACTOR: float = 0.5
    # Per-attempt limit for one git clone or one curl download.
    GIT_CLONE_TIMEOUT_SECONDS: int = 300
    # Overall ceiling for a clone including retries and waits between
    # attempts when failures are transient.
    GIT_CLONE_RETRY_CEILING_SECONDS: int = 600
    GIT_CLONE_RETRY_WAIT_SECONDS: float = 5.0
    DOWNLOAD_TIMEOUT_SECONDS: int = 300
    # Overall ceiling for one batch of downloads plus unzips.
    DOWNLOAD_BATCH_CEILING_SECONDS: int = 1000
    # Limit for weasyprint/prince/ebook-convert subprocesses.
    CONVERTER_TIMEOUT_SECONDS: int = 900
    SMTP_TIMEOUT_SECONDS: float = 30.0

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
