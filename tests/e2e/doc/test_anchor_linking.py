"""
This module can be run after tests that generate PDF and thus the HTML
PDFs depend on. It will run tests that ensure that anchor links have
both source and destination to automate link checking for dead links
in the HTML and thus in the PDF.
"""

import glob
import re

from doc.config import get_settings
from termcolor import colored

logger = get_settings().logger(__name__)

ANCHOR_SOURCE_LINK_REGEX = r"\<a.*href=\"\#(?P<source_ref>.*?)\"\>"
ANCHOR_DESTINATION_LINK = r' id=(?:"|\'){}(?:"|\')'


# See Makefile target local-check-anchor-links for how to use this
# test. Note that the function name does not start with test_ and is
# thus not picked up, by design, by pytest on test runs as it is meant
# to be run manually and selectively after other tests generate HTML
# output, e.g., the Makefile target: local-e2e-tests.
def check_anchor_links_have_source_and_destination() -> None:
    """
    Test that anchor source links have a corresponding destination
    link in the generated HTML on which the PDF is based. This is an
    automated way of checking for dead links rather than clicking in
    PDF files.
    """
    generated_html_files = glob.glob(
        "{}/*.html".format(get_settings().DOCUMENT_OUTPUT_DIR)
    )
    for html_file in generated_html_files:
        logger.debug("Checking anchor links in html_file: {}".format(html_file))
        with open(html_file, "r") as fin:
            html = fin.read()
            for match in re.finditer(ANCHOR_SOURCE_LINK_REGEX, html):
                anchor_dest_link = ANCHOR_DESTINATION_LINK.format(
                    re.escape(match.group("source_ref"))
                )
                logger.debug(
                    "Found anchor source link target: %s, about to test for anchor destination link target regex: %s in file %s",
                    match.group("source_ref"),
                    anchor_dest_link,
                    html_file,
                )
                assert re.search(anchor_dest_link, html)
                logger.debug(
                    "%s anchor source link target: %s, has matching anchor destination link target found by regex: %s",
                    colored("PASSED", "green"),
                    match.group("source_ref"),
                    anchor_dest_link,
                )


if __name__ == "__main__":
    check_anchor_links_have_source_and_destination()
