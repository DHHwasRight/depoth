//! The components a document may call.
//!
//! Four callouts, not six. Across a real 489-page corpus, `Check`, `Danger`
//! and the generic `Callout` were used zero times, while `Info` alone appeared
//! 231 times. Shipping a component nobody writes costs a name in the
//! namespace and a row in the documentation.
//!
//! Each maps onto a variant the stylesheet already defines, so a callout never
//! picks its own colours: one hue is the accent and every other hue means
//! exactly one thing.
//!
//! # A container that reads its own children
//!
//! `Tabs` and `CodeGroup` both need something the mechanism does not hand
//! them: the labels of their children. A component is called with its
//! attributes and with its children **already rendered to HTML**, innermost
//! first, so by the time `Tabs` runs, every `Tab` inside it is a string of
//! markup and nothing else.
//!
//! So the container reads that markup. `Tab` leaves its label on the panel it
//! emits (`<section class="mdx-tab" data-mdx-tab="...">`) and `Tabs` scans for
//! those sections, lifts the labels off and rebuilds the whole widget.
//! `CodeGroup` does the same over `<pre class="hl">`, which is what depoth's
//! own highlighter emits, and takes each label from the `language-` class
//! comrak puts on the `<code>` inside.
//!
//! What it costs, in both directions:
//!
//! * The scan is string work over HTML, not a tree walk. It is safe because
//!   the strings it looks for are ones we emit ourselves, and because it
//!   counts opening tags to find a matching close -- but it would be wrong the
//!   day a `Tab` stopped being a `<section>`. Those two facts are a contract,
//!   and the tests below are where it is written down.
//! * `CodeGroup` labels a tab by its **language**, because that is all that
//!   survives. comrak reduces a fence's info string to `class="language-rust"`
//!   before anyone sees it, so a `title="Cargo.toml"` written after the
//!   language is already gone. Recovering it means teaching depoth's
//!   highlighter to carry the rest of the info string onto the `<pre>` -- the
//!   docs template already reads a `data-filename` from one -- which is a
//!   change to the markdown crate rather than to this file.
//!
//! # Why every component flattens its own output
//!
//! A component's HTML goes back into the document and is parsed as markdown
//! again. A blank line ends a raw HTML block there, so a code sample with an
//! empty line in it -- inside a `Tab`, inside a `Step` -- would cut its own
//! container in half and the rest would be read as prose. `no_blank_lines`
//! replaces each blank line with `&#10;`, which is the same character to the
//! DOM and to a copy, and not a blank line to the parser.

use depoth::components::{Attrs, Components};

/// `Info` and `Note` are the same box. Mintlify ships both and real documents
/// use both, so accepting both costs nothing and rejecting one costs an edit
/// on every page that migrates.
const CALLOUTS: [(&str, &str, &str); 5] = [
    ("Note", "note", "Note"),
    ("Info", "note", "Note"),
    ("Tip", "tip", "Tip"),
    ("Warning", "caution", "Warning"),
    ("Check", "success", "Check"),
];

/// What a component does: attributes and already-rendered children in, HTML
/// out.
type Render = fn(&Attrs, &str) -> String;

/// Everything else, in the order PLAN §5 measured it.
///
/// One table, read by both `components` and `component_names`, because the
/// render cache keys on the names: a component registered but missing from
/// the list would be served from a cache that never saw it.
///
/// `Columns` is `CardGroup`. Mintlify renamed it and real corpora still use
/// the old name, so accepting both costs one row.
const BLOCKS: [(&str, Render); 11] = [
    ("Frame", frame),
    ("CodeGroup", code_group),
    ("Card", card),
    ("CardGroup", card_group),
    ("Columns", card_group),
    ("Accordion", accordion),
    ("AccordionGroup", accordion_group),
    ("Steps", steps),
    ("Step", step),
    ("Tabs", tabs),
    ("Tab", tab),
];

/// How many tabs a widget may have before the stylesheet runs out of rules.
/// Eight is twice the largest `CodeGroup` in the corpus.
const MAX_TABS: usize = 8;

/// Every name a document may call, for the render cache's signature.
pub fn component_names() -> Vec<&'static str> {
    CALLOUTS
        .iter()
        .map(|(tag, _, _)| *tag)
        .chain(BLOCKS.iter().map(|(tag, _)| *tag))
        .collect()
}

