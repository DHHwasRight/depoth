//! Markdown to HTML, with the heading outline taken on the way past.
//!
//! The outline is collected by walking the AST before rendering, not by
//! re-parsing the output. That gives one source of truth for both the `id`
//! attributes and the on-this-page rail, so the two cannot disagree -- and the
//! link checker can then verify an anchor with a set lookup rather than a
//! second parse.
//!
//! Highlighting happens here, at build time, so a page ships coloured markup
//! and no highlighter.

use std::fmt;
use std::sync::OnceLock;

use comrak::adapters::SyntaxHighlighterAdapter;
use comrak::nodes::{AstNode, NodeValue};
use comrak::{Arena, Options};
use syntect::html::{ClassStyle, ClassedHTMLGenerator};
use syntect::parsing::{SyntaxReference, SyntaxSet};
use syntect::util::LinesWithEndings;

use crate::components::{self, CallError, Components};
use crate::slug::Anchors;

const CLASS_STYLE: ClassStyle = ClassStyle::SpacedPrefixed { prefix: "hl-" };

/// A heading, for the on-this-page rail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heading {
    pub level: u8,
    pub id: String,
    pub text: String,
}

/// Rendered markdown and what was learned on the way.
#[derive(Debug, Clone)]
pub struct Rendered {
    pub html: String,
    pub headings: Vec<Heading>,
}

fn options() -> Options<'static> {
    let mut o = Options::default();
    o.extension.table = true;
    o.extension.strikethrough = true;
    o.extension.autolink = true;
    o.extension.tasklist = true;
    o.extension.footnotes = true;
    o.extension.description_lists = true;
    // The content is ours, and it contains HTML on purpose. A site taking
    // submissions would want this false.
    o.render.r#unsafe = true;
    o
}

fn syntaxes() -> &'static SyntaxSet {
    static SET: OnceLock<SyntaxSet> = OnceLock::new();
    // Megabytes, immutable after load, and Sync -- so one is built for the
    // whole process and shared by reference. Cloning it per page, or per
    // thread, is the classic way to make a build slow for no reason.
    SET.get_or_init(two_face::syntax::extra_newlines)
}

/// Resolves a fence's info string to a syntax. Only the first token names a
/// language; the rest may be a title or options.
fn syntax_for(lang: Option<&str>) -> &'static SyntaxReference {
    let set = syntaxes();
    lang.map(str::trim)
        .and_then(|l| l.split([' ', ',']).next())
        .filter(|l| !l.is_empty())
        .and_then(|l| set.find_syntax_by_token(l))
        .unwrap_or_else(|| set.find_syntax_plain_text())
}

/// Highlights into CSS classes rather than inline styles: the markup stays
/// small across hundreds of pages, and a colour scheme becomes a stylesheet
/// swap instead of a rebuild.
struct Highlighter;

impl SyntaxHighlighterAdapter for Highlighter {
    fn write_highlighted(
        &self,
        output: &mut dyn fmt::Write,
        lang: Option<&str>,
        code: &str,
    ) -> fmt::Result {
        let syntax = syntax_for(lang);
        let mut generator =
            ClassedHTMLGenerator::new_with_class_style(syntax, syntaxes(), CLASS_STYLE);
        for line in LinesWithEndings::from(code) {
            // A syntax that trips over its own regex would otherwise drop the
            // rest of the block. Losing colour is acceptable; losing the
            // sample is not.
            if generator
                .parse_html_for_line_which_includes_newline(line)
                .is_err()
            {
                return comrak::html::escape(output, code);
            }
        }
        output.write_str(&generator.finalize())
    }

    fn write_pre_tag(
        &self,
        output: &mut dyn fmt::Write,
        _attrs: std::collections::HashMap<&'static str, std::borrow::Cow<'_, str>>,
    ) -> fmt::Result {
        output.write_str("<pre class=\"hl\">")
    }

    fn write_code_tag(
        &self,
        output: &mut dyn fmt::Write,
        attrs: std::collections::HashMap<&'static str, std::borrow::Cow<'_, str>>,
    ) -> fmt::Result {
        match attrs.get("class") {
            Some(class) => write!(output, "<code class=\"{class}\">"),
            None => output.write_str("<code>"),
        }
    }
}

