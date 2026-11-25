//! Markdown to HTML (what DOC does with `mistune.markdown(text)`).
//!
//! mistune 3.2.0 defaults (READ `markdown(text, escape=True, renderer="html", plugins=None)`):
//! CommonMark-style blocks, no tables, no strikethrough, raw HTML in the source is
//! escaped. The engine below is chosen by measurement (report section 3).

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Engine {
    Comrak,
}

thread_local! {
    static ORACLE: std::cell::RefCell<Option<std::collections::HashMap<String, String>>> = const { std::cell::RefCell::new(None) };
}

/// Test hook: answer `to_html` from a table of DOC's own (markdown, HTML) pairs where one
/// exists. Used by the harness to separate "the markdown engine differs" from "a reader or
/// transformer differs". Never set in production use.
pub fn set_oracle(table: Option<std::collections::HashMap<String, String>>) {
    ORACLE.with(|o| *o.borrow_mut() = table);
}

pub fn to_html(md: &str) -> String {
    let hit = ORACLE.with(|o| o.borrow().as_ref().and_then(|t| t.get(md).cloned()));
    hit.unwrap_or_else(|| to_html_with(Engine::Comrak, md))
}

pub fn to_html_with(engine: Engine, md: &str) -> String {
    match engine {
        Engine::Comrak => comrak_html(md),
    }
}

fn comrak_html(md: &str) -> String {
    use comrak::nodes::{Ast, NodeValue};
    use comrak::{format_html, parse_document, Arena};
    use std::cell::RefCell;

    let mut options = comrak::Options::default();
    options.render.escape = true;
    let arena = Arena::new();
    let root = parse_document(&arena, md, &options);
    // mistune (escape=True) renders a raw HTML block as an escaped paragraph; comrak
    // renders it as bare escaped text. Rewrite those nodes before formatting.
    for node in root.descendants() {
        let literal = match &node.data.borrow().value {
            NodeValue::HtmlBlock(h) => Some(h.literal.clone()),
            _ => None,
        };
        if let Some(lit) = literal {
            node.data.borrow_mut().value = NodeValue::Paragraph;
            let text = lit.trim_end_matches('\n').to_string();
            let child = arena.alloc(comrak::arena_tree::Node::new(RefCell::new(Ast::new(NodeValue::Text(text.into()), (0, 0).into()))));
            node.append(child);
        }
    }
    let mut out = Vec::new();
    if format_html(root, &options, &mut out).is_err() {
        return String::new();
    }
    mistune_list_whitespace(&String::from_utf8_lossy(&out))
}

/// mistune writes `<li><p>..` and `<li>text<ul>` where cmark-style renderers put a
/// newline (`<li>\n<p>`, `<li>text\n<ul>`).
fn mistune_list_whitespace(html: &str) -> String {
    use crate::pyre::PyRegex;
    use std::sync::OnceLock;
    static NESTED: OnceLock<PyRegex> = OnceLock::new();
    let nested = NESTED.get_or_init(|| PyRegex::new(r"(<li>(?:(?!</li>|<p>)[^\n])*)\n(<[uo]l>)", false).expect("nested-list pattern"));
    let step = html.replace("<li>\n<p>", "<li><p>").replace("<li>\n<pre>", "<li><pre>").replace("<li>\n<blockquote>", "<li><blockquote>");
    let mut out = String::with_capacity(step.len());
    let mut last = 0;
    for m in nested.finditer(&step).unwrap_or_default() {
        out.push_str(&step[last..m.start]);
        out.push_str(m.group(1).unwrap_or(""));
        out.push_str(m.group(2).unwrap_or(""));
        last = m.end;
    }
    out.push_str(&step[last..]);
    out
}