pub fn components() -> Components {
    let mut set = Components::new();
    for (tag, variant, default_title) in CALLOUTS {
        set = add(set, tag, move |attrs: &Attrs, children: &str| {
            let title = attrs.get_or("title", default_title);
            format!(
                "<aside class=\"callout {variant}\">\
                   <p class=\"callout__title\">{}</p>\
                   <div class=\"callout__body\">{children}</div>\
                 </aside>",
                escape(title)
            )
        });
    }
    for (tag, render) in BLOCKS {
        set = add(set, tag, render);
    }
    set
}

/// Registers a component, flattening its output first.
///
/// Every component goes through here, because every component's HTML is
/// parsed as markdown one more time on the way out.
fn add<F>(set: Components, name: &str, f: F) -> Components
where
    F: Fn(&Attrs, &str) -> String + 'static,
{
    set.add(name, move |attrs, children| {
        no_blank_lines(&f(attrs, children))
    })
}

// ------------------------------------------------------------------- Frame

/// A screenshot, with an optional line above it and a caption below.
///
/// The caption is markdown, because a caption that cannot hold a link or a
/// `code span` gets written as a paragraph outside the frame instead.
fn frame(attrs: &Attrs, children: &str) -> String {
    let hint = match attrs.get("hint").filter(|h| !h.is_empty()) {
        Some(h) => format!("<p class=\"mdx-frame__hint\">{}</p>", escape(h)),
        None => String::new(),
    };
    let caption = match attrs.get("caption").filter(|c| !c.is_empty()) {
        Some(c) => format!(
            "<figcaption class=\"mdx-frame__caption\">{}</figcaption>",
            inline_markdown(c)
        ),
        None => String::new(),
    };
    format!(
        "<figure class=\"mdx-frame\">{hint}\
           <div class=\"mdx-frame__media\">{}</div>\
         {caption}</figure>",
        video_defaults(children)
    )
}

/// Gives every `<video>` the three attributes a documentation clip always
/// wants. A clip with sound, or one that stops after a single pass, is a bug
/// report every time.
fn video_defaults(html: &str) -> String {
    const WANT: [&str; 3] = ["playsinline", "loop", "muted"];
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(at) = rest.find("<video") {
        out.push_str(&rest[..at]);
        let tail = &rest[at..];
        let Some(end) = tag_end(tail) else {
            break;
        };
        let tag = &tail[..end];
        let (body, slash) = match tag.trim_end().strip_suffix('/') {
            Some(b) => (b, " /"),
            None => (tag, ""),
        };
        out.push_str(body.trim_end());
        for want in WANT {
            if !has_word(tag, want) {
                out.push(' ');
                out.push_str(want);
            }
        }
        out.push_str(slash);
        out.push('>');
        rest = &tail[end + 1..];
    }
    out.push_str(rest);
    out
}

// -------------------------------------------------------------- Tabs, Tab

/// One panel, carrying its own label until `Tabs` lifts it off.
///
/// A `Tab` written outside a `Tabs` still renders its content; it is then a
/// plain section with a data attribute nothing reads.
fn tab(attrs: &Attrs, children: &str) -> String {
    format!(
        "<section class=\"mdx-tab\" data-mdx-tab=\"{}\">{children}</section>",
        escape(attrs.get_or("title", ""))
    )
}

fn tabs(_: &Attrs, children: &str) -> String {
    let (panels, loose) = blocks(children, "section", "data-mdx-tab=\"");
    let items: Vec<(String, String)> = panels
        .iter()
        .map(|b| {
            // The label was escaped when the panel was built, so it goes back
            // out as it came in. Escaping it twice is how `&` becomes `&amp;`
            // on the page.
            let label = attr(b.open, "data-mdx-tab").unwrap_or_default();
            (label.to_string(), b.inner.to_string())
        })
        .collect();
    widget("mdx-tabs--prose", &items, &loose)
}

// --------------------------------------------------------------- CodeGroup

/// Several fenced blocks, shown as one tabbed widget.
///
/// The blocks arrive already highlighted, so the scan is for the `<pre>` that
/// depoth's highlighter emits and the label is the language on the `<code>`
/// inside it. See the module header for what that costs.
fn code_group(_: &Attrs, children: &str) -> String {
    let (found, loose) = blocks(children, "pre", "");
    let items: Vec<(String, String)> = found
        .iter()
        .enumerate()
        .map(|(i, b)| {
            let label = match language_of(b.full) {
                Some(lang) => escape(lang),
                // A fence with no language still deserves a tab a reader can
                // aim at.
                None => format!("Example {}", i + 1),
            };
            (label, b.full.to_string())
        })
        .collect();
    widget("mdx-tabs--code", &items, &loose)
}

