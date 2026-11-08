import json
import re
from glob import glob
from os.path import exists, join
from pathlib import Path
from urllib.parse import urlparse

import yaml
from doc.config import get_settings
from doc.domain.model import Data, JsonManifestBook, JsonManifestData
from passages.domain.survey_files import en_rg_dir
from pydantic import HttpUrl


logger = get_settings().logger(__name__)

# Specific replacements: lang code, last_segment -> replacement last_segment
REPLACEMENTS_BY_LANG_CODE_AND_LAST_SEGMENT = {
    ("fa", "fa_opv"): "fa_ulb",
    ("my", "my_juds"): "my_ulb",
    ("zmq", "faustin_azaza"): "zmq_mrk_text_reg",
}

# Prefixes to remove in last repo URL segment regardless of lang code
PREFIXES_TO_REMOVE = [
    "Jordan_",
    "alexandre_brazil_",
    "azz_athan_",
    "bayan_",
    "botsw01_",
    "danjuma_alfred_h_",
    "dijim1_",
    "ezekieldabere_",
    "gravy_",
    "jdwood_",
    "jks222111_",
    "jonathan_",
    "krispy_",
    "lversaw_",
    "michael_",
    "mitikiwostky_",
    "moufida_",
    "mushohe-25nb_63.kum_",
    "nbtt_",
    "ngamo1_",
    "ngamo_",
    "oratab01_",
    "otlaadisa_",
    "parfait-ayanou_",
    "romantts2_",
    "sambadanum_",
    "timothydanjuma_",
    "tom-88pn_0003.machinga_",
    "translator09_",
    "ukum1_",
    "vere3_",
    "yukuben1_",
]

# lang code -> prefixes to remove
LANG_SPECIFIC_PREFIXES_TO_REMOVE = {
    "acq": ["Dawit-Dessie_", "burje_duro_", "tersitzewde_"],
    "arb": ["burje_duro_"],
    "byn": ["Dawit-Dessie_", "burje_duro_"],
    "dz": ["Dzongkha_", "chuck_"],
    "iba-x-ketungau": ["dayakketungau_", "lawadinusah_", "Lawadinusah_"],
    "kcn": ["mvccbtt_"],
    "kmq": ["Dawit-Dessie_"],
    "knx-x-bajanya": ["bajanya_knx"],
    "kun": ["Dawit-Dessie_"],
    "kxh": ["burje_duro_"],
    "kxv": ["jathapu_"],
    "ndh": ["chindali_"],
    "sbx": ["faustin-azaza_"],
    "scg-x-dayakkatarak": ["yustius_"],
    "sdm-x-pangkalsuka": ["dayaksuka_"],
    "svr": ["billburns58_"],
    "sze": ["Dawit-Dessie_", "burje_duro_"],
    "xdy-x-dayakpunti": ["anselmus_"],
    "xdy-x-mentebah": ["dayakfdkj_", "lawadinusah_", "Lawadinusah_"],
    "xdy-x-senduruhan": ["dayaksenduruhan_", "Lawadinusah_"],
    "xsr-x-shyarpa": ["shyarpa_"],
    "xwg": ["Dawit-Dessie_", "burje_duro_"],
}


def normalize_last_segment(
    lang_code: str,
    last_segment: str,
    hardcoded_replacements: dict[
        tuple[str, str], str
    ] = REPLACEMENTS_BY_LANG_CODE_AND_LAST_SEGMENT,
    universal_prefixes: list[str] = PREFIXES_TO_REMOVE,
    lang_specific_prefixes: dict[str, list[str]] = LANG_SPECIFIC_PREFIXES_TO_REMOVE,
) -> str:
    """
    Handle special cases where git repo URL does not follow the expected pattern.
    Ideally these repos URLs would have their last segment renamed
    properly, e.g.,
    'https://content.bibletranslationtools.org/faustin_azaza/faustin_azaza'
    renamed to
    'https://content.bibletranslationtools.org/faustin_azaza/zmq_mrk_text_reg',
    but since we don't have control over that, we handle these anomalies
    here.
    """
    if lang_code == "en" and last_segment.endswith(".docx"):
        return en_rg_dir()
    if (lang_code, last_segment) in hardcoded_replacements:
        return hardcoded_replacements[(lang_code, last_segment)]
    for prefix in universal_prefixes:
        if last_segment.startswith(prefix):
            return re.sub(f"^{re.escape(prefix)}", "", last_segment)
    for lang, prefixes in lang_specific_prefixes.items():
        if lang_code == lang:
            for prefix in prefixes:
                if last_segment.startswith(prefix):
                    return re.sub(f"^{re.escape(prefix)}", "", last_segment)
    return last_segment


