from doc.config import get_settings
from doc.domain.model import (
    AssemblyLayoutEnum,
    AssemblyStrategyEnum,
    DocumentRequest,
    ResourceRequest,
)
from doc.domain.resource_lookup import resource_lookup_dto
from passages.domain.resource_lookup import (
    nt_survey_rg_passages,
    ot_survey_rg1_passages,
    ot_survey_rg2_passages,
    ot_survey_rg3_passages,
    ot_survey_rg4_passages,
)


def test_lookup_successes() -> None:
    assembly_strategy_kind: AssemblyStrategyEnum = (
        AssemblyStrategyEnum.INTERLEAVE_BY_BOOK
    )
    assembly_layout_kind: AssemblyLayoutEnum = AssemblyLayoutEnum.ONE_COLUMN
    resource_requests: list[ResourceRequest] = [
        ResourceRequest(lang_code="fr", resource_type="ulb", book_code="gen"),
        ResourceRequest(lang_code="fr", resource_type="tn", book_code="gen"),
        ResourceRequest(lang_code="mr", resource_type="ulb", book_code="gen"),
        ResourceRequest(lang_code="erk-x-erakor", resource_type="reg", book_code="eph"),
    ]
    document_request = DocumentRequest(
        email_address=get_settings().FROM_EMAIL_ADDRESS,
        assembly_strategy_kind=assembly_strategy_kind,
        assembly_layout_kind=assembly_layout_kind,
        layout_for_print=True,
        generate_pdf=True,
        generate_epub=False,
        generate_docx=False,
        resource_requests=resource_requests,
    )
    for resource_request in document_request.resource_requests:
        resource_lookup_dto_ = resource_lookup_dto(
            resource_request.lang_code,
            resource_request.resource_type,
            resource_request.book_code,
        )
        if resource_lookup_dto_:
            assert resource_lookup_dto_.url


# NOTE This fails, on purpose, because zh doesn't use ulb for its USFM resource
# type but 'cuv' instead, i.e., zh ulb is a special case.
def test_lookup_failures() -> None:
    assembly_strategy_kind: AssemblyStrategyEnum = (
        AssemblyStrategyEnum.INTERLEAVE_BY_BOOK
    )
    assembly_layout_kind: AssemblyLayoutEnum = AssemblyLayoutEnum.ONE_COLUMN
    resource_requests: list[ResourceRequest] = [
        ResourceRequest(lang_code="zh", resource_type="ulb", book_code="jol")
    ]
    document_request = DocumentRequest(
        email_address=get_settings().FROM_EMAIL_ADDRESS,
        assembly_strategy_kind=assembly_strategy_kind,
        assembly_layout_kind=assembly_layout_kind,
        layout_for_print=False,
        generate_pdf=True,
        generate_epub=False,
        generate_docx=False,
        resource_requests=resource_requests,
    )

    for resource_request in document_request.resource_requests:
        resource_lookup_dto_ = resource_lookup_dto(
            resource_request.lang_code,
            resource_request.resource_type,
            resource_request.book_code,
        )
        if resource_lookup_dto_:
            assert not resource_lookup_dto_.url


def test_nt_survey_rg_passages() -> None:
    bible_references = nt_survey_rg_passages()
    assert bible_references


def test_en_ot_survey_rg1_passages() -> None:
    bible_references = ot_survey_rg1_passages()
    assert bible_references


def test_en_ot_survey_rg2_passages() -> None:
    bible_references = ot_survey_rg2_passages()
    assert bible_references


def test_en_ot_survey_rg3_passages() -> None:
    bible_references = ot_survey_rg3_passages()
    assert bible_references


def test_en_ot_survey_rg4_passages() -> None:
    bible_references = ot_survey_rg4_passages()
    assert bible_references