/// `<code class="language-rust">` -> `rust`.
fn language_of(block: &str) -> Option<&str> {
    const PAT: &str = "<code class=\"language-";
    let start = block.find(PAT)? + PAT.len();
    let len = block[start..].find('"')?;
    Some(&block[start..start + len])
}

// ----------------------------------------------------------- the tab strip

/// The markup shared by `Tabs` and `CodeGroup`.
///
/// A radio group, because the project's hardest constraint is that every page
/// works with JavaScript off, and a radio group is the only native control
/// that already behaves like a tab strip: one of N is selected, and the arrow
/// keys move between them. The panels are shown by `:checked` and a sibling
/// selector, so switching a tab is the browser's own work.
///
/// `aria-selected` is deliberately absent. CSS cannot rewrite an attribute, so
/// the only honest choices are a value that is right once and wrong after the
/// first keypress, or none at all. Instead the selected tab carries a
/// visually-hidden word that CSS reveals, which makes its accessible name say
/// what is true at the moment it is read.
fn widget(variant: &str, items: &[(String, String)], loose: &str) -> String {
    let before = if loose.trim().is_empty() {
        String::new()
    } else {
        loose.to_string()
    };
    if items.is_empty() {
        return before;
    }
    let group = group_id(items);
    let mut list = String::new();
    let mut panels = String::new();
    for (i, (label, body)) in items.iter().enumerate().take(MAX_TABS) {
        let checked = if i == 0 { " checked" } else { "" };
        list.push_str(&format!(
            "<input class=\"mdx-tabs__input\" type=\"radio\" role=\"tab\" \
               name=\"{group}\" id=\"{group}-t{i}\" aria-controls=\"{group}-p{i}\"{checked}>\
             <label class=\"mdx-tabs__tab\" role=\"presentation\" for=\"{group}-t{i}\">\
               {label}<span class=\"mdx-tabs__state\"> (selected)</span></label>"
        ));
        panels.push_str(&format!(
            "<div class=\"mdx-tabs__panel\" id=\"{group}-p{i}\" role=\"tabpanel\" \
               aria-labelledby=\"{group}-t{i}\" tabindex=\"0\">{body}</div>"
        ));
    }
    // Past the cap the extra panels are still published, just not switchable.
    // Losing a tab is a layout bug; losing the content is a documentation bug.
    let overflow: String = items
        .iter()
        .skip(MAX_TABS)
        .map(|(_, body)| body.as_str())
        .collect();
    format!(
        "{before}<div class=\"mdx-tabs {variant}\">\
           <div class=\"mdx-tabs__list\" role=\"tablist\">{list}</div>\
           <div class=\"mdx-tabs__panels\">{panels}</div>\
         </div>{overflow}"
    )
}

/// A name for the radio group, derived from the widget's own content.
///
/// It has to be unique on the page and identical between two builds of the
/// same page. A counter the renderer keeps is neither: the build caches
/// pages, so which documents are re-rendered would decide what the numbers
/// are. A hash of the content is both. Two widgets with byte-identical
/// content do collide, and then they switch together -- which is the one case
/// where that is hard to notice.
fn group_id(items: &[(String, String)]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for (label, body) in items {
        for byte in label.bytes().chain([0]).chain(body.bytes()).chain([0]) {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x100_0000_01b3);
        }
    }
    format!("tab-{hash:011x}")
}

// --------------------------------------------------------- Card, CardGroup

/// A link card.
///
/// The link wraps the title only, and a pseudo-element stretches it over the
/// whole card. Wrapping the card in an `<a>` instead would make every link a
/// reader writes in the card's body a link inside a link, which is invalid and
/// unreachable by keyboard.
fn card(attrs: &Attrs, children: &str) -> String {
    let title = escape(attrs.get_or("title", ""));
    let icon = attrs.get_or("icon", "");
    let horizontal = if attrs.flag("horizontal") {
        " mdx-card--horizontal"
    } else {
        ""
    };
    // depoth ships no icon set. A glyph an author typed is printed; a name
    // from an icon font is kept as a hook so a project that has one can map
    // it, because printing the word "rocket" in a circle is worse than
    // printing nothing.
    let data_icon = if icon.is_empty() {
        String::new()
    } else {
        format!(" data-icon=\"{}\"", escape(icon))
    };
    let chip = if is_glyph(icon) {
        format!(
            "<span class=\"mdx-card__icon\" aria-hidden=\"true\">{}</span>",
            escape(icon)
        )
    } else {
        String::new()
    };
    let heading = match attrs.get("href").filter(|h| !h.is_empty()) {
        Some(href) => format!(
            "<p class=\"mdx-card__title\">\
               <a class=\"mdx-card__link\" href=\"{}\">{title}</a></p>",
            escape(href)
        ),
        None => format!("<p class=\"mdx-card__title\">{title}</p>"),
    };
    format!(
        "<div class=\"mdx-card{horizontal}\"{data_icon}>{chip}{heading}\
           <div class=\"mdx-card__body\">{children}</div></div>"
    )
}

