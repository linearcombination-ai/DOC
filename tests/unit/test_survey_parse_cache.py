"""Tests for the in-memory cache of parsed Reviewer's Guide .docx files."""

import os
import shutil
from pathlib import Path

import pytest
from passages.domain.survey_files import EN_RG_DIR, SURVEY_FILES
from passages.reviewers_guide import parser


def test_survey_files_exist_in_place() -> None:
    for file_name in SURVEY_FILES.values():
        assert (Path(EN_RG_DIR) / file_name).is_file()


def test_parse_is_cached_and_replaced_file_is_picked_up(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    nt = Path(EN_RG_DIR) / SURVEY_FILES["nt"]
    rg3 = Path(EN_RG_DIR) / SURVEY_FILES["ot_rg3"]
    target = tmp_path / "survey.docx"
    shutil.copyfile(nt, target)

    calls: list[str] = []
    real_find = parser.find_bible_references

    def counting_find(path: str) -> tuple[list[str], list[str]]:
        calls.append(path)
        return real_find(path)

    monkeypatch.setattr(parser, "find_bible_references", counting_find)

    first = parser.parsed_texts_from_docx(str(target))
    again = parser.parsed_texts_from_docx(str(target))
    assert len(calls) == 1, "second call should be served from the cache"
    assert first == again

    # Callers mutate what they get (the readers localize book names); that
    # must not leak into the cache.
    first[0].bible_reference.book_name = "MUTATED"
    assert parser.parsed_texts_from_docx(str(target))[0] == again[0]

    # Replace the file with different content, forcing a different mtime
    # even on filesystems with coarse timestamps.
    old_mtime_ns = target.stat().st_mtime_ns
    shutil.copyfile(rg3, target)
    os.utime(target, ns=(old_mtime_ns + 10**9, old_mtime_ns + 10**9))
    replaced = parser.parsed_texts_from_docx(str(target))
    assert len(calls) == 2, "a replaced file must be reparsed"
    assert replaced != again
    assert replaced[0].bible_reference.book_code == "job"
