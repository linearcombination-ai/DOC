from os import scandir
from typing import Annotated, Sequence

import celery.states
from celery.result import AsyncResult
from doc.config import settings
from stet.domain import resource_lookup
from docx import Document
from fastapi import APIRouter, Depends, Request, HTTPException, status
from fastapi.responses import JSONResponse
from stet.domain import document_generator, model

router = APIRouter()

logger = settings.logger(__name__)

_PRODUCTION_LANG_CODES: tuple[str, ...] = ("en", "es-419", "pt-br")


# Providers take no arguments so they add nothing to the HTTP contract.
def get_production_lang_codes() -> tuple[str, ...]:
    return _PRODUCTION_LANG_CODES


def get_stet_dir() -> str:
    return settings.STET_DIR


@router.get("/stet/source_languages")
async def source_lang_codes_and_names(
    request: Request,
    stet_dir: Annotated[str, Depends(get_stet_dir)],
    production_lang_codes: Annotated[
        tuple[str, ...], Depends(get_production_lang_codes)
    ],
) -> Sequence[tuple[str, str, bool]]:
    """
    Return list of all available language code, name tuples for which
    Translation Services has provided a source document unless the
    request comes from production server in which case limit the
    languages according to special rules.
    """
    logger.debug("headers: %s", dict(request.headers))
    is_production = request.headers.get("x-is-production") == "true"
    logger.debug("is_production: %s", is_production)
    # Scan what source docs are available and make sure to filter
    # source language candidates to only those languages.
    ietf_codes = [
        entry.name.split("stet_")[-1].removesuffix(".docx")
        for entry in scandir(stet_dir)
        if entry.is_file()
        and entry.name.startswith("stet_")
        and entry.name.endswith(".docx")
    ]
    # logger.debug("source ietf_codes: %s", ietf_codes)
    ietf_codes_for_docs_with_fourth_column = []
    for ietf_code in ietf_codes:
        doc = Document(f"{stet_dir}/stet_{ietf_code}.docx")
        for table in doc.tables:
            for row in table.rows:
                # If 4th column exists, get bolded words from it
                if len(row.cells) > 3 and row.cells[3].text:
                    if ietf_code not in ietf_codes_for_docs_with_fourth_column:
                        ietf_codes_for_docs_with_fourth_column.append(ietf_code)
    languages = []
    for lang_code_and_name in resource_lookup.lang_codes_and_names_having_usfm():
        if is_production:
            if (
                lang_code_and_name[0] in production_lang_codes
                and lang_code_and_name[0] in ietf_codes_for_docs_with_fourth_column
            ):
                languages.append(lang_code_and_name)
        else:
            if lang_code_and_name[0] in ietf_codes:
                languages.append(lang_code_and_name)
    # logger.debug("source languages: %s", languages)
    return languages


@router.get("/stet/target_languages/{lang0_code}")
async def target_lang_codes_and_names(
    lang0_code: str,
) -> Sequence[tuple[str, str, bool]]:
    """
    Return list of all available language code, name tuples excluding
    the source language chosen: lang0_code.
    """
    # logger.debug("source language: %s", lang0_code)
    languages = [
        lang_code_and_name
        for lang_code_and_name in resource_lookup.lang_codes_and_names_having_usfm()
        if lang_code_and_name[0] != lang0_code
    ]
    # logger.debug("target languages: %s", languages)
    return languages


@router.post("/stet/documents_docx")
async def generate_docx_document(
    stet_document_request: model.StetDocumentRequest,
) -> JSONResponse:
    # Top level exception handler
    try:
        task = document_generator.generate_stet_docx_document.apply_async(
            args=(
                stet_document_request.lang0_code,
                stet_document_request.lang1_code,
                stet_document_request.email_address,
                stet_document_request.use_increased_line_spacing,
            )
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


@router.get("/stet/task_status/{task_id}")
async def task_status(task_id: str) -> JSONResponse:
    res: AsyncResult[dict[str, str]] = AsyncResult(task_id)
    if res.state == celery.states.SUCCESS:
        return JSONResponse({"state": celery.states.SUCCESS, "result": res.result})
    return JSONResponse(
        {
            "state": res.state,
        }
    )