fn card_group(attrs: &Attrs, children: &str) -> String {
    let cols = attrs
        .get("cols")
        .and_then(|c| c.trim().parse::<u8>().ok())
        .unwrap_or(2)
        .clamp(1, 4);
    format!("<div class=\"mdx-cards\" data-cols=\"{cols}\">{children}</div>")
}

/// Whether an icon value is something a browser can draw on its own: an emoji
/// or an arrow, rather than `rocket`.
fn is_glyph(icon: &str) -> bool {
    !icon.is_empty()
        && !icon
            .chars()
            .any(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

// ---------------------------------------------- Accordion, AccordionGroup

/// A disclosure, which `<details>` already is. No script, no state to keep,
/// and a browser following a link into one opens it on the way.
fn accordion(attrs: &Attrs, children: &str) -> String {
    let title = attrs.get_or("title", "");
    let id = match attrs.get("id").filter(|i| !i.is_empty()) {
        Some(id) => id.to_string(),
        // Derived from the title with the same slugger the headings use, so a
        // section keeps its link across an edit that does not rename it.
        None => depoth::slug::slugify(title),
    };
    let id_attr = if id.is_empty() {
        String::new()
    } else {
        format!(" id=\"{}\"", escape(&id))
    };
    let open = if attrs.flag("defaultOpen") || attrs.flag("open") {
        " open"
    } else {
        ""
    };
    format!(
        "<details class=\"mdx-accordion\"{id_attr}{open}>\
           <summary class=\"mdx-accordion__summary\">\
             <span class=\"mdx-accordion__title\">{}</span></summary>\
           <div class=\"mdx-accordion__body\">{children}</div></details>",
        escape(title)
    )
}

fn accordion_group(_: &Attrs, children: &str) -> String {
    format!("<div class=\"mdx-accordions\">{children}</div>")
}

// ------------------------------------------------------------ Steps, Step

/// An ordered list, so the numbers are the browser's.
///
/// The chip's number is `counter(list-item)`, which every `<ol>` scopes to
/// itself. A counter kept here instead would be one sequence for the whole
/// page, and the second step of a procedure inside the second tab would be
/// numbered four.
fn steps(_: &Attrs, children: &str) -> String {
    format!("<ol class=\"mdx-steps\">{children}</ol>")
}

fn step(attrs: &Attrs, children: &str) -> String {
    let title = attrs.get_or("title", "");
    // Not a heading. The on-this-page rail and the harness both read heading
    // levels, and a step is not a section of the document.
    let heading = if title.is_empty() {
        String::new()
    } else {
        format!("<p class=\"mdx-step__title\">{}</p>", escape(title))
    };
    format!("<li class=\"mdx-step\">{heading}<div class=\"mdx-step__body\">{children}</div></li>")
}

// ---------------------------------------------------------------- scanning

/// One block found in a child's rendered HTML.
struct Block<'a> {
    /// The opening tag, `<section class="..." data-mdx-tab="One">`.
    open: &'a str,
    /// What sat between the tags.
    inner: &'a str,
    /// Tags included.
    full: &'a str,
}

/// Every top-level `<tag ...>...</tag>` in `html` whose opening tag contains
/// `marker`, plus everything that sat outside them.
///
/// Openings are counted so a block that contains another of its own kind ends
/// in the right place. The strings being matched are ones this module emits,
/// which is what makes scanning markup with `find` defensible here and nowhere
/// else.
fn blocks<'a>(html: &'a str, tag: &str, marker: &str) -> (Vec<Block<'a>>, String) {
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let mut found = Vec::new();
    let mut outside = String::new();
    let mut i = 0;
    while let Some(rel) = html[i..].find(&open) {
        let start = i + rel;
        let Some(end) = tag_end(&html[start..]) else {
            break;
        };
        let after_open = start + end + 1;
        if !html[start..after_open].contains(marker) {
            outside.push_str(&html[i..after_open]);
            i = after_open;
            continue;
        }
        let mut depth = 1usize;
        let mut j = after_open;
        let inner_end = loop {
            let next_open = html[j..].find(&open).map(|n| j + n);
            let next_close = html[j..].find(&close).map(|n| j + n);
            match (next_open, next_close) {
                (_, None) => break html.len(),
                (Some(o), Some(c)) if o < c => {
                    depth += 1;
                    j = o + open.len();
                }
                (_, Some(c)) => {
                    depth -= 1;
                    if depth == 0 {
                        break c;
                    }
                    j = c + close.len();
                }
            }
        };
        outside.push_str(&html[i..start]);
        let block_end = (inner_end + close.len()).min(html.len());
        found.push(Block {
            open: &html[start..after_open],
            inner: &html[after_open..inner_end],
            full: &html[start..block_end],
        });
        i = block_end;
    }
    outside.push_str(&html[i..]);
    (found, outside)
}

