from doc.config import settings
from doc.domain import model
from doc.entrypoints.app import app
from fastapi.testclient import TestClient

from tests.shared.utils import check_finished_document_with_verses_success

logger = settings.logger(__name__)


def test_send_email_with_es_419_ulb_jud_pdf() -> None:
    """
    Produce chapter level interleaved document for language, ar, Arabic
    scripture. There are no other resources than USFM available at
    this time.
    """
    # First generate the PDF
    with TestClient(app=app, base_url=settings.api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": settings.TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN,
                "layout_for_print": False,
                "generate_pdf": True,
                "generate_docx": False,
                "resource_requests": [
                    {
                        "lang_code": "es-419",
                        "resource_type": "ulb",
                        "book_code": "jud",
                    },
                ],
            },
        )
        check_finished_document_with_verses_success(response, suffix="pdf")
