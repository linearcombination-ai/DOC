"""The proposed-look style table (P3b): the earlier PROPOSED table (stakeholder text) extended with the
styles the sample uses and that the core's semantic classes need. Style names are the sample's where the
sample has the style ("Indented definition text", "Line space", "List Number 2/3", "List Bullet 3").
One table: `styles_xml()` writes the docx styles, `css()` the PDF rules (selectors in `Style.css`)."""

from __future__ import annotations

import style_table
from profiles_p3 import BLACK, COMMON_TAIL, PROPOSED
from style_table import Profile, Style

style_table.CUSTOM_IDS.update({"IndentedDefinitionText", "LineSpace", "UlbPoetry1", "UlbPoetry2", "UlbChapterNumber", "VerseNumber", "UlbText"})

_base = tuple(s for s in PROPOSED.styles if s.id not in {"IndentedText", "UlbText", "Heading1"})

LOOK = Profile(
    name="proposed-look",
    font=PROPOSED.font,
    size_pt=PROPOSED.size_pt,
    line=PROPOSED.line,
    after_pt=PROPOSED.after_pt,
    page=PROPOSED.page,
    styles=(
        *_base,
        # Heading 1: no space before; the extra line above comes from a "Line space" paragraph (the sample's way)
        Style("Heading1", "heading 1", next="BodyText", css="h1.resource-title", bold=True, color=BLACK, size_pt=16,
              align="center", before_pt=0, after_pt=8, keep_next=True, keep_lines=True, outline=0),
        Style("LineSpace", "Line space", css="p.line-space", line=("exact", 15.0), after_pt=0),
        Style("IndentedDefinitionText", "Indented definition text", css="p.indented", left_in=0.2, after_pt=4),
        Style("UlbText", "ULB Text", css="p.ulb-text", line=("exact", 15.0), after_pt=4),
        Style("UlbPoetry1", "ULB Poetry 1", css="p.ulb-poetry-1", line=("exact", 15.0), after_pt=0),
        Style("UlbPoetry2", "ULB Poetry 2", css="p.ulb-poetry-2", line=("exact", 15.0), after_pt=0, first_line_in=0.5),
        Style("UlbChapterNumber", "ULB Chapter Number", css="p.ulb-chapter-num", size_pt=31.5, line=("exact", 30.0), after_pt=0, keep_next=True, bold=False),
        Style("VerseNumber", "Verse Number", kind="character", based_on="DefaultParagraphFont", css="sup.vn", vert_align="superscript"),
        Style("ListNumber2", "List Number 2", list_kind="number", contextual_spacing=True, q_format=False, left_in=0.5),
        Style("ListNumber3", "List Number 3", list_kind="number", contextual_spacing=True, q_format=False, left_in=0.75),
        Style("ListBullet3", "List Bullet 3", list_kind="bullet", contextual_spacing=True, q_format=False, left_in=0.5),
    ),
)
