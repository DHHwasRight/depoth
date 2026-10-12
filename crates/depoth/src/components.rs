//! Calling a typed component from inside a document.
//!
//! A document writes a capitalised tag:
//!
//! ```markdown
//! <Warning>
//! This endpoint **upserts**. Calling it twice is not an error.
//! </Warning>
//! ```
//!
//! and gets whatever the project decided `Warning` renders as -- here, an rsx
//! component with typed parameters.
//!
//! Three rules, each of which the Crystal predecessor learned the hard way.
//!
//! **Calls nest.** `<Tab>` indented inside `<Tabs>` is a call, not literal
//! markup. The previous generation only recognised a tag at the start of a
//! line, which broke exactly the three container components a documentation
//! site is built from, and was described at the time as "the main thing
//! standing between this and a faithful rebuild".
//!
//! **Children are markdown.** What sits between the tags is rendered as a
//! document, recursively, so a callout can contain a list and a tab can
//! contain a fenced block.
//!
//! **Indentation is removed first.** Content indented to sit under its opening
//! tag would otherwise read as an indented code block, which is the most
//! common way a nested call silently turns into a grey box.
//!
//! Attributes arrive as strings, because markdown is read at run time and the
//! component was compiled. That is a real limit of the mechanism and the
//! reason a render hook is the better tool whenever one will do: a hook takes
//! the markup out of every document at once, where a call puts it into one.

use std::collections::HashMap;

/// Attributes written on a call, before any coercion.
#[derive(Debug, Default, Clone)]
pub struct Attrs(HashMap<String, String>);

impl Attrs {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).map(String::as_str)
    }

    /// The value, or `default` when the author did not write one.
    pub fn get_or<'a>(&'a self, key: &str, default: &'a str) -> &'a str {
        self.get(key).unwrap_or(default)
    }

    /// Whether a bare attribute is present: `<Card wide>` or `wide="true"`.
    pub fn flag(&self, key: &str) -> bool {
        matches!(self.get(key), Some("") | Some("true"))
    }
}

type Render = Box<dyn Fn(&Attrs, &str) -> String>;

/// The components a document may call.
#[derive(Default)]
pub struct Components {
    map: HashMap<String, Render>,
}

impl Components {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers `name`, rendered by `f` from its attributes and its
    /// already-rendered children.
    pub fn add<F>(mut self, name: &str, f: F) -> Self
    where
        F: Fn(&Attrs, &str) -> String + 'static,
    {
        self.map.insert(name.to_string(), Box::new(f));
        self
    }

    pub fn contains(&self, name: &str) -> bool {
        self.map.contains_key(name)
    }

    fn render(&self, name: &str, attrs: &Attrs, children: &str) -> Option<String> {
        self.map.get(name).map(|f| f(attrs, children))
    }
}

/// A call that could not be rendered, with enough detail to fix it.
#[derive(Debug, PartialEq, Eq)]
pub struct CallError {
    pub tag: String,
    pub message: String,
}

impl std::error::Error for CallError {}

impl std::fmt::Display for CallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<{}>: {}", self.tag, self.message)
    }
}

/// Replaces every component call in `source` with its rendered HTML.
///
/// `render_children` turns the markdown between a call's tags into HTML. It is
/// passed in rather than called directly so this module does not depend on the
/// markdown renderer, and so the renderer can expand calls inside children by
/// calling back in.
pub fn expand(
    source: &str,
    components: &Components,
    render_children: &dyn Fn(&str) -> String,
) -> Result<String, CallError> {
    let mut out = String::with_capacity(source.len());
    let mut rest = source;

    loop {
        let Some(found) = next_call(rest, components) else {
            out.push_str(rest);
            return Ok(out);
        };
        out.push_str(&rest[..found.at]);
        let after_open = found.at + found.open_len;

        let (children, after) = if found.self_closing {
            (String::new(), after_open)
        } else {
            let Some(close) = find_close(&rest[after_open..], &found.name) else {
                return Err(CallError {
                    tag: found.name,
                    message: "is never closed".into(),
                });
            };
            let inner = dedent(&rest[after_open..after_open + close.at]);
            (render_children(&inner), after_open + close.end)
        };

        let html = components
            .render(&found.name, &found.attrs, &children)
            .expect("only registered names are matched");
        // Blank lines around the block keep comrak from folding it into a
        // surrounding paragraph.
        out.push_str("\n\n");
        out.push_str(&html);
        out.push_str("\n\n");
        rest = &rest[after..];
    }
}

