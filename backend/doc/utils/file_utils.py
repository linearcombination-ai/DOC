"""This module provides various file utilities."""

import codecs
import json
import os
import shutil
import zipfile
from datetime import datetime, timedelta
from os.path import join
from pathlib import Path
from typing import Any, Optional

import yaml
from doc.config import settings

logger = settings.logger(__name__)

# User agent value required by domain host to allow serving
# files. Other values could possibly also work.
USER_AGENT: str = (
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.11 (KHTML, like Gecko) Chrome/23.0.1271.64 Safari/537.11"
)


def delete_tree(dir: str) -> None:
    try:
        shutil.rmtree(dir)
    except OSError:
        logger.debug(
            "Directory %s was not removed due to an error.",
            dir,
        )
        logger.exception("Caught exception: ")


def unzip(source_file: str, destination_dir: str) -> None:
    """
    Unzips <source_file> into <destination_dir>.

    :param str source_file: The path of the file to read
    :param str destination_dir: The path of the directory to write the unzipped files
    """
    with zipfile.ZipFile(source_file) as zf:
        zf.extractall(destination_dir)


def make_dir(
    dir_name: str, linux_mode: int = 0o755, error_if_not_writable: bool = False
) -> None:
    """
    Creates a directory, if it doesn't exist already.

    If the directory does exist, and <error_if_not_writable> is True,
    the directory will be checked for write-ability.

    :param dir_name: The name of the directory to create
    :param linux_mode: The mode/permissions to set for the new directory expressed as an octal integer (ex. 0o755)
    :param error_if_not_writable: Boolean saying whether to raise an exception if the file is not writable.
    """
    if not os.path.exists(dir_name):
        os.makedirs(dir_name, linux_mode)
    elif error_if_not_writable:
        if not os.access(dir_name, os.R_OK | os.W_OK | os.X_OK):
            raise IOError("Directory {0} is not writable.".format(dir_name))


def read_file(file_name: str, encoding: str = "utf-8") -> str:
    r"""
    Read file into content and return content. If file doesn't exist
    return an empty string. Change line endings from \r\n to \n.
    """
    content = ""
    with codecs.open(file_name, "r", encoding=encoding) as fin:
        content = fin.read()
        # convert Windows line endings to Linux line endings
        content = content.replace("\r\n", "\n")
    return content


def write_file(
    file_name: str, file_contents: Any, indent: Optional[int] = None
) -> None:
    """
    Writes the <file_contents> to <file_name>.
    If <file_contents> is not a string, it is serialized as JSON.
    :param file_name: The path of the file to write
    :param file_contents: The string to write or the object to serialize
    :param indent: Specify a value if you want the output formatted to be more easily readable
    """
    # Make sure the directory exists
    make_dir(os.path.dirname(file_name))
    if isinstance(file_contents, str):
        text_to_write = file_contents
    else:
        if os.path.splitext(file_name)[1] == ".yaml":
            text_to_write = yaml.safe_dump(file_contents)
        else:
            text_to_write = json.dumps(file_contents, sort_keys=True, indent=indent)
    with codecs.open(file_name, "w", encoding="utf-8") as out_file:
        out_file.write(text_to_write)


def file_needs_update(
    file_path: str | Path,
    asset_caching_enabled: bool = settings.ASSET_CACHING_ENABLED,
    asset_caching_period: int = settings.ASSET_CACHING_PERIOD,
) -> bool:
    if not asset_caching_enabled:
        return True
    try:
        path = Path(file_path)
        stat = path.stat()
        mod_time = datetime.fromtimestamp(stat.st_mtime)
        expiry = timedelta(minutes=asset_caching_period)
        return stat.st_size == 0 or (datetime.now() - mod_time > expiry)
    except FileNotFoundError:
        logger.debug("Cache miss for %s", file_path)
        return True


def html_filepath(
    document_request_key: str, output_dir: str = settings.DOCUMENT_OUTPUT_DIR
) -> str:
    """Given document_request_key, return the HTML output file path."""
    return join(output_dir, "{}.html".format(document_request_key))


def pdf_filepath(
    document_request_key: str, output_dir: str = settings.DOCUMENT_OUTPUT_DIR
) -> str:
    """Given document_request_key, return the PDF output file path."""
    return join(output_dir, "{}.pdf".format(document_request_key))


def epub_filepath(
    document_request_key: str, output_dir: str = settings.DOCUMENT_OUTPUT_DIR
) -> str:
    """Given document_request_key, return the ePub output file path."""
    return join(output_dir, "{}.epub".format(document_request_key))


def docx_filepath(
    document_request_key: str,
    prefix: str = "",
    output_dir: str = settings.DOCUMENT_OUTPUT_DIR,
) -> str:
    """Given document_request_key, return the docx output file path."""
    return join(
        output_dir,
        "{}{}.docx".format(prefix, document_request_key),
    )
