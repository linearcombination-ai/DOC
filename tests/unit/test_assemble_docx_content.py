import pytest
from doc.domain import document_generator, parsing, resource_lookup


def test_assemble_docx_content_ordering_of_books() -> None:
    document_request_json = '{"email_address":null,"assembly_strategy_kind":"lbo","assembly_layout_kind":"1c","layout_for_print":false,"resource_requests":[{"lang_code":"tpi","resource_type":"ulb","book_code":"mat"}, {"lang_code":"tpi","resource_type":"ulb","book_code":"mrk"},{"lang_code":"tpi","resource_type":"ulb","book_code":"luk"}],"generate_pdf":true,"generate_epub":false,"generate_docx":false,"chunk_size":"chapter","limit_words":false,"show_tn_book_intro":false,"document_request_source":"ui"}'
    document_request, document_request_key = (
        document_generator.initialize_document_request_and_key(document_request_json)
    )
    resource_lookup_dtos = []
    for resource_request in document_request.resource_requests:
        resource_lookup_dto = resource_lookup.resource_lookup_dto(
            resource_request.lang_code,
            resource_request.resource_type,
            resource_request.book_code,
        )
        if resource_lookup_dto:
            resource_lookup_dtos.append(resource_lookup_dto)
    found_resource_lookup_dtos = [
        resource_lookup_dto
        for resource_lookup_dto in resource_lookup_dtos
        if resource_lookup_dto.url is not None
    ]
    resource_dirs = [
        resource_lookup.prepare_resource_filepath(dto)
        for dto in found_resource_lookup_dtos
    ]
    for resource_dir, dto in zip(resource_dirs, found_resource_lookup_dtos):
        resource_lookup.provision_asset_files(dto.url, resource_dir)
    usfm_books, tn_books, tnc_books, tq_books, tw_books, bc_books = parsing.books(
        found_resource_lookup_dtos,
        resource_dirs,
        document_request.resource_requests,
        document_request.layout_for_print,
        document_request.use_chapter_labels,
        document_request.generate_docx,
        document_request.show_tn_book_intro,
    )
    document_parts = document_generator.assemble_content(
        document_request_key,
        document_request,
        usfm_books,
        tn_books,
        tnc_books,
        tq_books,
        tw_books,
        bc_books,
    )

    def contains_before(seq: list[str], a: str, b: str) -> bool:
        a_index = b_index = None
        for i, s in enumerate(seq):
            if a_index is None and a in s:
                a_index = i
            if b_index is None and b in s:
                b_index = i
            if a_index is not None and b_index is not None:
                break
        if a_index is None or b_index is None:
            return False  # One or both substrings not found
        return a_index < b_index

    content = [part.content for part in document_parts]
    assert contains_before(content, "Matyu", "Mak")
    assert contains_before(content, "Mak", "Luk")
    assert contains_before(content, "Matyu", "Luk")


if __name__ == "__main__":
    pytest.main()
