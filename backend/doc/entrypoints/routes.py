from typing import Sequence, cast

import celery.states
from celery.result import AsyncResult
from doc.config import settings
from doc.domain import (
    document_generator,
    model,
    resource_availability as resource_availability_,
    resource_lookup,
)
from fastapi import APIRouter, HTTPException, status
from fastapi.responses import JSONResponse

router = APIRouter()

logger = settings.logger(__name__)


@router.post("/documents")
async def generate_document(document_request: model.DocumentRequest) -> JSONResponse:
    """
    Get the document request and hand it off to the document_generator
    module for processing.
    """
    # Top level exception handler
    try:
        task = document_generator.generate_document.apply_async(
            args=(document_request.json(),)
        )
    except HTTPException as exc:
        raise exc
    except (
        Exception
    ) as exc:  # catch any exceptions we weren't expecting, handlers handle the ones we do expect.
        logger.exception(
            "There was an error while attempting to fulfill the document "
            "request. Likely reason is the following exception:"
        )
        # Handle exceptions that aren't handled otherwise
        raise HTTPException(
            status_code=status.HTTP_500_INTERNAL_SERVER_ERROR, detail=str(exc)
        )
    else:
        logger.debug("task_id: %s", task.id)
        return JSONResponse({"task_id": task.id})


@router.post("/documents_docx")
async def generate_docx_document(
    document_request: model.DocumentRequest,
) -> JSONResponse:
    """
    Get the document request and hand it off to the document_generator
    module for processing.
    """
    # Top level exception handler
    try:
        task = document_generator.generate_docx_document.apply_async(
            args=(document_request.json(),)
        )
    except HTTPException as exc:
        raise exc
    except (
        Exception
    ) as exc:  # catch any exceptions we weren't expecting, handlers handle the ones we do expect.
        logger.exception(
            "There was an error while attempting to fulfill the document "
            "request. Likely reason is the following exception:"
        )
        # Handle exceptions that aren't handled otherwise
        raise HTTPException(
            status_code=status.HTTP_500_INTERNAL_SERVER_ERROR, detail=str(exc)
        )
    else:
        logger.debug("task_id: %s", task.id)
        return JSONResponse({"task_id": task.id})


@router.get("/task_status/{task_id}")
async def task_status(task_id: str) -> JSONResponse:
    res: AsyncResult[dict[str, str]] = AsyncResult(task_id)
    if res.state == celery.states.SUCCESS:
        return JSONResponse({"state": celery.states.SUCCESS, "result": res.result})
    return JSONResponse(
        {
            "state": res.state,
        }
    )


@router.get("/language_codes_and_names")
async def lang_codes_and_names() -> Sequence[tuple[str, str, bool]]:
    """
    Return list of all available language code, name tuples.
    """
    return resource_lookup.lang_codes_and_names()


@router.get("/shared_book_codes/{lang0_code}/{lang1_code}")
async def shared_book_codes(
    lang0_code: str, lang1_code: str
) -> Sequence[tuple[str, str]]:
    """
    Return list of available resource codes common to both lang0_code and lang1_code.
    """
    return resource_lookup.shared_book_codes(lang0_code, lang1_code)


@router.get("/resource_types/{lang_code}/{book_codes}")
async def resource_types(
    lang_code: str,
    book_codes: str,
) -> Sequence[tuple[str, str]]:
    """
    Return the list of available resource types tuples for lang_code
    with book_codes.
    """
    return cast(
        Sequence[tuple[str, str]], resource_lookup.resource_types(lang_code, book_codes)
    )


@router.get("/resource_availability/{lang_code}")
def resource_availability(lang_code: str) -> model.ResourceAvailability:
    """
    Return every (book, resource type) combination available for
    lang_code. This is a plain def, rather than async def, so that
    FastAPI runs it in its threadpool: provisioning a language's
    resource assets can block for a while.
    """
    return resource_availability_.resource_availability(lang_code)


@router.get("/book_codes_for_lang/{lang_code}")
async def book_codes_for_lang(lang_code: str) -> Sequence[tuple[str, str]]:
    """Return list of all available resource codes."""
    return cast(
        Sequence[tuple[str, str]], resource_lookup.book_codes_for_lang(lang_code)
    )


@router.get("/book_codes_for_lang_from_usfm_only/{lang_code}")
async def book_codes_for_lang_from_usfm_only(
    lang_code: str,
) -> Sequence[tuple[str, str]]:
    """Return list of all available resource codes."""
    return cast(
        Sequence[tuple[str, str]],
        resource_lookup.book_codes_for_lang_from_usfm_only(lang_code),
    )


@router.get("/chapters_in_books")
async def chapters_in_books() -> dict[str, list[int]]:
    return resource_lookup.chapters_in_books()


# @router.get("/chapters_in_book/{book_code}")
# async def chapters_in_book(book_code: str) -> list[int]:
#     return resource_lookup.chapters_in_book(book_code)


@router.get("/health/status")
async def health_status() -> tuple[dict[str, str], int]:
    """Ping-able server endpoint."""
    return {"status": "ok"}, 200