/// The offset of the `>` that ends an opening tag, ignoring one inside a
/// quoted value.
fn tag_end(tag: &str) -> Option<usize> {
    let mut quoted = false;
    for (i, c) in tag.char_indices() {
        match c {
            '"' => quoted = !quoted,
            '>' if !quoted => return Some(i),
            _ => {}
        }
    }
    None
}

/// The value of `name="..."` in an opening tag, still escaped.
fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let pat = format!("{name}=\"");
    let start = tag.find(&pat)? + pat.len();
    let len = tag[start..].find('"')?;
    Some(&tag[start..start + len])
}

/// Whether `tag` carries `word` as an attribute of its own.
fn has_word(tag: &str, word: &str) -> bool {
    tag.split(|c: char| c.is_whitespace() || c == '=' || c == '"')
        .any(|t| t.eq_ignore_ascii_case(word))
}

// ----------------------------------------------------------------- strings

/// Markdown for a value written in an attribute, as inline content.
///
/// comrak gives a paragraph back; a caption is already inside a `<figcaption>`
/// and does not want a second block around it.
fn inline_markdown(source: &str) -> String {
    let html = depoth::markdown::render(source).html;
    let trimmed = html.trim();
    match trimmed
        .strip_prefix("<p>")
        .and_then(|s| s.strip_suffix("</p>"))
    {
        Some(inner) if !inner.contains("<p>") => inner.to_string(),
        _ => trimmed.to_string(),
    }
}