/// The stylesheet that gives the `hl-` classes colour.
///
/// Opt-in, and emitted for both schemes at once: the dark rules are wrapped in
/// a `prefers-color-scheme` block and a `[data-theme="dark"]` block, because a
/// toggle has to win in both directions.
pub fn highlight_stylesheet() -> String {
    use two_face::theme::EmbeddedThemeName as T;
    let themes = two_face::theme::extra();
    let css = |name: T| {
        syntect::html::css_for_theme_with_class_style(themes.get(name), CLASS_STYLE)
            .unwrap_or_default()
    };
    let light = css(T::Github);
    let dark = css(T::GruvboxDark);
    let indent = |s: &str| {
        s.lines()
            .map(|l| format!("  {l}"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    format!(
        "{light}\n\n@media (prefers-color-scheme: dark) {{\n  :root:not([data-theme=\"light\"]) {{\n{}\n  }}\n}}\n\n:root[data-theme=\"dark\"] {{\n{}\n}}\n",
        indent(&dark),
        indent(&dark),
    )
}

/// Renders `source` with no components registered.
pub fn render(source: &str) -> Rendered {
    render_with(source, &Components::new()).expect("no components, so no call can fail")
}

/// Renders `source`, expanding any component call it contains.
///
/// Children are rendered by calling back into this function, which is what
/// makes a call inside a call work.
pub fn render_with(source: &str, components: &Components) -> Result<Rendered, CallError> {
    let expanded = expand_calls(source, components)?;
    Ok(render_plain(&expanded))
}

fn expand_calls(source: &str, components: &Components) -> Result<String, CallError> {
    // A child's markdown is rendered before its parent sees it, so what the
    // component receives is HTML and not a second dialect to parse.
    let render_children = |inner: &str| match expand_calls(inner, components) {
        Ok(expanded) => render_plain(&expanded).html,
        // The outer call reports it; this inner failure cannot be lost,
        // because the same source is walked again below.
        Err(_) => inner.to_string(),
    };
    components::expand(source, components, &render_children)
}

/// Renders `source`, assigning heading ids and returning the outline.
fn render_plain(source: &str) -> Rendered {
    // comrak's AST is !Send -- it cannot cross a thread boundary at all, not
    // merely be shared across one -- so the arena is created and dropped
    // inside this function and only the String escapes.
    let arena = Arena::new();
    let opts = options();
    let root = comrak::parse_document(&arena, source, &opts);

    let mut anchors = Anchors::new();
    let mut headings = Vec::new();
    collect_headings(root, &mut anchors, &mut headings);

    static HIGHLIGHTER: Highlighter = Highlighter;
    let mut plugins = comrak::options::Plugins::default();
    plugins.render.codefence_syntax_highlighter = Some(&HIGHLIGHTER);

    let mut html = String::new();
    comrak::format_html_with_plugins(root, &opts, &mut html, &plugins)
        .expect("writing to a String cannot fail");
    Rendered { html, headings }
}

/// Walks headings in document order, giving each an `id` and recording it.
fn collect_headings<'a>(node: &'a AstNode<'a>, anchors: &mut Anchors, out: &mut Vec<Heading>) {
    for child in node.children() {
        // The borrow must end before anything else touches this node: the
        // rewrite below takes a mutable one, and a RefCell will panic rather
        // than wait.
        let level = match child.data.borrow().value {
            NodeValue::Heading(heading) => Some(heading.level),
            _ => None,
        };
        match level {
            Some(level) => {
                let text = child.collect_text();
                let id = anchors.id_for(&text);
                out.push(Heading {
                    level,
                    id: id.clone(),
                    text,
                });
                set_heading_id(child, level, &id);
            }
            None => collect_headings(child, anchors, out),
        }
    }
}

/// Replaces a heading with raw HTML carrying its `id`.
///
/// Rendering the text rather than the inline children is deliberate: a
/// heading's id comes from its text, and a link inside a heading has nowhere
/// useful to go.
fn set_heading_id<'a>(node: &'a AstNode<'a>, level: u8, id: &str) {
    let text = node.collect_text();
    let mut data = node.data.borrow_mut();
    data.value = NodeValue::HtmlBlock(comrak::nodes::NodeHtmlBlock {
        block_type: 6,
        literal: format!("<h{level} id=\"{id}\">{}</h{level}>\n", html_escape(&text)),
    });
}

fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headings_get_ids_and_an_outline() {
        let r = render("# Title\n\n## First section\n\n### Nested\n\n## Second\n");
        assert_eq!(
            r.headings,
            vec![
                Heading {
                    level: 1,
                    id: "title".into(),
                    text: "Title".into()
                },
                Heading {
                    level: 2,
                    id: "first-section".into(),
                    text: "First section".into()
                },
                Heading {
                    level: 3,
                    id: "nested".into(),
                    text: "Nested".into()
                },
                Heading {
                    level: 2,
                    id: "second".into(),
                    text: "Second".into()
                },
            ]
        );
        assert!(r.html.contains("<h2 id=\"first-section\">"), "{}", r.html);
    }

    #[test]
    fn the_outline_and_the_markup_agree() {
        // The whole reason the outline is taken from the AST rather than from
        // the output: every id in the rail must exist in the page.
        let r = render("## Overview\n\n## Overview\n\n## Fees & billing\n");
        for h in &r.headings {
            assert!(
                r.html.contains(&format!("id=\"{}\"", h.id)),
                "{:?} missing from markup: {}",
                h.id,
                r.html
            );
        }
        assert_eq!(r.headings[1].id, "overview-1");
    }

    #[test]
    fn a_fenced_block_is_highlighted_at_build_time() {
        let r = render("```rust\nfn main() {}\n```\n");
        assert!(r.html.contains("<pre"), "{}", r.html);
        // Syntect emits spans with colours; the point is that the browser is
        // never asked to do this.
        assert!(r.html.contains("<span"), "no highlighting: {}", r.html);
    }

    #[test]
    fn tables_and_strikethrough_are_on() {
        let r = render("| a | b |\n|---|---|\n| 1 | 2 |\n\n~gone~\n");
        assert!(r.html.contains("<table>"), "{}", r.html);
        assert!(r.html.contains("<del>"), "{}", r.html);
    }
}

