"""
Useful to unearth combinations that (possibly) do not work (because
no test has been run on them until now). Such failing tests can then
be added as permanent non-random tests so that regressions are
avoided.

Since randomization is involved with respect to fixture combinations,
it could generate invalid document requests instances that fail their
model validation or may access resources which translators have not
yet prepared to be in the required formats properly which will then
cause the whole test suite to fail (depending on pytest config).
Therefore these tests are marked with the custom pytest marker
'randomized' to run them separately from the other tests. Note that
tests exist in additional places that could overlap with these so you
aren't not testing these at all by skipping this in the main test run.
This is just additional coverage that could be run once in a while
using the 'randomized' pytest marker.
"""

import pytest
from doc.config import get_settings
from doc.entrypoints.app import app
from fastapi.testclient import TestClient

from doc.domain import model


@pytest.mark.english_randomized
def test_english_variable_resource_type_combos_for_all_books(
    english_document_request: model.DocumentRequest,
) -> None:
    """
    Use the fixtures in ./conftest.py for English language to generate
    tests that test all books of the bible, but with random
    combinations of resource type, e.g., ulb, tn, tq, tw; ulb, tq, tw;
    etc.. Because all books of the bible are tested, it takes a while
    to run. There were no errors when it ran to completion.
    """
    data = english_document_request.dict()
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post("/documents", json=data)
        assert response.status_code == 200