def get_last_segment(url: HttpUrl, lang_code: str) -> str:
    """
    Extract the last segment of the URL path and normalize it
    according to known anomalies and naming patterns.
    """
    parsed_url = urlparse(str(url))
    path_segments = parsed_url.path.strip("/").split("/")
    last_segment = path_segments[-1] if path_segments else ""
    return normalize_last_segment(lang_code, last_segment)


def get_book_name_from_title_file(
    resource_filepath: str,
    lang_code: str,
    repo_components: list[str],
) -> str:
    """
    Book names in front/title.txt files may or may not be localized,
    it depends on the translation work done for lang_code.
    """
    # book_codes_and_names_localized: dict[str, str] = {}
    book_name = ""
    book_name_file = join(resource_filepath, "front", "title.txt")
    if exists(book_name_file):
        with open(book_name_file, "r") as fin:
            book_name = fin.read()
            logger.debug("book_name: %s", book_name)
            # if book_name:
            # Moved this code to the caller
            # localized_book_name_ = normalize_localized_book_name(book_name)
            # localized_book_name = maybe_correct_book_name(
            #     lang_code, localized_book_name_
            # )
            # book_code = repo_components[1]
            # book_codes_and_names_localized[book_code] = book_name
    return book_name


def load_manifest(file_path: str) -> str:
    with open(file_path, "r") as file:
        return file.read()


def book_codes_and_names_from_manifest(
    resource_dir: str,
    manifest_glob_fmt_str: str = "{}/**/manifest.{}",
    manifest_glob_alt_fmt_str: str = "{}/manifest.{}",
) -> dict[str, str]:
    """
    Look up the language direction in the manifest file if one is
    available for this resource.
    """
    book_codes_and_names: dict[str, str] = {}
    # Try to find manifest yaml at typical directory
    manifest_candidates = glob(manifest_glob_fmt_str.format(resource_dir, "yaml"))
    if not manifest_candidates:
        # Now try to find manifest yaml at parent directory of typical directory
        manifest_candidates = glob(
            manifest_glob_alt_fmt_str.format(resource_dir, "yaml")
        )
        if not manifest_candidates:
            # Some languages provide their manifest in json format.
            # Try to find manifest json at typical directory
            manifest_candidates = glob(
                manifest_glob_fmt_str.format(resource_dir, "json")
            )
            if not manifest_candidates:
                # Try to find manifest json at parent directory of typical directory
                manifest_candidates = glob(
                    manifest_glob_alt_fmt_str.format(resource_dir, "json")
                )
    # logger.debug("manifest_candidates: %s", manifest_candidates)
    if manifest_candidates:
        candidate = manifest_candidates[0]
        suffix = str(Path(candidate).suffix)
        # book_codes_and_names: dict[str, str] = {}
        # Get localized book names
        manifest_data = load_manifest(candidate)
        # logger.debug("manifest_data: %s", manifest_data)
        if suffix == ".yaml":
            data: Data = yaml.safe_load(manifest_data)
            book_codes_and_names = {
                book["identifier"]: book["title"] for book in data["projects"]
            }
            logger.debug("book_codes_and_names from yaml: %s", book_codes_and_names)
        # Heart languages often have .json manifest files
        # per book and not per language.
        elif suffix == ".json":
            json_data: JsonManifestData = json.loads(manifest_data)
            logger.debug("json_data: %s", json_data)
            project: JsonManifestBook = json_data["project"]
            book_codes_and_names = {project["id"]: project["name"]}
            logger.debug("book_codes_and_names from json: %s", book_codes_and_names)
    return book_codes_and_names