struct Call {
    at: usize,
    open_len: usize,
    name: String,
    attrs: Attrs,
    self_closing: bool,
}

/// Finds the next registered call, skipping fenced code.
fn next_call(src: &str, components: &Components) -> Option<Call> {
    let mut i = 0;
    let mut fence: Option<String> = None;
    for line in src.split_inclusive('\n') {
        let trimmed = line.trim_start();
        let marker = fence_marker(trimmed);
        match (&fence, marker) {
            // A sample of depoth's own syntax must survive being documented.
            (None, Some(m)) => fence = Some(m),
            (Some(open), Some(m)) if m == *open => fence = None,
            _ => {}
        }
        if fence.is_none() {
            if let Some(call) = call_in(src, i, line, components) {
                return Some(call);
            }
        }
        i += line.len();
    }
    None
}

fn fence_marker(trimmed: &str) -> Option<String> {
    for m in ["```", "~~~"] {
        if trimmed.starts_with(m) {
            return Some(m.to_string());
        }
    }
    None
}

fn call_in(src: &str, line_start: usize, line: &str, components: &Components) -> Option<Call> {
    let mut offset = 0;
    while let Some(rel) = line[offset..].find('<') {
        let at = line_start + offset + rel;
        if let Some(call) = parse_open(src, at, components) {
            return Some(call);
        }
        offset += rel + 1;
    }
    None
}

/// Parses `<Name attr="v" ...>` or `<Name ... />` at `at`.
fn parse_open(src: &str, at: usize, components: &Components) -> Option<Call> {
    let rest = &src[at..];
    let body = rest.strip_prefix('<')?;
    // A capital letter is what separates <Card> from <div>, and leaves plain
    // HTML to markdown.
    if !body.starts_with(|c: char| c.is_ascii_uppercase()) {
        return None;
    }
    let name_len = body
        .find(|c: char| !(c.is_alphanumeric() || c == '_'))
        .unwrap_or(body.len());
    let name = &body[..name_len];
    if !components.contains(name) {
        return None;
    }
    // The tag ends at the first `>` that is not inside a quoted value.
    let mut in_quote = false;
    let mut end = None;
    for (i, c) in body[name_len..].char_indices() {
        match c {
            '"' => in_quote = !in_quote,
            '>' if !in_quote => {
                end = Some(name_len + i);
                break;
            }
            _ => {}
        }
    }
    let end = end?;
    let raw = &body[name_len..end];
    let self_closing = raw.trim_end().ends_with('/');
    Some(Call {
        at,
        open_len: 1 + end + 1,
        name: name.to_string(),
        attrs: parse_attrs(raw.trim_end().trim_end_matches('/')),
        self_closing,
    })
}

fn parse_attrs(raw: &str) -> Attrs {
    let mut map = HashMap::new();
    let mut rest = raw.trim();
    while !rest.is_empty() {
        let key_len = rest
            .find(|c: char| c.is_whitespace() || c == '=')
            .unwrap_or(rest.len());
        let key = rest[..key_len].trim().to_string();
        rest = rest[key_len..].trim_start();
        if let Some(after_eq) = rest.strip_prefix('=') {
            let after_eq = after_eq.trim_start();
            // `cols={2}` is MDX for `cols="2"`, and real corpora are full of it.
            let (value, consumed) = match after_eq.chars().next() {
                Some('"') => take_until(&after_eq[1..], '"'),
                Some('{') => take_until(&after_eq[1..], '}'),
                _ => {
                    let n = after_eq.find(char::is_whitespace).unwrap_or(after_eq.len());
                    (after_eq[..n].to_string(), n)
                }
            };
            let skip = if matches!(after_eq.chars().next(), Some('"') | Some('{')) {
                consumed + 2
            } else {
                consumed
            };
            rest = after_eq[skip.min(after_eq.len())..].trim_start();
            map.insert(key, value.trim_matches('"').to_string());
        } else if !key.is_empty() {
            map.insert(key, String::new());
        }
    }
    Attrs(map)
}