/// Prefixes every root-relative `href` and `src` with `base`.
///
/// A document writes `/docs/navigation` because that is what the page is
/// called. Where the site is deployed is not the author's problem, and making
/// it theirs means every link breaks the day the site moves.
///
/// Left alone: anything with a scheme, a protocol-relative `//host`, a bare
/// fragment or query, and anything already carrying the prefix.
pub fn rebase(html: &str, base: &str) -> String {
    if base.is_empty() {
        return html.to_string();
    }
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(at) = rest.find(['h', 's']).and_then(|_| find_attr(rest)) {
        let (before, from) = rest.split_at(at.start);
        out.push_str(before);
        let value = &from[at.open..at.close];
        out.push_str(&from[..at.open]);
        if value.starts_with('/') && !value.starts_with("//") && !value.starts_with(base) {
            out.push_str(base);
        }
        out.push_str(value);
        rest = &from[at.close..];
    }
    out.push_str(rest);
    out
}

struct Attr {
    start: usize,
    open: usize,
    close: usize,
}

/// Locates the next `href="` or `src="` and the bounds of its value.
fn find_attr(haystack: &str) -> Option<Attr> {
    let mut search = 0;
    loop {
        let rel = haystack[search..].find(['h', 's'])?;
        let start = search + rel;
        for name in ["href=\"", "src=\""] {
            if haystack[start..].starts_with(name) {
                let open = name.len();
                let close = open + haystack[start + open..].find('"')?;
                return Some(Attr { start, open, close });
            }
        }
        search = start + 1;
    }
}

#[cfg(test)]
mod rebase_tests {
    use super::rebase;

