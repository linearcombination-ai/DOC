"""The two style tables: CURRENT (reverse-engineered from template.docx) and PROPOSED
(stakeholder requirements + proposed-doc-format.docx)."""

from style_table import Page, Profile, Style

BLACK = "000000"


def _headings(
    h1: Style | None, h2: Style, h3: Style, h4: Style, h5: Style
) -> tuple[Style, ...]:
    return tuple(s for s in (h1, h2, h3, h4, h5) if s is not None)


COMMON_TAIL = (
    Style("ListBullet", "List Bullet", list_kind="bullet", contextual_spacing=True, q_format=False),
    Style("ListBullet2", "List Bullet 2", list_kind="bullet", contextual_spacing=True, q_format=False,
          tabs=(("clear", 720),), left_in=0.0, first_line_in=0.0),
    Style("ListNumber", "List Number", list_kind="number", contextual_spacing=True, q_format=False, left_in=0.5),
    Style("LicenseText", "License Text", size_pt=10, after_pt=0, q_format=False),
    Style("Header", "header", tabs=(("center", 4680), ("right", 9360)), after_pt=0, q_format=False),
    Style("Footer", "footer", tabs=(("center", 4680), ("right", 9360)), after_pt=0, q_format=False),
    Style("PlainHyperlink", "PlainHyperlink", color=BLACK, underline=True),
    Style("PlainHyperlinkChar0", "PlainHyperlinkChar", kind="character", based_on="DefaultParagraphFont",
          color=BLACK, underline=True),
)

CURRENT = Profile(
    name="current",
    font="Aptos", size_pt=12, line=("auto", 278 / 240), after_pt=8,
    styles=(
        Style("Normal", "Normal", based_on=None, css="p", size_pt=11, line=("auto", 259 / 240)),
        Style("Title", "Title", next="BodyText", css="h1", font="Aptos Display", bold=True, color=BLACK, size_pt=18,
              align="center", before_pt=24, after_pt=12, keep_next=True, keep_lines=True),
        Style("Subtitle", "Subtitle", based_on="Title", next="BodyText", size_pt=15, before_pt=12),
        *_headings(
            None,
            Style("Heading2", "heading 2", next="BodyText", css="h2", bold=True, color=BLACK, size_pt=13,
                  before_pt=8, after_pt=0, keep_next=True, keep_lines=True, outline=1),
            Style("Heading3", "heading 3", next="BodyText", css="h3", bold=True, color=BLACK,
                  before_pt=8, after_pt=0, keep_next=True, keep_lines=True, outline=2),
            Style("Heading4", "heading 4", next="BodyText", css="h4", bold=True, color=BLACK,
                  before_pt=4, after_pt=0, left_in=0.2, keep_next=True, keep_lines=True, outline=3),
            Style("Heading5", "heading 5", next="BodyText", css="h5", bold=True, color=BLACK,
                  before_pt=10, after_pt=0, keep_next=True, keep_lines=True, outline=4),
        ),
        Style("BodyText", "Body Text", first_line_in=0.25),
        *COMMON_TAIL,
    ),
)

PROPOSED = Profile(
    name="proposed",
    font="Noto Sans", size_pt=12, line=("auto", 1.0), after_pt=4,
    page=Page(top_in=1.0, bottom_in=1.0, left_in=1.0, right_in=1.0),
    styles=(
        Style("Normal", "Normal", based_on=None, css="p"),
        Style("UlbText", "ULB Text", css="p.ulb", line=("exact", 15.0), after_pt=4),
        Style("Title", "Title", next="BodyText", css="h1.title", bold=True, color=BLACK, size_pt=16,
              align="center", before_pt=15, after_pt=8, keep_next=True, keep_lines=True),
        Style("Subtitle", "Subtitle", based_on="Title", next="BodyText", size_pt=14, before_pt=12),
        *_headings(
            Style("Heading1", "heading 1", next="BodyText", css="h1", bold=True, color=BLACK, size_pt=16,
                  align="center", before_pt=15, after_pt=8, keep_next=True, keep_lines=True, outline=0),
            Style("Heading2", "heading 2", next="BodyText", css="h2", bold=True, color=BLACK, size_pt=13,
                  before_pt=8, after_pt=0, keep_next=True, keep_lines=True, outline=1),
            Style("Heading3", "heading 3", next="BodyText", css="h3", bold=True, color=BLACK, size_pt=12,
                  before_pt=8, after_pt=0, keep_next=True, keep_lines=True, outline=2),
            Style("Heading4", "heading 4", next="BodyText", css="h4", bold=True, color=BLACK, size_pt=12,
                  before_pt=4, after_pt=0, left_in=0.2, keep_next=True, keep_lines=True, outline=3),
            Style("Heading5", "heading 5", next="BodyText", css="h5", bold=True, color=BLACK, size_pt=12,
                  before_pt=10, after_pt=0, keep_next=True, keep_lines=True, outline=4),
        ),
        Style("IndentedText", "Indented Text", css="h4 + p, h4 + ul", left_in=0.2, after_pt=4),
        Style("BodyText", "Body Text", first_line_in=0.25),
        *COMMON_TAIL,
    ),
)