/// Removes every blank line, keeping the character it stood for.
///
/// See the module header: the output of a component is parsed as markdown
/// again, and a blank line ends a raw HTML block there. `&#10;` is a newline
/// to the DOM and to anything that copies the text, and is not a blank line to
/// the parser -- so a code sample with an empty line in it survives being put
/// inside a `Tab`.
fn no_blank_lines(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut first = true;
    for line in html.split('\n') {
        if line.trim().is_empty() {
            out.push_str("&#10;");
            out.push_str(line);
        } else {
            if !first {
                out.push('\n');
            }
            out.push_str(line);
        }
        first = false;
    }
    out
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(source: &str) -> String {
        depoth::markdown::render_with(source, &components())
            .expect("every call closes")
            .html
    }

    fn assert_has(html: &str, needle: &str) {
        assert!(html.contains(needle), "missing {needle:?} in:\n{html}");
    }

    /// Every value of an attribute, in document order.
    fn values_of(html: &str, pat: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut rest = html;
        while let Some(at) = rest.find(pat) {
            let after = &rest[at + pat.len()..];
            let end = after.find('"').expect("a quoted value closes");
            out.push(after[..end].to_string());
            rest = &after[end..];
        }
        out
    }

    // --------------------------------------------------------------- Frame

    #[test]
    fn a_frame_wraps_its_children_and_captions_them_in_markdown() {
        let html = render(
            "<Frame caption=\"A **wide** shot\" hint=\"After step 3\">\n![alt](/a.png)\n</Frame>\n",
        );
        assert_has(&html, "<figure class=\"mdx-frame\">");
        assert_has(&html, "<p class=\"mdx-frame__hint\">After step 3</p>");
        assert_has(&html, "<div class=\"mdx-frame__media\">");
        assert_has(&html, "src=\"/a.png\"");
        assert_has(
            &html,
            "<figcaption class=\"mdx-frame__caption\">A <strong>wide</strong> shot</figcaption>",
        );
    }

    #[test]
    fn a_video_in_a_frame_plays_inline_and_loops_without_sound() {
        let html = render("<Frame>\n<video src=\"/demo.mp4\"></video>\n</Frame>\n");
        assert_has(&html, "<video src=\"/demo.mp4\" playsinline loop muted>");
    }

    #[test]
    fn a_video_keeps_the_attributes_it_already_had() {
        let html = render("<Frame>\n<video muted controls src=\"/d.mp4\" />\n</Frame>\n");
        assert_has(&html, "playsinline");
        assert_eq!(html.matches("muted").count(), 1, "{html}");
    }

    // ---------------------------------------------------------------- Tabs

    #[test]
    fn tabs_become_one_strip_and_one_panel_each() {
        let html = render(
            "<Tabs>\n  <Tab title=\"npm\">\n    use npm\n  </Tab>\n  <Tab title=\"pnpm\">\n    use pnpm\n  </Tab>\n</Tabs>\n",
        );
        // The strip the old implementation could not build: a container that
        // knows its children's titles.
        assert_has(&html, "<div class=\"mdx-tabs__list\" role=\"tablist\">");
        assert_eq!(html.matches("role=\"tab\"").count(), 2, "{html}");
        assert_eq!(html.matches("role=\"tabpanel\"").count(), 2, "{html}");
        assert_has(&html, ">npm<span class=\"mdx-tabs__state\"");
        assert_has(&html, ">pnpm<span class=\"mdx-tabs__state\"");
        // No per-panel heading any more, and no leftover marker section.
        assert!(!html.contains("mdx-tab__title"), "{html}");
        assert!(!html.contains("data-mdx-tab"), "{html}");
        // One radio group, first tab selected, nothing selected twice.
        let names = values_of(&html, "name=\"");
        assert_eq!(names.len(), 2, "{html}");
        assert_eq!(names[0], names[1], "{html}");
        assert_eq!(html.matches(" checked>").count(), 1, "{html}");
        assert_has(&html, "use npm");
        assert_has(&html, "use pnpm");
    }

    #[test]
    fn a_tab_strip_wires_each_tab_to_its_own_panel() {
        let html = render("<Tabs>\n<Tab title=\"One\">\nfirst\n</Tab>\n</Tabs>\n");
        let group = values_of(&html, "name=\"")[0].clone();
        assert_has(
            &html,
            &format!("id=\"{group}-t0\" aria-controls=\"{group}-p0\""),
        );
        assert_has(
            &html,
            &format!("id=\"{group}-p0\" role=\"tabpanel\" aria-labelledby=\"{group}-t0\""),
        );
        assert_has(&html, &format!("for=\"{group}-t0\""));
    }

    #[test]
    fn a_blank_line_inside_a_sample_does_not_cut_its_tab_in_half() {
        // The failure this guards against is silent: the HTML block ends at
        // the blank line and the rest of the sample is read as prose.
        let html = render(
            "<Tabs>\n  <Tab title=\"Rust\">\n    ```rust\n    let a = 1;\n\n    let b = 2;\n    ```\n  </Tab>\n</Tabs>\n",
        );
        assert_eq!(html.matches("<pre").count(), 1, "{html}");
        assert_eq!(html.matches("</pre>").count(), 1, "{html}");
        // Both statements are in the one block, and the empty line between
        // them is still a newline -- as a character reference, so the parser
        // never saw a blank line.
        assert_has(&html, "&#10;");
        assert_eq!(html.matches(">let<").count(), 2, "{html}");
        assert_has(&html, "role=\"tabpanel\"");
    }

    // ----------------------------------------------------------- CodeGroup

    #[test]
    fn a_code_group_labels_each_tab_with_its_language() {
        let html = render(
            "<CodeGroup>\n\n```rust\nfn main() {}\n```\n\n```json\n{}\n```\n\n</CodeGroup>\n",
        );
        assert_has(&html, "mdx-tabs--code");
        assert_eq!(html.matches("role=\"tabpanel\"").count(), 2, "{html}");
        assert_has(&html, ">rust<span class=\"mdx-tabs__state\"");
        assert_has(&html, ">json<span class=\"mdx-tabs__state\"");
        assert_eq!(html.matches("<pre class=\"hl\">").count(), 2, "{html}");
    }

    #[test]
    fn a_fence_with_no_language_still_gets_a_tab() {
        let html = render("<CodeGroup>\n\n```\nplain\n```\n\n</CodeGroup>\n");
        assert_has(&html, ">Example 1<span");
    }

    // ------------------------------------------------------ Card, CardGroup

    #[test]
    fn a_card_is_a_box_with_one_stretched_link() {
        let html =
            render("<Card title=\"Install\" icon=\"🚀\" href=\"/install\">\nGet going.\n</Card>\n");
        assert_has(&html, "<div class=\"mdx-card\" data-icon=\"🚀\">");
        assert_has(
            &html,
            "<span class=\"mdx-card__icon\" aria-hidden=\"true\">🚀</span>",
        );
        assert_has(
            &html,
            "<a class=\"mdx-card__link\" href=\"/install\">Install</a>",
        );
        assert_has(&html, "Get going.");
    }

    #[test]
    fn an_icon_name_is_a_hook_and_not_a_word_in_a_circle() {
        let html = render("<Card title=\"Go\" icon=\"rocket\" />\n");
        assert_has(&html, "data-icon=\"rocket\"");
        assert!(!html.contains("mdx-card__icon"), "{html}");
    }

    #[test]
    fn a_card_group_takes_the_mdx_brace_form_for_its_columns() {
        let html = render("<CardGroup cols={3}>\n<Card title=\"A\" />\n</CardGroup>\n");
        assert_has(&html, "<div class=\"mdx-cards\" data-cols=\"3\">");
    }

    #[test]
    fn columns_is_the_old_name_for_a_card_group() {
        let html = render("<Columns cols={4}>\n<Card title=\"A\" />\n</Columns>\n");
        assert_has(&html, "<div class=\"mdx-cards\" data-cols=\"4\">");
    }

    #[test]
    fn a_column_count_outside_one_to_four_is_clamped() {
        let html = render("<CardGroup cols={9}>\n<Card title=\"A\" />\n</CardGroup>\n");
        assert_has(&html, "data-cols=\"4\"");
    }

    #[test]
    fn a_card_is_horizontal_on_request() {
        let html = render("<Card title=\"A\" horizontal />\n");
        assert_has(&html, "class=\"mdx-card mdx-card--horizontal\"");
    }

    // ------------------------------------------- Accordion, AccordionGroup

    #[test]
    fn an_accordion_is_a_details_with_a_link_to_itself() {
        let html = render(
            "<AccordionGroup>\n  <Accordion title=\"Why not six?\">\n    Because.\n  </Accordion>\n  <Accordion title=\"Open me\" defaultOpen>\n    Already open.\n  </Accordion>\n</AccordionGroup>\n",
        );
        assert_has(&html, "<div class=\"mdx-accordions\">");
        assert_has(
            &html,
            "<details class=\"mdx-accordion\" id=\"why-not-six\">",
        );
        assert_has(
            &html,
            "<details class=\"mdx-accordion\" id=\"open-me\" open>",
        );
        assert_has(&html, "<summary class=\"mdx-accordion__summary\">");
        assert_has(&html, "Because.");
    }

    #[test]
    fn an_accordion_keeps_an_id_its_author_wrote() {
        let html = render("<Accordion title=\"Anything\" id=\"rate-limits\">\nx\n</Accordion>\n");
        assert_has(&html, "id=\"rate-limits\"");
    }

    // --------------------------------------------------------------- Steps

    #[test]
    fn steps_are_an_ordered_list_so_the_browser_numbers_them() {
        let html = render(
            "<Steps>\n  <Step title=\"Install\">\n    Run it.\n  </Step>\n  <Step title=\"Build\">\n    Then this.\n  </Step>\n</Steps>\n",
        );
        assert_has(&html, "<ol class=\"mdx-steps\">");
        assert_eq!(html.matches("<li class=\"mdx-step\">").count(), 2, "{html}");
        assert_has(&html, "<p class=\"mdx-step__title\">Install</p>");
        // Nothing in the markup carries a number: the chip is a CSS counter.
        assert!(!html.contains("data-step"), "{html}");
        // And a step title is not a heading, so it cannot skip a level.
        assert!(!html.contains("<h3"), "{html}");
    }

    // ------------------------------------------------------------- nesting

    #[test]
    fn a_card_inside_a_card_group_inside_a_step_inside_a_tab_survives() {
        let html = render(
            "<Tabs>\n  <Tab title=\"Setup\">\n    <Steps>\n      <Step title=\"Pick one\">\n        <CardGroup cols={2}>\n          <Card title=\"A\" href=\"/a\">one</Card>\n          <Card title=\"B\" href=\"/b\">two</Card>\n        </CardGroup>\n      </Step>\n    </Steps>\n  </Tab>\n</Tabs>\n",
        );
        assert_has(&html, "role=\"tablist\"");
        assert_has(&html, "<ol class=\"mdx-steps\">");
        assert_has(&html, "<div class=\"mdx-cards\" data-cols=\"2\">");
        assert_eq!(html.matches("mdx-card__link").count(), 2, "{html}");
        // The outer widget consumed its own panels and no one else's.
        assert_eq!(html.matches("role=\"tabpanel\"").count(), 1, "{html}");
    }

    /// Two tab strips, one inside the other.
    ///
    /// The pair is a `CodeGroup` inside a `Tab` rather than a `Tabs` inside a
    /// `Tab`, because the second does not survive `find_close` in
    /// `depoth::components`: it looks for the next `<Tabs` and `</Tabs>`, and
    /// `<Tab` is a prefix of `<Tabs`, so an opening `<Tabs>` inside a `<Tab>`
    /// is counted as a second `<Tab>` and the call is reported as never
    /// closed. The same holds for `Steps` in a `Step` and `CardGroup` in a
    /// `Card`. It is a bug in the mechanism, not in these components, and the
    /// fix is a word boundary after the name there.
    #[test]
    fn two_tab_strips_one_inside_the_other_keep_their_own_groups() {
        let html = render(
            "<Tabs>\n  <Tab title=\"Rust\">\n    <CodeGroup>\n\n    ```rust\n    fn a() {}\n    ```\n\n    ```json\n    {}\n    ```\n\n    </CodeGroup>\n  </Tab>\n  <Tab title=\"Go\">\n    plain\n  </Tab>\n</Tabs>\n",
        );
        let names = values_of(&html, "name=\"");
        assert_eq!(names.len(), 4, "{html}");
        // Two groups: two code tabs inside, two prose tabs outside.
        let mut unique = names.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), 2, "{html}");
        // The outer strip took its own panels and left the inner widget whole.
        assert_eq!(html.matches("mdx-tabs--code").count(), 1, "{html}");
        assert_eq!(html.matches("mdx-tabs--prose").count(), 1, "{html}");
        assert_eq!(html.matches("role=\"tabpanel\"").count(), 4, "{html}");
        assert!(!html.contains("data-mdx-tab"), "{html}");
    }

    // ------------------------------------------------------------ escaping

    #[test]
    fn an_attribute_with_markup_in_it_cannot_break_the_page() {
        let html = render(
            "<Tabs>\n<Tab title=\"Fish & <chips>\">\nx\n</Tab>\n</Tabs>\n\n<Card title=\"A & B\" href=\"/a?x=1&y=2\" />\n\n<Accordion title=\"5 < 6\">\ny\n</Accordion>\n",
        );
        // Escaped exactly once, in the label the container lifted off the
        // panel -- the markup never reaches the page as markup.
        assert_has(&html, ">Fish &amp; &lt;chips&gt;<span");
        assert!(!html.contains("<chips>"), "{html}");
        assert_has(&html, "href=\"/a?x=1&amp;y=2\"");
        assert_has(&html, ">A &amp; B</a>");
        assert_has(
            &html,
            "<span class=\"mdx-accordion__title\">5 &lt; 6</span>",
        );
    }

    // --------------------------------------------------------------- units

    #[test]
    fn a_blank_line_becomes_a_character_reference() {
        assert_eq!(no_blank_lines("a\n\nb"), "a&#10;\nb");
        assert_eq!(no_blank_lines("a\nb"), "a\nb");
        assert_eq!(no_blank_lines("a\n  \nb"), "a&#10;  \nb");
    }

    #[test]
    fn every_name_the_cache_knows_about_is_registered() {
        // The render cache keys on this list. A name that is registered but
        // unlisted would be served from a cache that never saw it.
        let set = components();
        for name in component_names() {
            assert!(set.contains(name), "{name} is listed but not registered");
        }
        assert_eq!(component_names().len(), CALLOUTS.len() + BLOCKS.len());
    }

    #[test]
    fn a_block_scan_counts_its_own_openings() {
        let html =
            "<section class=\"mdx-tab\" data-mdx-tab=\"A\">x<section>y</section>z</section>tail";
        let (found, loose) = blocks(html, "section", "data-mdx-tab=\"");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].inner, "x<section>y</section>z");
        assert_eq!(loose, "tail");
    }
}