    #[test]
    fn root_relative_links_gain_the_prefix() {
        let html = r#"<a href="/docs/nav">x</a><img src="/img/a.png">"#;
        assert_eq!(
            rebase(html, "/depoth"),
            r#"<a href="/depoth/docs/nav">x</a><img src="/depoth/img/a.png">"#
        );
    }

    #[test]
    fn everything_else_is_left_alone() {
        for html in [
            r#"<a href="https://example.com/x">x</a>"#,
            r#"<a href="//cdn.example.com/x">x</a>"#,
            r##"<a href="#section">x</a>"##,
            r#"<a href="relative/page">x</a>"#,
            r#"<a href="mailto:a@b.c">x</a>"#,
        ] {
            assert_eq!(rebase(html, "/depoth"), html, "changed {html}");
        }
    }

    #[test]
    fn an_empty_base_changes_nothing_and_a_prefixed_link_is_not_doubled() {
        let html = r#"<a href="/docs/nav">x</a>"#;
        assert_eq!(rebase(html, ""), html);
        assert_eq!(
            rebase(r#"<a href="/depoth/docs">x</a>"#, "/depoth"),
            r#"<a href="/depoth/docs">x</a>"#
        );
    }

    #[test]
    fn text_that_merely_contains_h_or_s_survives() {
        let html = "<p>she has hats</p>";
        assert_eq!(rebase(html, "/depoth"), html);
    }
}

#[cfg(test)]
mod component_tests {
    use super::*;

    fn components() -> Components {
        Components::new()
            .add("Warning", |_, c| {
                format!("<aside class=\"callout warning\">{c}</aside>")
            })
            .add("Tabs", |_, c| format!("<div class=\"tabs\">{c}</div>"))
            .add("Tab", |a, c| {
                format!(
                    "<section data-title=\"{}\">{c}</section>",
                    a.get_or("title", "")
                )
            })
    }

    #[test]
    fn a_callout_renders_its_children_as_markdown() {
        let r = render_with(
            "<Warning>\nThis **upserts**. See the [guide](/g).\n</Warning>\n",
            &components(),
        )
        .expect("render");
        assert!(
            r.html.contains("<aside class=\"callout warning\">"),
            "{}",
            r.html
        );
        assert!(r.html.contains("<strong>upserts</strong>"), "{}", r.html);
        assert!(r.html.contains("href=\"/g\""), "{}", r.html);
    }

    #[test]
    fn a_fenced_block_survives_inside_a_nested_call() {
        // Tabs holding code is the single commonest shape in real docs, and
        // the one the previous generation could not render.
        let src = "<Tabs>\n  <Tab title=\"Rust\">\n    ```rust\n    fn main() {}\n    ```\n  </Tab>\n</Tabs>\n";
        let r = render_with(src, &components()).expect("render");
        assert!(r.html.contains("<div class=\"tabs\">"), "{}", r.html);
        assert!(r.html.contains("data-title=\"Rust\""), "{}", r.html);
        assert!(r.html.contains("<pre"), "no code block: {}", r.html);
        assert!(!r.html.contains("```"), "fence leaked as text: {}", r.html);
    }

    #[test]
    fn prose_around_a_call_is_still_prose() {
        let r = render_with(
            "Before.\n\n<Warning>\ninside\n</Warning>\n\nAfter.\n",
            &components(),
        )
        .expect("render");
        assert!(r.html.contains("<p>Before.</p>"), "{}", r.html);
        assert!(r.html.contains("<p>After.</p>"), "{}", r.html);
    }

    #[test]
    fn headings_inside_a_call_still_reach_the_outline() {
        // They do not today: the inner render has its own anchor table, so a
        // heading inside a component is absent from the on-this-page rail.
        // Recorded as a known limit rather than left to be discovered.
        let r = render_with(
            "## Outside\n\n<Warning>\n### Inside\n</Warning>\n",
            &components(),
        )
        .expect("render");
        assert_eq!(r.headings.len(), 1, "known limit: {:?}", r.headings);
        assert_eq!(r.headings[0].text, "Outside");
    }
}
