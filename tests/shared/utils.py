import os
import re
import time
from typing import Literal, Union

import httpx
from doc.config import get_settings
from doc.entrypoints.app import app
from fastapi.testclient import TestClient
from docx import Document
from docx.document import Document as DocxDocument

logger = get_settings().logger(__name__)

AcceptedSuffixes = Union[
    Literal["html"], Literal["pdf"], Literal["epub"], Literal["docx"]
]


def check_result(
    response: httpx._models.Response,
    suffix: AcceptedSuffixes,
    poll_duration: int = 4,
    status_url_fmt_str: str = "/task_status/{}",
    success_state: str = "SUCCESS",
    failure_state: str = "FAILURE",
) -> str:
    logger.debug("response.json(): {}".format(response.json()))
    task_id = response.json()["task_id"]
    assert task_id
    logger.debug("task_id: %s", task_id)
    finished_document_request_key: str
    while True:
        with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
            response2 = client.get(
                status_url_fmt_str.format(task_id),
            )
            json_data = response2.json()
            logger.debug("json task status data: {}".format(json_data))
            if json_data["state"] == success_state:
                finished_document_request_key = json_data["result"]
                finished_document_path = os.path.join(
                    get_settings().DOCUMENT_OUTPUT_DIR,
                    "{}.{}".format(finished_document_request_key, suffix),
                )
                logger.debug(
                    "finished_document_path: {}".format(finished_document_path)
                )
                assert os.path.exists(finished_document_path)
                assert response2.status_code == 200
                break
            elif json_data["state"] == failure_state:
                logger.info(
                    "Test failed likely due to celery task failure, check the celery flower dashboard"
                )
                raise Exception(
                    "Received celery FAILURE state therefore e2e test failed"
                )
            time.sleep(poll_duration)
    return finished_document_request_key


def document_contains_substring(
    doc: DocxDocument, substring: str, case_insensitive: bool = False
) -> bool:
    if case_insensitive:
        substring = substring.lower()
    # Check all paragraphs
    for paragraph in doc.paragraphs:
        text = paragraph.text.lower() if case_insensitive else paragraph.text
        if substring in text:
            return True
    # Check all tables
    for table in doc.tables:
        for row in table.rows:
            for cell in row.cells:
                text = cell.text.lower() if case_insensitive else cell.text
                if substring in text:
                    return True
    # Substring not found
    return False


def check_finished_document_with_verses_success(
    response: httpx._models.Response,
    suffix: AcceptedSuffixes = "html",
    poll_duration: int = 4,
) -> None:
    """
    Helper to keep tests DRY.
    """
    finished_document_request_key = check_result(response, suffix, poll_duration)
    html_filepath = os.path.join(
        get_settings().DOCUMENT_OUTPUT_DIR,
        "{}.html".format(finished_document_request_key),
    )
    assert html_filepath, "HTML output file does not exist"
    with open(html_filepath, "r") as fin:
        html = fin.read()
        body_match = re.search(r"<body.*?>(.*?)</body>", html, re.DOTALL)
        assert body_match, "Body not found in HTML"
        body_content = body_match.group(1)
        assert 'class="versemarker"' in body_content, "No verses found in HTML"


def check_finished_document_without_verses_success(
    response: httpx._models.Response,
    suffix: AcceptedSuffixes = "html",
    poll_duration: int = 4,
) -> None:
    """
    Helper to keep tests DRY.
    """
    finished_document_request_key = check_result(response, suffix, poll_duration)
    html_filepath = os.path.join(
        get_settings().DOCUMENT_OUTPUT_DIR,
        "{}.html".format(finished_document_request_key),
    )
    assert html_filepath, "HTML output file does not exist"
    with open(html_filepath, "r") as fin:
        html = fin.read()
        body_match = re.search(r"<body.*?>(.*?)</body>", html, re.DOTALL)
        assert body_match, "Body not found in HTML"
        body_content = body_match.group(1)
        assert 'class="verse"' not in body_content


def check_finished_document_with_body_success(
    response: httpx._models.Response,
    suffix: AcceptedSuffixes = "html",
    poll_duration: int = 4,
) -> None:
    """
    Helper to keep tests DRY.
    """
    finished_document_request_key = check_result(response, suffix, poll_duration)
    html_filepath = os.path.join(
        get_settings().DOCUMENT_OUTPUT_DIR,
        "{}.html".format(finished_document_request_key),
    )
    with open(html_filepath, "r") as fin:
        html = fin.read()
        body_match = re.search(
            r"<body.*?>(\s*\S.*\S\s*|\s*\S\s*)</body>", html, re.DOTALL
        )
        assert body_match, "Body not found in HTML"


def is_within_distance(text: str, substr1: str, substr2: str, distance: int) -> bool:
    """
    Check if substr2 is within a certain distance of substr1 in the given text.

    :param text: The text to search.
    :param substr1: The first substring to find.
    :param substr2: The second substring to find.
    :param distance: The maximum distance allowed between substr1 and substr2.
    :return: True if substr2 is within the distance of substr1, otherwise False.
    """
    index1 = text.find(substr1)
    index2 = text.find(substr2)
    logger.debug("index1: %s, index2: %s", index1, index2)

    # If either substring is not found, return False
    if index1 == -1 or index2 == -1:
        return False

    # Check if the distance between substr1 and substr2 is within the given distance
    logger.debug("distance: %s", abs(index1 - index2))
    return abs(index1 - index2) <= distance
