"""
Where the Reviewer's Guide survey .docx sources live and what they are
called.

The files are versioned in the repo next to the code that parses them and
are read in place (never copied). The location is anchored on this file,
not on the working directory, so it is correct in the Docker image, in the
test-runner and in a local checkout alike. This module is deliberately a
leaf (it imports only the settings accessor) so that both the passages and
doc packages can use it without import cycles.
"""

from pathlib import Path

from doc.config import get_settings

DEFAULT_EN_RG_DIR = Path(__file__).resolve().parents[1] / "data" / "en_rg"


def en_rg_dir() -> str:
    """
    Return the directory of the Reviewer's Guide survey .docx files.
    The EN_RG_DIR setting is only an optional override.
    """
    return get_settings().EN_RG_DIR or str(DEFAULT_EN_RG_DIR)


# Survey key -> file name within en_rg_dir(). To change a survey, replace
# the file with the same name; the parse cache notices the new mtime.
SURVEY_FILES: dict[str, str] = {
    "nt": "en_rg_nt_survey.docx",
    "ot_rg1": "en_ot_survey_rg1_gen_deu.docx",
    "ot_rg2": "en_ot_survey_rg2_jos_est.docx",
    "ot_rg3": "en_ot_survey_rg3_job_sng.docx",
    "ot_rg4": "en_ot_survey_rg4_isa_mal.docx",
}
