import json
import pytest
from fastapi.testclient import TestClient
from tests.shared.utils import check_result
from doc.entrypoints.app import app
from doc.config import settings
from passages.reviewers_guide.model import BibleReference


# passage_references JSON is not correct, skipping for now as this is
# covered by frontend test anyway.
@pytest.mark.skip
@pytest.mark.passages
@pytest.mark.docx
def test_en_passages_docx() -> None:
    with TestClient(app=app, base_url=settings.api_test_url()) as client:
        response = client.post(
            "/passages/document_docx",
            json={
                "lang_code": "en",
                "passage_references": json.dumps(
                    [
                        BibleReference(
                            lang_code="en",
                            book_code="mat",
                            book_name="Matthew",
                            start_chapter=2,
                            start_chapter_verse_ref="1-12",
                            end_chapter=None,
                            end_chapter_verse_ref=None,
                        ).model_dump(),
                        BibleReference(
                            lang_code="en",
                            book_code="mat",
                            book_name="Matthew",
                            start_chapter=3,
                            start_chapter_verse_ref="13-17",
                            end_chapter=None,
                            end_chapter_verse_ref=None,
                        ).model_dump(),
                        BibleReference(
                            lang_code="en",
                            book_code="mat",
                            book_name="Matthew",
                            start_chapter=4,
                            start_chapter_verse_ref="1-11",
                            end_chapter=None,
                            end_chapter_verse_ref=None,
                        ).model_dump(),
                        BibleReference(
                            lang_code="en",
                            book_code="mat",
                            book_name="Matthew",
                            start_chapter=26,
                            start_chapter_verse_ref="3,7-10",
                            end_chapter=None,
                            end_chapter_verse_ref=None,
                        ).model_dump(),
                        BibleReference(
                            lang_code="en",
                            book_code="gen",
                            book_name="Genesis",
                            start_chapter=1,
                            start_chapter_verse_ref="1,3-4,12",
                            end_chapter=None,
                            end_chapter_verse_ref=None,
                        ).model_dump(),
                        BibleReference(
                            lang_code="en",
                            book_code="exo",
                            book_name="Exodus",
                            start_chapter=3,
                            start_chapter_verse_ref="5",
                            end_chapter=None,
                            end_chapter_verse_ref=None,
                        ).model_dump(),
                    ]
                ),
                "email_address": settings.TO_EMAIL_ADDRESS,
            },
        )
        check_result(response, suffix="docx")