fn take_until(s: &str, end: char) -> (String, usize) {
    match s.find(end) {
        Some(n) => (s[..n].to_string(), n),
        None => (s.to_string(), s.len()),
    }
}

struct Close {
    at: usize,
    end: usize,
}

/// Finds `</Name>`, counting nested `<Name>` openings so a component can
/// contain another of its own kind.
fn find_close(src: &str, name: &str) -> Option<Close> {
    let open = format!("<{name}");
    let close = format!("</{name}>");
    let mut depth = 1usize;
    let mut i = 0;
    // A tag name must end at the opening tag's boundary. Matching by prefix
    // counts `<Tabs>` as a second `<Tab>`, so the outer call is reported
    // never closed and the whole region is emitted raw -- and `Tabs` in a
    // `Tab`, `Steps` in a `Step` and `CardGroup` in a `Card` are all ordinary.
    let opens_here = |at: usize| {
        src[at + open.len()..]
            .chars()
            .next()
            .is_none_or(|c| !(c.is_alphanumeric() || c == '_'))
    };
    while i < src.len() {
        let next_open = src[i..]
            .match_indices(&open)
            .map(|(n, _)| i + n)
            .find(|at| opens_here(*at));
        let next_close = src[i..].find(&close).map(|n| i + n);
        match (next_open, next_close) {
            (_, None) => return None,
            (Some(o), Some(c)) if o < c => {
                depth += 1;
                i = o + open.len();
            }
            (_, Some(c)) => {
                depth -= 1;
                if depth == 0 {
                    return Some(Close {
                        at: c,
                        end: c + close.len(),
                    });
                }
                i = c + close.len();
            }
        }
    }
    None
}

