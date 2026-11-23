"""Tests that the shared fakes stand in for the real collaborators."""

import smtplib
import subprocess
from pathlib import Path

import pytest
import requests
from doc.config import settings
from doc.domain import parsing, resource_lookup
from doc.domain.email_utils import send_email_with_attachment
from doc.domain.model import Attachment

from tests.unit.celery.celery_scaffold import (
    FakeHttp,
    FakeResponse,
    FakeSmtp,
    FakeSubprocess,
    data_api_payload,
    smtp_error,
)


def test_fake_data_api_feeds_fetch_source_data(fake_http: FakeHttp) -> None:
    fake_http.data_api_response = FakeResponse(
        data_api_payload(
            [("https://content.example.org/org/en_ulb", "ulb", "en")]
        )
    )
    source_data = resource_lookup.fetch_source_data()
    assert source_data is not None
    assert [repo.content.language.ietf_code for repo in source_data.git_repo] == ["en"]
    assert len(fake_http.posts_to(str(settings.DATA_API_URL))) == 1


def test_fake_data_api_outage_is_reachable_through_requests(
    fake_http: FakeHttp,
) -> None:
    fake_http.data_api_error = requests.ConnectionError("down")
    with pytest.raises(requests.ConnectionError):
        requests.post("http://data-api.invalid/graphql", json={})


def test_fake_usfm_parser_writes_the_requested_output(
    fake_http: FakeHttp, tmp_path: Path
) -> None:
    output_file = tmp_path / "chapter.html"
    html = parsing.usfm_chapter_html(
        "\\c 1", str(tmp_path / "chapter.usfm"), str(output_file)
    )
    assert html == fake_http.usfm_parser_html
    assert len(fake_http.posts_to("/api/converter/convert")) == 1


def test_fake_usfm_parser_outage_is_swallowed_like_the_real_one(
    fake_http: FakeHttp, tmp_path: Path
) -> None:
    fake_http.usfm_parser_error = requests.ConnectionError("parser down")
    html = parsing.usfm_chapter_html(
        "\\c 1", str(tmp_path / "chapter.usfm"), str(tmp_path / "chapter.html")
    )
    assert html is None


def test_fake_subprocess_records_commands_and_creates_outputs(
    fake_subprocess: FakeSubprocess, tmp_path: Path
) -> None:
    output = tmp_path / "out.pdf"
    subprocess.run(["weasyprint", "in.html", str(output)], check=True)
    assert fake_subprocess.commands == [["weasyprint", "in.html", str(output)]]
    assert output.read_bytes() == fake_subprocess.output_bytes
    assert subprocess.call("git clone x y", shell=True) == 0
    assert fake_subprocess.commands[-1] == "git clone x y"


def test_fake_subprocess_failure_modes(fake_subprocess: FakeSubprocess) -> None:
    fake_subprocess.exit_code = 128
    assert subprocess.call("git clone x y", shell=True) == 128
    with pytest.raises(subprocess.CalledProcessError):
        subprocess.check_call("curl x", shell=True)
    with pytest.raises(subprocess.CalledProcessError):
        subprocess.run(["prince"], check=True)


def test_fake_smtp_records_mail(fake_smtp: FakeSmtp, tmp_path: Path) -> None:
    attachment = tmp_path / "doc.pdf"
    attachment.write_bytes(b"%PDF")
    send_email_with_attachment(
        "reader@example.org",
        [Attachment(filepath=str(attachment), mime_type=("application", "pdf"))],
        "some_key",
        smtp_host="smtp.example.org",
        smtp_port=587,
    )
    assert fake_smtp.connections == [("smtp.example.org", 587)]
    assert len(fake_smtp.sent) == 1
    assert list(fake_smtp.sent[0].recipients) == ["reader@example.org"]
    assert "doc.pdf" in fake_smtp.sent[0].message


def test_smtp_outage_is_swallowed_by_email_sending(fake_smtp: FakeSmtp) -> None:
    fake_smtp.login_error = smtp_error()
    # Current behavior: failures are logged, not raised.
    send_email_with_attachment("reader@example.org", [], "some_key")
    assert fake_smtp.sent == []
    assert isinstance(smtp_error(), smtplib.SMTPException)
