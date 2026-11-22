"""
Clean handling of Celery's soft time limit for the document generation tasks.

When a task exceeds ``task_soft_time_limit``, Celery raises
``SoftTimeLimitExceeded`` inside it. Left unhandled, a half-written output
file (for instance a truncated PDF) could later be served from the file cache
because it exists and is non-empty. The context manager here removes partial
outputs and turns the exception into a failure with a clear message.
"""

import os
from contextlib import contextmanager
from typing import Iterable, Iterator

from celery.exceptions import SoftTimeLimitExceeded
from doc.config import settings
from doc.utils.file_utils import file_needs_update

logger = settings.logger(__name__)


class DocumentGenerationTimeoutError(Exception):
    """The document could not be generated within the allotted time."""


@contextmanager
def fail_cleanly_on_soft_time_limit(output_filepaths: Iterable[str]) -> Iterator[None]:
    """
    Run the enclosed task body. If Celery's soft time limit fires, delete any
    of ``output_filepaths`` that this run was going to (re)build, then raise
    DocumentGenerationTimeoutError with a clear message so the task fails
    cleanly instead of leaving partial outputs behind.

    Only files that were missing or stale on entry are removed; a fresh cached
    output that this run did not touch is left alone.
    """
    stale_filepaths = [
        filepath for filepath in output_filepaths if file_needs_update(filepath)
    ]
    try:
        yield
    except SoftTimeLimitExceeded as exc:
        logger.error(
            "Task exceeded its soft time limit; removing partial outputs: %s",
            stale_filepaths,
        )
        for filepath in stale_filepaths:
            try:
                os.remove(filepath)
            except FileNotFoundError:
                pass
            except OSError:
                logger.exception("Could not remove partial output %s", filepath)
        raise DocumentGenerationTimeoutError(
            "Document generation took too long and was stopped. "
            "Please try again, or request fewer books or resources."
        ) from exc
