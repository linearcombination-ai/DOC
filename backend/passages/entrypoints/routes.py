import json
from typing import Sequence, cast

import celery.states
from celery.result import AsyncResult
from doc.config import get_settings
from passages.reviewers_guide.model import BibleReference
from fastapi import APIRouter, HTTPException, status
from fastapi.responses import JSONResponse
from passages.domain import document_generator, model, resource_lookup
from passages.domain.document_generator import stet_exhaustive_verse_list

router = APIRouter()

logger = get_settings().logger(__name__)


@router.post("/passages/document_docx")
async def generate_passages_docx_document(
    passages_document_request: model.PassagesDocumentRequest,
) -> JSONResponse:
    logger.debug(
        "passages_document_request: %s",
        passages_document_request,
    )
    try:
        task = document_generator.generate_passages_docx_document.apply_async(
            args=(
                passages_document_request.lang0_code,
                passages_document_request.lang0_name,
                passages_document_request.lang1_code,
                passages_document_request.lang1_name,
                # Serialize the list of objects to a JSON string
                json.dumps(
                    passages_document_request.bible_references,
                    default=lambda obj: obj.model_dump(),
                ),
                passages_document_request.email_address,
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


@router.get("/passages/task_status/{task_id}")
async def task_status(task_id: str) -> JSONResponse:
    res: AsyncResult[dict[str, str]] = AsyncResult(task_id)
    if res.state == celery.states.SUCCESS:
        return JSONResponse({"state": celery.states.SUCCESS, "result": res.result})
    return JSONResponse(
        {
            "state": res.state,
        }
    )


@router.get("/passages/stet_verse_list/{lang_code}")
async def stet_verse_list(lang_code: str) -> Sequence[BibleReference]:
    return cast(Sequence[BibleReference], stet_exhaustive_verse_list(lang_code))


@router.get("/nt_survey_rg_passages/{lang_code}")
async def nt_survey_rg_passages(lang_code: str) -> Sequence[BibleReference]:
    """
    Return list of reified NT Survey Reviewer's Guide passages as BibleReference instances.
    """
    bible_references = resource_lookup.nt_survey_rg_passages(lang_code)
    return bible_references


@router.get("/ot_survey_rg1_passages/{lang_code}")
async def ot_survey_rg1_passages(lang_code: str) -> Sequence[BibleReference]:
    """
    Return list of reified OT Survey Reviewer's Guide 1 passages as BibleReference instances.
    """
    bible_references = resource_lookup.ot_survey_rg1_passages(lang_code)
    return bible_references


@router.get("/ot_survey_rg2_passages/{lang_code}")
async def ot_survey_rg2_passages(lang_code: str) -> Sequence[BibleReference]:
    """
    Return list of reified OT Survey Reviewer's Guide 2 passages as BibleReference instances.
    """
    bible_references = resource_lookup.ot_survey_rg2_passages(lang_code)
    return bible_references


@router.get("/ot_survey_rg3_passages/{lang_code}")
async def ot_survey_rg3_passages(lang_code: str) -> Sequence[BibleReference]:
    """
    Return list of reified OT Survey Reviewer's Guide 3 passages as BibleReference instances.
    """
    bible_references = resource_lookup.ot_survey_rg3_passages(lang_code)
    return bible_references


@router.get("/ot_survey_rg4_passages/{lang_code}")
async def ot_survey_rg4_passages(lang_code: str) -> Sequence[BibleReference]:
    """
    Return list of reified OT Survey Reviewer's Guide 4 passages as BibleReference instances.
    """
    bible_references = resource_lookup.ot_survey_rg4_passages(lang_code)
    return bible_references