/// Removes the indentation shared by every non-blank line.
///
/// Without this, content indented to sit under its opening tag reads as an
/// indented code block -- the commonest way a nested call turns silently into
/// a grey box.
fn dedent(text: &str) -> String {
    let indent = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start().len())
        .min()
        .unwrap_or(0);
    if indent == 0 {
        return text.to_string();
    }
    text.lines()
        .map(|l| {
            if l.len() >= indent {
                &l[indent..]
            } else {
                l.trim_start()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Stands in for the markdown renderer. It recurses, which is the whole
    /// contract: children are expanded by calling back in, so a call inside a
    /// call is found. Here it skips the markdown so the tests measure the call
    /// machinery alone.
    fn raw(s: &str) -> String {
        expand(s, &components(), &raw)
            .expect("expand")
            .trim()
            .to_string()
    }

    fn components() -> Components {
        Components::new()
            .add("Warning", |a, c| {
                format!(
                    "<div class=\"callout {}\">{c}</div>",
                    a.get_or("kind", "warning")
                )
            })
            .add("Tabs", |_, c| format!("<div class=\"tabs\">{c}</div>"))
            .add("Tab", |a, c| {
                format!("<section title=\"{}\">{c}</section>", a.get_or("title", ""))
            })
            .add("Icon", |a, _| format!("<i>{}</i>", a.get_or("name", "")))
    }

    fn run(src: &str) -> String {
        expand(src, &components(), &raw).expect("expand")
    }

    #[test]
    fn a_call_becomes_its_components_output() {
        let out = run("<Warning>\nbe careful\n</Warning>\n");
        assert!(
            out.contains("<div class=\"callout warning\">be careful</div>"),
            "{out}"
        );
    }

    #[test]
    fn calls_nest_even_when_indented() {
        // The case the Crystal predecessor could not do, and the reason Tabs,
        // Steps and AccordionGroup were all broken there.
        let out = run("<Tabs>\n  <Tab title=\"One\">\n    first\n  </Tab>\n</Tabs>\n");
        assert!(out.contains("<div class=\"tabs\">"), "{out}");
        assert!(
            out.contains("<section title=\"One\">first</section>"),
            "{out}"
        );
    }

    #[test]
    fn a_component_may_contain_one_of_its_own_kind() {
        let out = run("<Warning>\nouter\n<Warning>\ninner\n</Warning>\n</Warning>\n");
        assert_eq!(out.matches("class=\"callout").count(), 2, "{out}");
    }

    #[test]
    fn a_self_closing_call_has_no_children() {
        let out = run("<Icon name=\"star\" />\n");
        assert!(out.contains("<i>star</i>"), "{out}");
    }

    #[test]
    fn mdx_brace_values_are_accepted() {
        // Real corpora are full of cols={2}, which is MDX for cols="2".
        let attrs = parse_attrs(r#"cols={2} title="Hi there" wide"#);
        assert_eq!(attrs.get("cols"), Some("2"));
        assert_eq!(attrs.get("title"), Some("Hi there"));
        assert!(attrs.flag("wide"));
    }

    #[test]
    fn a_lowercase_tag_is_left_to_markdown() {
        let out = run("<div>plain html</div>\n");
        assert_eq!(out, "<div>plain html</div>\n");
    }

    #[test]
    fn an_unregistered_tag_is_left_alone() {
        // Otherwise documenting a component would require implementing it.
        let out = run("<Unknown>text</Unknown>\n");
        assert_eq!(out, "<Unknown>text</Unknown>\n");
    }

    #[test]
    fn a_call_inside_a_fence_is_a_code_sample() {
        // depoth's own documentation has to be able to show this syntax.
        let src = "```markdown\n<Warning>\nnot a call\n</Warning>\n```\n";
        assert_eq!(run(src), src);
    }

    #[test]
    fn an_unclosed_call_names_itself() {
        let err = expand("<Warning>\noops\n", &components(), &raw).expect_err("should fail");
        assert_eq!(
            err,
            CallError {
                tag: "Warning".into(),
                message: "is never closed".into()
            }
        );
    }

    #[test]
    fn indentation_shared_by_every_line_is_removed() {
        assert_eq!(dedent("    a\n      b\n\n    c"), "a\n  b\n\nc");
    }
}

#[cfg(test)]
mod boundary_tests {
    use super::*;

    fn set() -> Components {
        Components::new()
            .add("Tab", |_, c| format!("<section>{c}</section>"))
            .add("Tabs", |_, c| format!("<div class=\"tabs\">{c}</div>"))
    }

    fn render(s: &str) -> String {
        expand(s, &set(), &|inner| render(inner).trim().to_string()).expect("expand")
    }

    #[test]
    fn a_longer_tag_sharing_a_prefix_does_not_close_the_shorter_one() {
        // `<Tabs>` inside `<Tab>` was counted as a nested `<Tab>`, so the
        // outer call was reported never closed and the region was emitted
        // raw. `Steps` in a `Step` and `CardGroup` in a `Card` are the same
        // shape, and all three are ordinary documentation markup.
        let out = render("<Tab>\nouter\n<Tabs>\ninner\n</Tabs>\n</Tab>\n");
        assert!(out.contains("<section>"), "outer never rendered: {out}");
        assert!(
            out.contains("<div class=\"tabs\">"),
            "inner never rendered: {out}"
        );
        assert!(!out.contains("&lt;Tabs&gt;"), "emitted raw: {out}");
        assert!(!out.contains("<Tabs>"), "emitted raw: {out}");
    }
}
