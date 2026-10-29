from re import compile, match, sub, IGNORECASE, Match

from doc.config import settings

logger = settings.logger(__name__)

HEADING_RE = compile(r"</?h([1-6])\b", IGNORECASE)


def normalize_spaces(text: str) -> str:
    return sub(r"\s+", " ", text).strip()


_ROMAN_TO_INT = {
    "I": "1",
    "II": "2",
    "III": "3",
}

BOOK_NAME_CORRECTION_TABLE: dict[tuple[str, str], str] = {
    ("es-419", "I juan"): "1 Juan",
    ("fr", "Ephésiens"): "Éphésiens",
    ("pt-br", "1 Corintios"): "1 Coríntios",
    (
        "rmp",
        "Galasians sapta 1. v/1 da-h pol. dal goad phi da-h dululan, ne dal mai-h phi da-h apostel ipais ag mayaib. phi. je-su krais mai-h mam gad pha, nug krais matmat ag mau sen pha, nug da-h ipais ag malan. v/2 da-h ayaid amayaid da-h pha dade, hit jain hen ohvu iu- an sios galesia e-h hagaug. v/3 gad mam hita, hayaug je-su krais pha, nug-te hagaug he-eh phadu ne mab hogad nauha-h da-h-du. v/4",
    ): "Galasians",
    ("sw", "Matendo ya mitume"): "Matendo ya Mitume",
    ("sw", "Luke"): "Luka",
    ("sw", "Waraka wa yakobo"): "Yakobo",
}


def normalize_localized_book_name(localized_book_name: str) -> str:
    """
    >>> normalize_localized_book_name("1Peter")
    '1 Peter'
    >>> normalize_localized_book_name("I peter")
    '1 Peter'
    >>> normalize_localized_book_name("III John")
    '3 John'
    >>> normalize_localized_book_name("II john")
    '2 John'
    >>> normalize_localized_book_name("IIjohn")
    '2 John'
    >>> normalize_localized_book_name("john")
    'John'
    >>> normalize_localized_book_name("John")
    'John'
    >>> normalize_localized_book_name("Isaías")
    'Isaías'
    """
    name = localized_book_name.strip()
    match_ = match(r"^(1|2|3|i{1,3})", name, IGNORECASE)
    if match_:
        numeral_raw = match_.group(1)
        numeral_upper = numeral_raw.upper()
        next_char = name[len(numeral_raw) : len(numeral_raw) + 1]
        # Special case: single "I" must be followed by space or uppercase to count as numeral
        if numeral_upper == "I" and not (
            next_char.isspace() or (next_char and next_char.isupper())
        ):
            pass  # treat as normal word
        else:
            if numeral_upper in _ROMAN_TO_INT:
                number = _ROMAN_TO_INT[numeral_upper]
            else:
                number = numeral_upper  # already numeric
            rest = name[len(numeral_raw) :].strip()
            if rest:
                # rest = rest[0].upper() + rest[1:]
                rest = rest.lower().capitalize()
                name = f"{number} {rest}"
            else:
                name = number
            return normalize_spaces(name)
    # Default: just capitalize first letter
    # name = name[0].upper() + name[1:]
    name = name.lower().capitalize()
    return normalize_spaces(name)


def chapter_label_sans_numeric_part(s: str) -> str:
    parts = s.rsplit(maxsplit=1)
    # logger.debug("chapter label parts: %s", parts)
    if len(parts) > 1 and parts[-1].isdigit():
        result = parts[0]
    else:
        result = s
    # logger.debug("chapter label: %s", result)
    return result


def chapter_label_numeric_part(s: str) -> int:
    parts = s.rsplit(maxsplit=1)
    # logger.debug("chapter label parts: %s", parts)
    if len(parts) > 1 and parts[-1].isdigit():
        result = int(parts[-1])
    else:
        result = -1  # Sentinel
    return result


def maybe_correct_book_name(
    lang_code: str,
    book_name: str,
) -> str:
    """
    Translate incorrect or undesirable book names to a preferred form.
    """
    book_name_ = BOOK_NAME_CORRECTION_TABLE.get((lang_code, book_name), "")
    if not book_name_:
        book_name_ = book_name
    return book_name_


def _demote_heading(match: Match[str], levels: int) -> str:
    tag = match.group(0)
    level = int(match.group(1))
    new_level = min(level + levels, 6)
    return tag.replace(f"h{level}", f"h{new_level}", 1)


def demote_headings_by_one(content: str) -> str:
    return HEADING_RE.sub(
        lambda m: _demote_heading(m, levels=1),
        content,
    )


if __name__ == "__main__":

    # To run the doctests in this module, in the root of the project do:
    # python backend/document/domain/resource_lookup.py
    # or
    # python backend/document/domain/resource_lookup.py -v
    # See https://docs.python.org/3/library/doctest.html
    # for more details.
    import doctest

    doctest.testmod()
