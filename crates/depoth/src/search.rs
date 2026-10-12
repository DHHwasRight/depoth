//! Build-time search index, and the client that queries it.
//!
//! # Why this is hand-written and not `pagefind`
//!
//! §6 of the plan takes `pagefind` as a library at +166 crates, on the grounds
//! that a search costs ~35 KB and stays flat as the corpus grows. That trade
//! is read at 500 pages. This site has eleven. Eleven pages of prose is 37 KB
//! of markdown; the whole index below is smaller than one `pagefind` fragment
//! fetch, so the flat-cost argument buys nothing yet and the 166 crates and
//! ~27 s of cold build are paid on every CI run from today.
//!
//! So: an inverted index, built here, queried by `site/assets/js/search.js`.
//! About 300 lines of Rust and 450 of JavaScript, no new dependency, and the
//! serialised form is plain JSON that anything can read.
//!
//! **Revisit at roughly 150 pages**, where the index stops fitting in one
//! fetch and sharding or `pagefind` starts to earn the dependency. The switch
//! is cheap because nothing outside this module knows the format. If it is
//! made, the manifest needs exactly:
//!
//! ```toml
//! [dependencies]
//! pagefind = { version = "1.5", optional = true }   # NOT default-features = false
//!
//! [features]
//! pagefind-index = ["dep:pagefind"]
//! ```
//!
//! and this module goes behind `#[cfg(feature = "pagefind-index")]`. The
//! `default-features = false` note is not style: pagefind 1.5.2 fails to
//! compile without them, because its own tokio dependency omits `io-util`.
//!
//! # Measured, on this site
//!
//! Ten documentation pages, 37 042 bytes of markdown source:
//!
//! ```text
//! pages                  10      sections            62
//! distinct terms      1 163      postings         3 860
//! index.json         83 971 bytes   (2.27x the markdown source)
//!   of which: postings 47 090, section text 35 515, documents 1 471
//! over gzip -9       24 144 bytes   <- what a first search actually costs
//! ```
//!
//! The whole index is one fetch, taken lazily on the first sign that someone
//! means to search -- a pointer over the trigger, or focus on it -- so a page
//! that is never searched pays nothing and a page that is searched has
//! usually finished loading it before the dialog is open. Against pagefind's
//! ~35 KB per search, flat: this is cheaper outright until the corpus is
//! roughly fifteen times larger.
//!
//! # Wiring a site to it
//!
//! Three lines, none of them in this crate:
//!
//! ```text
//! // 1. publish the index, built from the same docs the routes render
//! .static_file("/search-index.json", Index::build(&pages).to_json())
//! // 2. publish the client
//! .static_file("/assets/js/search.js", read("assets/js/search.js"))
//! // 3. load it, and mark the trigger `hidden` so it is absent without
//! //    JavaScript rather than present and dead -- the script reveals it
//! <script defer src="/assets/js/search.js"></script>
//! ```
//!
//! # What is indexed
//!
//! A [`Page`] is a rendered page: url, title, description, HTML body, and the
//! heading outline [`crate::markdown`] already produced. Only the body is
//! indexed. Navigation chrome -- `<nav>`, `<header>`, `<footer>`, `<aside>`,
//! and script and style -- is dropped wholesale, because a sidebar listing
//! every page would otherwise make every page match every query.
//!
//! # Why results are sections and not pages
//!
//! A hit on a 4 000-word page that opens the page at its top has not answered
//! the question. The body is cut at every heading in the outline, and each
//! piece carries that heading's `id` as its anchor -- the ids come from the
//! outline rather than from re-parsing the markup, so an anchor is guaranteed
//! to exist in the page it points at.
//!
//! # Ranking
//!
//! One formula, no per-branch constants:
//!
//! ```text
//! score(section, query) = SUM over t in query of
//!                             idf(t) * w(t, section) * (1 + ln(tf(t, section)))
//!
//!     idf(t) = ln(1 + N / df(t))          N = number of sections
//!     w(t, s) = 2^rank(field)             highest-ranked field t occurs in
//!     rank: body 0, description 1, heading 2, title 3
//! ```
//!
//! So a title match is worth eight body matches and a heading match four, and
//! the ordering asked for falls out of the one exponent rather than out of
//! three hand-picked numbers. A term matched only as a prefix of a longer word
//! -- which is what type-ahead does to the last word typed -- contributes
//! [`PREFIX_FACTOR`] of its weight.
//!
//! Every query term must appear, or the section does not score at all.
//!
//! # No stemmer
//!
//! Deliberate. A stemmer in the client has to agree with the builder on every
//! word or a query silently misses, and "silently" is the expensive part: no
//! error, just no results. Prefix matching on the last word gives type-ahead,
//! which is what a reader actually feels, at none of that risk.

use std::collections::BTreeMap;
use std::collections::HashMap;
use std::fmt::Write as _;

use crate::markdown::Heading;

// ------------------------------------------------------------------- fields

/// Field bits. The weight of a field is `2^rank`, and the rank *is* the bit
/// index, so the table and the formula cannot drift apart.
pub const FIELD_BODY: u8 = 1 << 0;
/// See [`FIELD_BODY`].
pub const FIELD_DESCRIPTION: u8 = 1 << 1;
/// See [`FIELD_BODY`].
pub const FIELD_HEADING: u8 = 1 << 2;
/// See [`FIELD_BODY`].
pub const FIELD_TITLE: u8 = 1 << 3;

/// What a prefix-only match is worth, relative to a whole-word match. Applies
/// to the last word of a query, which is the one still being typed.
pub const PREFIX_FACTOR: f64 = 0.5;

/// Characters of section text shown around the first matching term.
const SNIPPET_CHARS: usize = 220;

/// The weight of the highest-ranked field a term was found in: `2^rank`.
fn field_weight(fields: u8) -> f64 {
    if fields == 0 {
        return 0.0;
    }
    let rank = 7 - fields.leading_zeros();
    f64::from(1u32 << rank)
}

// -------------------------------------------------------------------- input

/// One rendered page, as the index wants it.
#[derive(Debug, Clone, Copy)]
pub struct Page<'a> {
    /// The page's own URL, deployment sub-path included. Hits append `#id`.
    pub url: &'a str,
    pub title: &'a str,
    pub description: Option<&'a str>,
    /// The rendered body. Chrome is stripped here, but passing the whole
    /// document works too.
    pub html: &'a str,
    /// The outline from [`crate::markdown::Rendered::headings`]. Its ids are
    /// the ones in `html`, which is why anchors are never re-derived.
    pub headings: &'a [Heading],
}

// -------------------------------------------------------------------- index

/// A page in the index. Sections point at it by position.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Document {
    pub url: String,
    pub title: String,
    pub description: String,
}

/// One heading's worth of a page: what a result actually opens.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Section {
    /// Position in [`Index::docs`].
    pub doc: u32,
    /// The heading's `id`, or empty for the text before the first heading.
    pub anchor: String,
    /// The heading, or the page title for the leading section.
    pub heading: String,
    /// The section's prose, markup stripped. The snippet is cut from this.
    pub text: String,
}

/// One term's occurrence in one section.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Posting {
    /// Position in [`Index::sections`].
    pub section: u32,
    /// Bitwise or of the `FIELD_*` constants.
    pub fields: u8,
    /// Term frequency within the section.
    pub count: u32,
}

/// The whole index. Serialises to JSON and back without loss.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Index {
    pub docs: Vec<Document>,
    pub sections: Vec<Section>,
    /// Sorted, so the JSON is byte-stable and the client can binary-search a
    /// prefix range without building a second structure.
    pub terms: BTreeMap<String, Vec<Posting>>,
}

/// A section that matched, ready to render.
#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    /// The document URL with the section's fragment appended.
    pub url: String,
    pub title: String,
    /// The section's heading, or the page title for the leading section.
    pub heading: String,
    /// The heading id, or `None` for the leading section.
    pub anchor: Option<String>,
    pub snippet: String,
    pub score: f64,
}

// ---------------------------------------------------------------- tokenising

/// Lowercased runs of alphanumerics. No stemming -- see the module docs. The
/// client reimplements exactly this, and `search.js` says so beside it.
pub fn tokenize(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty() && t.chars().count() <= 48)
        .map(str::to_lowercase)
        .collect()
}

// --------------------------------------------------------------------- html

/// Elements whose entire contents are dropped. The first five are navigation
/// chrome: indexing them makes every page match every query, because every
/// page carries the same sidebar. The rest never hold prose.
const DROPPED: &[&str] = &[
    "nav", "header", "footer", "aside", "form", "script", "style", "svg", "template", "noscript",
    "button", "select",
];

/// Elements that do not break a word. Everything else is a word boundary, so
/// `<p>one</p><p>two</p>` is two words and `<code>one</code>two` is one.
const INLINE: &[&str] = &[
    "a", "abbr", "b", "bdi", "bdo", "cite", "code", "data", "del", "dfn", "em", "i", "ins", "kbd",
    "mark", "q", "rp", "rt", "ruby", "s", "samp", "small", "span", "strong", "sub", "sup", "time",
    "u", "var", "wbr",
];

/// The tag name at `html[at..]`, where `at` is just past a `<`, lowercased,
/// plus whether it was a closing tag.
fn tag_name(html: &str, at: usize) -> (String, bool) {
    let rest = &html[at..];
    let closing = rest.starts_with('/');
    let rest = if closing { &rest[1..] } else { rest };
    let end = rest
        .find(|c: char| !c.is_ascii_alphanumeric())
        .unwrap_or(rest.len());
    (rest[..end].to_ascii_lowercase(), closing)
}

/// The byte just past the matching close tag for `name`, opened at `from`.
/// Counts nesting, so a `<nav>` inside a `<nav>` does not end the outer one.
fn skip_element(html: &str, from: usize, name: &str) -> usize {
    let mut depth = 0usize;
    let mut at = from;
    while let Some(rel) = html[at..].find('<') {
        let lt = at + rel;
        let (tag, closing) = tag_name(html, lt + 1);
        let gt = match html[lt..].find('>') {
            Some(i) => lt + i + 1,
            None => return html.len(),
        };
        if tag == name {
            if closing {
                if depth == 0 {
                    return gt;
                }
                depth -= 1;
            } else if !html[lt..gt].ends_with("/>") {
                depth += 1;
            }
        }
        at = gt;
    }
    html.len()
}

/// Expands the entity at `html[at..]` (just past `&`), returning the
/// replacement and the byte just past the `;`.
fn entity(html: &str, at: usize) -> Option<(char, usize)> {
    let rest = &html[at..];
    let end = rest.find(';')?;
    if end > 10 {
        return None;
    }
    let body = &rest[..end];
    let ch = match body {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" | "#39" => '\'',
        "nbsp" | "#160" => ' ',
        "hellip" => '…',
        "mdash" => '—',
        "ndash" => '–',
        _ => {
            let digits = body.strip_prefix('#')?;
            let code = match digits.strip_prefix(['x', 'X']) {
                Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                None => digits.parse().ok()?,
            };
            char::from_u32(code)?
        }
    };
    Some((ch, at + end + 1))
}

/// HTML to indexable text: chrome removed, tags turned into word boundaries,
/// entities decoded, whitespace collapsed.
pub fn to_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len() / 2);
    let mut at = 0usize;
    while at < html.len() {
        let Some(rel) = html[at..].find(['<', '&']) else {
            out.push_str(&html[at..]);
            break;
        };
        out.push_str(&html[at..at + rel]);
        let mark = at + rel;
        if html[mark..].starts_with('&') {
            match entity(html, mark + 1) {
                Some((ch, next)) => {
                    out.push(ch);
                    at = next;
                }
                None => {
                    out.push('&');
                    at = mark + 1;
                }
            }
            continue;
        }
        if html[mark..].starts_with("<!--") {
            at = html[mark..]
                .find("-->")
                .map_or(html.len(), |i| mark + i + 3);
            continue;
        }
        let (tag, closing) = tag_name(html, mark + 1);
        let gt = match html[mark..].find('>') {
            Some(i) => mark + i + 1,
            None => break,
        };
        if !closing && DROPPED.contains(&tag.as_str()) && !html[mark..gt].ends_with("/>") {
            out.push(' ');
            at = skip_element(html, gt, &tag);
            continue;
        }
        if !INLINE.contains(&tag.as_str()) {
            out.push(' ');
        }
        at = gt;
    }
    let mut text = String::with_capacity(out.len());
    for word in out.split_whitespace() {
        if !text.is_empty() {
            text.push(' ');
        }
        text.push_str(word);
    }
    text
}

// ----------------------------------------------------------------- sectioning

/// Cuts `html` at every heading in `outline`, in order.
///
/// A heading is located by its own `id`, which is why the outline is an
/// argument rather than something re-derived here: these ids are the ones the
/// renderer wrote, so an anchor cannot point at a fragment that is not there.
/// A heading the markup lost -- a component swallowed it, say -- is skipped
/// rather than fatal, and its prose stays with the previous section.
fn cut_into_sections<'a>(
    html: &'a str,
    outline: &'a [Heading],
) -> Vec<(Option<&'a Heading>, &'a str)> {
    let mut out = Vec::with_capacity(outline.len() + 1);
    let mut body_start = 0usize;
    let mut current: Option<&Heading> = None;
    let mut cursor = 0usize;

    for heading in outline {
        let needle = format!("id=\"{}\"", heading.id);
        let Some(rel) = html[cursor..].find(&needle) else {
            continue;
        };
        let at = cursor + rel;
        let Some(tag_start) = html[..at].rfind('<') else {
            continue;
        };
        out.push((current, &html[body_start..tag_start]));

        let close = format!("</h{}>", heading.level);
        let after = html[at..]
            .find(&close)
            .map(|i| at + i + close.len())
            .or_else(|| html[at..].find('>').map(|i| at + i + 1))
            .unwrap_or(html.len());
        current = Some(heading);
        body_start = after;
        cursor = after;
    }
    out.push((current, &html[body_start..]));
    out
}

// -------------------------------------------------------------------- build

/// Records one occurrence. Sections arrive in increasing order, so the
/// posting for the current section is always the last one in the list.
fn post(terms: &mut BTreeMap<String, Vec<Posting>>, token: String, section: u32, field: u8) {
    let postings = terms.entry(token).or_default();
    match postings.last_mut() {
        Some(p) if p.section == section => {
            p.fields |= field;
            p.count += 1;
        }
        _ => postings.push(Posting {
            section,
            fields: field,
            count: 1,
        }),
    }
}

impl Index {
    /// Builds the index from rendered pages.
    pub fn build(pages: &[Page<'_>]) -> Self {
        let mut index = Index::default();
        for page in pages {
            let doc = u32::try_from(index.docs.len()).expect("a site has fewer than 4e9 pages");
            index.docs.push(Document {
                url: page.url.to_string(),
                title: page.title.to_string(),
                description: page.description.unwrap_or_default().to_string(),
            });

            for (nth, (heading, chunk)) in cut_into_sections(page.html, page.headings)
                .into_iter()
                .enumerate()
            {
                let text = to_text(chunk);
                // A heading with nothing under it is still worth keeping: the
                // heading itself is what the reader was looking for.
                if nth > 0 && text.is_empty() && heading.is_none() {
                    continue;
                }
                let section = u32::try_from(index.sections.len())
                    .expect("a site has fewer than 4e9 sections");
                index.sections.push(Section {
                    doc,
                    anchor: heading.map(|h| h.id.clone()).unwrap_or_default(),
                    heading: heading.map_or_else(|| page.title.to_string(), |h| h.text.clone()),
                    text: text.clone(),
                });

                for token in tokenize(&text) {
                    post(&mut index.terms, token, section, FIELD_BODY);
                }
                match heading {
                    Some(h) => {
                        for token in tokenize(&h.text) {
                            post(&mut index.terms, token, section, FIELD_HEADING);
                        }
                    }
                    None => {
                        // The leading section stands for the page, so the page's
                        // own title and description are indexed onto it. Putting
                        // them on every section would return the whole page for
                        // a title query.
                        for token in tokenize(page.title) {
                            post(&mut index.terms, token, section, FIELD_TITLE);
                        }
                        for token in tokenize(page.description.unwrap_or_default()) {
                            post(&mut index.terms, token, section, FIELD_DESCRIPTION);
                        }
                    }
                }
            }
        }
        index
    }

    /// Bytes of the serialised index, and what it holds. For the report that
    /// has to accompany any "search is cheap" claim.
    pub fn stats(&self) -> Stats {
        Stats {
            docs: self.docs.len(),
            sections: self.sections.len(),
            terms: self.terms.len(),
            postings: self.terms.values().map(Vec::len).sum(),
            json_bytes: self.to_json().len(),
        }
    }
}

/// What [`Index::stats`] measures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stats {
    pub docs: usize,
    pub sections: usize,
    pub terms: usize,
    pub postings: usize,
    pub json_bytes: usize,
}

// -------------------------------------------------------------------- query

/// What one query term matched in one section.
#[derive(Debug, Clone, Copy)]
struct Match {
    fields: u8,
    count: u32,
    /// 1.0 for a whole word, [`PREFIX_FACTOR`] for a prefix-only match.
    factor: f64,
}

impl Index {
    /// Postings for one term. The last term of a query also matches as a
    /// prefix, because it is the one still being typed.
    fn postings_for(&self, term: &str, prefix: bool) -> HashMap<u32, Match> {
        let mut found: HashMap<u32, Match> = HashMap::new();
        let mut take = |postings: &Vec<Posting>, factor: f64| {
            for p in postings {
                let slot = found.entry(p.section).or_insert(Match {
                    fields: 0,
                    count: 0,
                    factor: 0.0,
                });
                slot.fields |= p.fields;
                slot.count += p.count;
                slot.factor = slot.factor.max(factor);
            }
        };
        if let Some(exact) = self.terms.get(term) {
            take(exact, 1.0);
        }
        if prefix {
            for (word, postings) in self.terms.range(term.to_string()..) {
                if !word.starts_with(term) {
                    break;
                }
                if word != term {
                    take(postings, PREFIX_FACTOR);
                }
            }
        }
        found
    }

    /// The best `limit` sections for `query`, highest score first.
    ///
    /// Every term must appear somewhere in the section, so a two-word query
    /// narrows rather than widens, and a query matching nothing returns
    /// nothing rather than everything.
    pub fn search(&self, query: &str, limit: usize) -> Vec<Hit> {
        let terms = tokenize(query);
        if terms.is_empty() || self.sections.is_empty() {
            return Vec::new();
        }
        let total = self.sections.len() as f64;

        let mut scores: HashMap<u32, f64> = HashMap::new();
        for (nth, term) in terms.iter().enumerate() {
            let last = nth + 1 == terms.len();
            let found = self.postings_for(term, last);
            if found.is_empty() {
                return Vec::new();
            }
            // idf(t) = ln(1 + N / df(t)).
            let idf = (1.0 + total / found.len() as f64).ln();
            if nth == 0 {
                for (section, m) in &found {
                    scores.insert(*section, contribution(idf, *m));
                }
            } else {
                scores.retain(|section, score| match found.get(section) {
                    Some(m) => {
                        *score += contribution(idf, *m);
                        true
                    }
                    None => false,
                });
            }
            if scores.is_empty() {
                return Vec::new();
            }
        }

        let mut ranked: Vec<(u32, f64)> = scores.into_iter().collect();
        // Score first; section order second, so equal scores stay in reading
        // order and the list does not reshuffle between builds.
        ranked.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
        ranked.truncate(limit);

        ranked
            .into_iter()
            .map(|(id, score)| {
                let section = &self.sections[id as usize];
                let doc = &self.docs[section.doc as usize];
                let anchor = (!section.anchor.is_empty()).then(|| section.anchor.clone());
                Hit {
                    url: match &anchor {
                        Some(a) => format!("{}#{a}", doc.url),
                        None => doc.url.clone(),
                    },
                    title: doc.title.clone(),
                    heading: section.heading.clone(),
                    anchor,
                    snippet: snippet(
                        if section.text.is_empty() {
                            &doc.description
                        } else {
                            &section.text
                        },
                        &terms,
                    ),
                    score,
                }
            })
            .collect()
    }
}

/// One term's share of a section's score: `idf * w * (1 + ln(tf))`.
fn contribution(idf: f64, m: Match) -> f64 {
    idf * field_weight(m.fields) * (1.0 + f64::from(m.count).ln()) * m.factor
}

/// A window of `text` around the first term that occurs in it.
fn snippet(text: &str, terms: &[String]) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= SNIPPET_CHARS {
        return text.to_string();
    }
    let lower: Vec<char> = text.to_lowercase().chars().collect();
    let at = terms
        .iter()
        .filter_map(|t| find_chars(&lower, t))
        .min()
        .unwrap_or(0);

    // Centre the window on the match, then pull it back inside the text.
    let half = SNIPPET_CHARS / 2;
    let mut start = at.saturating_sub(half);
    // Start on a word boundary, so a snippet never opens mid-word.
    while start > 0 && !chars[start - 1].is_whitespace() {
        start -= 1;
    }
    let end = (start + SNIPPET_CHARS).min(chars.len());
    let mut out = String::new();
    if start > 0 {
        out.push('…');
    }
    out.extend(&chars[start..end]);
    if end < chars.len() {
        out.push('…');
    }
    out
}

/// Character offset of `needle` in an already-lowercased character slice.
fn find_chars(haystack: &[char], needle: &str) -> Option<usize> {
    let pattern: Vec<char> = needle.chars().collect();
    if pattern.is_empty() || pattern.len() > haystack.len() {
        return None;
    }
    (0..=haystack.len() - pattern.len()).find(|&i| haystack[i..i + pattern.len()] == pattern[..])
}

// --------------------------------------------------------------------- json
//
// Written by hand rather than with `serde_json`, which is not in the tree and
// would be a dependency added for one file. The shape is positional -- every
// record is an array -- because repeating `{"section":…,"fields":…}` across
// tens of thousands of postings is most of what an index of this kind weighs.
//
//   {"v":1,
//    "docs":     [[url, title, description], …],
//    "sections": [[doc, anchor, heading, text], …],
//    "terms":    {word: [[section, fields, count], …], …}}

/// Bumped whenever the shape above changes, so a cached or deployed index from
/// an older build is rejected instead of half-read. `search.js` checks it too.
pub const FORMAT_VERSION: u32 = 1;

fn write_json_string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

impl Index {
    /// The index as JSON, exactly as `search.js` expects to fetch it.
    pub fn to_json(&self) -> String {
        let mut out = String::with_capacity(64 * self.sections.len() + 16 * self.terms.len());
        let _ = write!(out, "{{\"v\":{FORMAT_VERSION},\"docs\":[");
        for (nth, doc) in self.docs.iter().enumerate() {
            if nth > 0 {
                out.push(',');
            }
            out.push('[');
            write_json_string(&mut out, &doc.url);
            out.push(',');
            write_json_string(&mut out, &doc.title);
            out.push(',');
            write_json_string(&mut out, &doc.description);
            out.push(']');
        }
        out.push_str("],\"sections\":[");
        for (nth, s) in self.sections.iter().enumerate() {
            if nth > 0 {
                out.push(',');
            }
            let _ = write!(out, "[{},", s.doc);
            write_json_string(&mut out, &s.anchor);
            out.push(',');
            write_json_string(&mut out, &s.heading);
            out.push(',');
            write_json_string(&mut out, &s.text);
            out.push(']');
        }
        out.push_str("],\"terms\":{");
        for (nth, (term, postings)) in self.terms.iter().enumerate() {
            if nth > 0 {
                out.push(',');
            }
            write_json_string(&mut out, term);
            out.push_str(":[");
            for (i, p) in postings.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                let _ = write!(out, "[{},{},{}]", p.section, p.fields, p.count);
            }
            out.push(']');
        }
        out.push_str("}}");
        out
    }

    /// Reads back what [`Index::to_json`] wrote.
    pub fn from_json(src: &str) -> Result<Self, JsonError> {
        let mut r = Reader {
            src: src.as_bytes(),
            at: 0,
        };
        let mut index = Index::default();
        let mut version = None;
        r.ws();
        r.byte(b'{')?;
        if !r.peek(b'}') {
            loop {
                r.ws();
                let key = r.string()?;
                r.ws();
                r.byte(b':')?;
                match key.as_str() {
                    "v" => version = Some(r.number()?),
                    "docs" => {
                        index.docs = r.array(|r| {
                            r.byte(b'[')?;
                            let url = r.string()?;
                            r.ws();
                            r.byte(b',')?;
                            let title = r.string()?;
                            r.ws();
                            r.byte(b',')?;
                            let description = r.string()?;
                            r.ws();
                            r.byte(b']')?;
                            Ok(Document {
                                url,
                                title,
                                description,
                            })
                        })?
                    }
                    "sections" => {
                        index.sections = r.array(|r| {
                            r.byte(b'[')?;
                            let doc = r.number()?;
                            r.ws();
                            r.byte(b',')?;
                            let anchor = r.string()?;
                            r.ws();
                            r.byte(b',')?;
                            let heading = r.string()?;
                            r.ws();
                            r.byte(b',')?;
                            let text = r.string()?;
                            r.ws();
                            r.byte(b']')?;
                            Ok(Section {
                                doc,
                                anchor,
                                heading,
                                text,
                            })
                        })?
                    }
                    "terms" => {
                        r.ws();
                        r.byte(b'{')?;
                        if !r.peek(b'}') {
                            loop {
                                r.ws();
                                let term = r.string()?;
                                r.ws();
                                r.byte(b':')?;
                                let postings = r.array(|r| {
                                    r.byte(b'[')?;
                                    let section = r.number()?;
                                    r.ws();
                                    r.byte(b',')?;
                                    let fields = r.number()?;
                                    r.ws();
                                    r.byte(b',')?;
                                    let count = r.number()?;
                                    r.ws();
                                    r.byte(b']')?;
                                    Ok(Posting {
                                        section,
                                        fields: u8::try_from(fields)
                                            .map_err(|_| JsonError::new(r.at, "field bits"))?,
                                        count,
                                    })
                                })?;
                                index.terms.insert(term, postings);
                                r.ws();
                                if !r.take(b',') {
                                    break;
                                }
                            }
                        }
                        r.ws();
                        r.byte(b'}')?;
                    }
                    _ => return Err(JsonError::new(r.at, "unknown key")),
                }
                r.ws();
                if !r.take(b',') {
                    break;
                }
            }
        }
        r.ws();
        r.byte(b'}')?;
        match version {
            Some(FORMAT_VERSION) => Ok(index),
            _ => Err(JsonError::new(0, "wrong index format version")),
        }
    }
}

/// Where the index JSON stopped making sense.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("search index: {what} at byte {at}")]
pub struct JsonError {
    pub at: usize,
    pub what: &'static str,
}

impl JsonError {
    fn new(at: usize, what: &'static str) -> Self {
        JsonError { at, what }
    }
}

struct Reader<'a> {
    src: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn ws(&mut self) {
        while matches!(self.src.get(self.at), Some(b' ' | b'\n' | b'\r' | b'\t')) {
            self.at += 1;
        }
    }

    fn peek(&mut self, b: u8) -> bool {
        self.ws();
        self.src.get(self.at) == Some(&b)
    }

    fn take(&mut self, b: u8) -> bool {
        if self.src.get(self.at) == Some(&b) {
            self.at += 1;
            return true;
        }
        false
    }

    fn byte(&mut self, b: u8) -> Result<(), JsonError> {
        self.ws();
        if self.take(b) {
            return Ok(());
        }
        Err(JsonError::new(
            self.at,
            match b {
                b'{' => "expected {",
                b'}' => "expected }",
                b'[' => "expected [",
                b']' => "expected ]",
                b':' => "expected :",
                _ => "expected ,",
            },
        ))
    }

    fn number(&mut self) -> Result<u32, JsonError> {
        self.ws();
        let start = self.at;
        while matches!(self.src.get(self.at), Some(c) if c.is_ascii_digit()) {
            self.at += 1;
        }
        if start == self.at {
            return Err(JsonError::new(self.at, "expected a number"));
        }
        std::str::from_utf8(&self.src[start..self.at])
            .ok()
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| JsonError::new(start, "number out of range"))
    }

    fn string(&mut self) -> Result<String, JsonError> {
        self.ws();
        self.byte(b'"')?;
        let mut out = String::new();
        loop {
            let start = self.at;
            while !matches!(self.src.get(self.at), None | Some(b'"' | b'\\')) {
                self.at += 1;
            }
            out.push_str(
                std::str::from_utf8(&self.src[start..self.at])
                    .map_err(|_| JsonError::new(start, "invalid utf-8"))?,
            );
            match self.src.get(self.at) {
                None => return Err(JsonError::new(self.at, "unterminated string")),
                Some(b'"') => {
                    self.at += 1;
                    return Ok(out);
                }
                _ => {}
            }
            self.at += 1;
            let escape = *self
                .src
                .get(self.at)
                .ok_or_else(|| JsonError::new(self.at, "unterminated escape"))?;
            self.at += 1;
            match escape {
                b'"' => out.push('"'),
                b'\\' => out.push('\\'),
                b'/' => out.push('/'),
                b'b' => out.push('\u{8}'),
                b'f' => out.push('\u{c}'),
                b'n' => out.push('\n'),
                b'r' => out.push('\r'),
                b't' => out.push('\t'),
                b'u' => out.push(self.unicode_escape()?),
                _ => return Err(JsonError::new(self.at, "unknown escape")),
            }
        }
    }

    fn hex4(&mut self) -> Result<u32, JsonError> {
        let end = self.at + 4;
        let digits = self
            .src
            .get(self.at..end)
            .ok_or_else(|| JsonError::new(self.at, "short \\u escape"))?;
        let value = std::str::from_utf8(digits)
            .ok()
            .and_then(|s| u32::from_str_radix(s, 16).ok())
            .ok_or_else(|| JsonError::new(self.at, "bad \\u escape"))?;
        self.at = end;
        Ok(value)
    }

    fn unicode_escape(&mut self) -> Result<char, JsonError> {
        let first = self.hex4()?;
        // A code point above the BMP arrives as a surrogate pair.
        if (0xd800..0xdc00).contains(&first) {
            let at = self.at;
            if self.take(b'\\') && self.take(b'u') {
                let low = self.hex4()?;
                if (0xdc00..0xe000).contains(&low) {
                    let combined = 0x10000 + ((first - 0xd800) << 10) + (low - 0xdc00);
                    return char::from_u32(combined)
                        .ok_or_else(|| JsonError::new(at, "bad surrogate pair"));
                }
            }
            return Err(JsonError::new(at, "lone surrogate"));
        }
        char::from_u32(first).ok_or_else(|| JsonError::new(self.at, "bad code point"))
    }

    fn array<T>(
        &mut self,
        mut item: impl FnMut(&mut Reader<'a>) -> Result<T, JsonError>,
    ) -> Result<Vec<T>, JsonError> {
        self.byte(b'[')?;
        let mut out = Vec::new();
        if self.peek(b']') {
            self.at += 1;
            return Ok(out);
        }
        loop {
            self.ws();
            out.push(item(self)?);
            self.ws();
            if !self.take(b',') {
                break;
            }
        }
        self.byte(b']')?;
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::markdown::{self, Rendered};
    use indoc::indoc;

    // Fixtures go through the real renderer, so the outline ids under test are
    // the ids the markup actually carries. A fake outline would make the
    // anchor test prove nothing.
    const NAVIGATION: &str = indoc! {r#"
        Navigation is one tree declared in `depoth.yaml`, outside the content.

        <div class="note">
        Mintlify's model is worth copying exactly.
        </div>

        ## The one-child-kind rule

        A node carries exactly one kind of child. A tab holds groups or pages,
        never both.

        ## Anchors and tabs

        An anchor is a link, never a container. The tree names slugs, never
        keys.
    "#};

    const CACHING: &str = indoc! {"
        A fingerprint in the filename lets a host cache an asset forever.

        ## Cache keys

        The render cache must be versioned, or navigation serves stale markup.
    "};

    struct Fixture {
        navigation: Rendered,
        caching: Rendered,
    }

    impl Fixture {
        fn new() -> Self {
            Fixture {
                navigation: markdown::render(NAVIGATION),
                caching: markdown::render(CACHING),
            }
        }

        fn pages(&self) -> Vec<Page<'_>> {
            vec![
                Page {
                    url: "/docs/navigation",
                    title: "Navigation",
                    description: Some("One tree, declared outside the content."),
                    html: &self.navigation.html,
                    headings: &self.navigation.headings,
                },
                Page {
                    url: "/docs/caching",
                    title: "Assets and the cache",
                    description: Some("Fingerprinted output, and the render cache."),
                    html: &self.caching.html,
                    headings: &self.caching.headings,
                },
            ]
        }

        fn index(&self) -> Index {
            Index::build(&self.pages())
        }
    }

    #[test]
    fn a_word_in_a_document_is_found() {
        let f = Fixture::new();
        let hits = f.index().search("fingerprint", 10);
        assert_eq!(hits.len(), 1, "one section carries it");
        assert_eq!(hits[0].url, "/docs/caching");
        assert!(
            hits[0].snippet.to_lowercase().contains("fingerprint"),
            "the snippet should show the match: {:?}",
            hits[0].snippet
        );
    }

    #[test]
    fn a_word_in_no_document_is_not_found() {
        let f = Fixture::new();
        assert_eq!(f.index().search("zebra", 10), Vec::new());
    }

    #[test]
    fn a_title_match_outranks_a_body_match() {
        let f = Fixture::new();
        // "navigation" is the title of one page and a body word on the other.
        let hits = f.index().search("navigation", 10);
        assert!(hits.len() >= 2, "both pages use the word: {hits:?}");
        assert_eq!(hits[0].url, "/docs/navigation", "the title match leads");
        assert!(
            hits.iter().any(|h| h.url == "/docs/caching#cache-keys"),
            "the body match is still returned, just lower: {hits:?}"
        );
        assert!(
            hits[0].score > hits[1].score,
            "a title match is 2^3 and a body match 2^0, so the gap is real"
        );
    }

    #[test]
    fn a_heading_match_outranks_a_body_match() {
        let f = Fixture::new();
        let index = f.index();
        // "keys" is a heading word on one page and a body word on the other,
        // and is in no title, so only the field weight separates them.
        let hits = index.search("keys", 10);
        assert_eq!(hits.len(), 2, "both pages use it: {hits:?}");
        assert_eq!(hits[0].url, "/docs/caching#cache-keys");
        assert_eq!(hits[1].url, "/docs/navigation#anchors-and-tabs");
        assert!(
            hits[0].score > hits[1].score,
            "2^2 against 2^0: {} vs {}",
            hits[0].score,
            hits[1].score
        );
    }

    #[test]
    fn a_hit_anchors_on_a_heading_from_that_pages_outline() {
        let f = Fixture::new();
        let hits = f.index().search("carries", 10);
        let hit = hits.first().expect("the word is in the fixture");
        let anchor = hit.anchor.as_deref().expect("it sits under a heading");
        assert_eq!(anchor, "the-one-child-kind-rule");
        assert_eq!(hit.url, "/docs/navigation#the-one-child-kind-rule");
        assert!(
            f.navigation.headings.iter().any(|h| h.id == anchor),
            "the anchor must be an id the renderer wrote: {:?}",
            f.navigation.headings
        );
    }

    #[test]
    fn the_leading_section_of_a_page_has_no_anchor() {
        let f = Fixture::new();
        let hits = f.index().search("fingerprint", 10);
        assert_eq!(hits[0].anchor, None);
        assert!(
            !hits[0].url.contains('#'),
            "it opens the page, not a stale id"
        );
    }

    #[test]
    fn markup_is_stripped_before_indexing() {
        let f = Fixture::new();
        let index = f.index();
        assert!(
            index.sections.iter().all(|s| !s.text.contains('<')),
            "a section's text is text"
        );
        // The fixture really does contain a <div>.
        assert!(f.navigation.html.contains("<div"));
        assert_eq!(index.search("div", 10), Vec::new(), "tags are not words");
        assert_eq!(index.search("class", 10), Vec::new(), "nor are attributes");
        // The prose inside the div survives, which is the point of stripping
        // rather than skipping.
        assert_eq!(index.search("mintlify", 10).len(), 1);
    }

    #[test]
    fn navigation_chrome_is_not_indexed() {
        // Every page carries the same sidebar; indexing it makes every page
        // match every query.
        let page = Page {
            url: "/docs/shell",
            title: "Shell",
            description: None,
            html: "<nav><a href=\"/x\">Kangaroo</a></nav><p>Prose about wombats.</p>\
                   <footer>Platypus</footer>",
            headings: &[],
        };
        let index = Index::build(&[page]);
        assert_eq!(index.search("kangaroo", 10), Vec::new());
        assert_eq!(index.search("platypus", 10), Vec::new());
        assert_eq!(index.search("wombats", 10).len(), 1);
    }

    #[test]
    fn a_multi_word_query_requires_every_term() {
        let f = Fixture::new();
        let index = f.index();
        assert_eq!(index.search("mintlify", 10).len(), 1);
        assert_eq!(index.search("fingerprint", 10).len(), 1);
        // Each word is in the corpus, but never in one section together.
        assert_eq!(
            index.search("mintlify fingerprint", 10),
            Vec::new(),
            "terms are intersected, not unioned"
        );
        assert!(
            !index.search("mintlify model", 10).is_empty(),
            "both words in one section still match"
        );
    }

    #[test]
    fn a_query_matching_nothing_returns_nothing() {
        let f = Fixture::new();
        let index = f.index();
        assert_eq!(index.search("", 10), Vec::new());
        assert_eq!(index.search("   ", 10), Vec::new());
        assert_eq!(index.search("!!!", 10), Vec::new());
        assert_eq!(index.search("zebra quagga", 10), Vec::new());
        // One known word beside one unknown is still nothing: an unmatched
        // term must not fall back to returning the corpus.
        assert_eq!(index.search("navigation quagga", 10), Vec::new());
    }

    #[test]
    fn the_last_word_matches_as_a_prefix_so_typing_shows_results() {
        let f = Fixture::new();
        let index = f.index();
        assert!(!index.search("fingerpr", 10).is_empty(), "type-ahead");
        // Only the last word: an earlier word is taken as written.
        assert_eq!(index.search("fingerpr filename", 10), Vec::new());
    }

    #[test]
    fn the_index_round_trips_through_json() {
        let f = Fixture::new();
        let index = f.index();
        let json = index.to_json();
        let back = Index::from_json(&json).expect("parses");
        assert_eq!(back, index);
        assert_eq!(back.to_json(), json, "and serialises identically");
        assert_eq!(
            back.search("navigation", 10),
            index.search("navigation", 10)
        );
    }

    #[test]
    fn json_survives_quotes_newlines_and_astral_characters() {
        let page = Page {
            url: "/p",
            title: "A \"quoted\" title",
            description: Some("tab\there, backslash \\ there"),
            html: "<p>Emoji \u{1f680} and a line\nbreak and &amp; an entity.</p>",
            headings: &[],
        };
        let index = Index::build(&[page]);
        let back = Index::from_json(&index.to_json()).expect("parses");
        assert_eq!(back, index);
        assert!(back.sections[0].text.contains('\u{1f680}'));
        assert!(back.sections[0].text.contains('&'), "entities are decoded");
    }

    #[test]
    fn a_stale_format_version_is_rejected_rather_than_half_read() {
        let json = r#"{"v":99,"docs":[],"sections":[],"terms":{}}"#;
        assert!(Index::from_json(json).is_err());
        assert!(Index::from_json("not json").is_err());
    }

    #[test]
    fn field_weight_is_two_to_the_rank() {
        assert_eq!(field_weight(FIELD_BODY), 1.0);
        assert_eq!(field_weight(FIELD_DESCRIPTION), 2.0);
        assert_eq!(field_weight(FIELD_HEADING), 4.0);
        assert_eq!(field_weight(FIELD_TITLE), 8.0);
        // A term in several fields is worth the best of them, not their sum.
        assert_eq!(field_weight(FIELD_TITLE | FIELD_BODY), 8.0);
        assert_eq!(field_weight(0), 0.0);
    }

    #[test]
    fn stats_count_what_the_report_quotes() {
        let f = Fixture::new();
        let index = f.index();
        let stats = index.stats();
        assert_eq!(stats.docs, 2);
        assert_eq!(stats.sections, index.sections.len());
        assert!(stats.sections > stats.docs, "pages are cut at headings");
        assert_eq!(stats.json_bytes, index.to_json().len());
    }

    /// Measures the real documentation site. Ignored because the library's
    /// tests must not depend on the site's content; run it on purpose:
    ///
    /// ```text
    /// cargo test -p depoth search -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore = "reads ../../site/content and is a measurement, not an assertion"]
    fn measure_the_documentation_site() {
        #[derive(serde::Deserialize)]
        struct Front {
            title: String,
            #[serde(default)]
            description: Option<String>,
        }

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../site/content/docs")
            .canonicalize()
            .expect("the site is in the workspace");

        let mut sources = Vec::new();
        let mut stack = vec![root.clone()];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).expect("readable") {
                let path = entry.expect("entry").path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|e| e == "md") {
                    sources.push((
                        path.clone(),
                        std::fs::read_to_string(&path).expect("readable"),
                    ));
                }
            }
        }
        sources.sort_by(|a, b| a.0.cmp(&b.0));

        let prepared: Vec<(String, Front, Rendered)> = sources
            .iter()
            .map(|(path, source)| {
                let split = crate::frontmatter::split(source);
                let front: Front = crate::frontmatter::parse(&split, path).expect("frontmatter");
                let slug = path
                    .strip_prefix(&root)
                    .expect("under root")
                    .with_extension("");
                let slug = slug.to_string_lossy().replace('\\', "/");
                let slug = slug.strip_suffix("/index").unwrap_or(&slug).to_string();
                (format!("/docs/{slug}"), front, markdown::render(split.body))
            })
            .collect();

        let pages: Vec<Page<'_>> = prepared
            .iter()
            .map(|(url, front, rendered)| Page {
                url,
                title: &front.title,
                description: front.description.as_deref(),
                html: &rendered.html,
                headings: &rendered.headings,
            })
            .collect();

        let index = Index::build(&pages);
        let json = index.to_json();
        let stats = index.stats();
        let markdown_bytes: usize = sources.iter().map(|(_, s)| s.len()).sum();

        let out = std::env::temp_dir().join("depoth-search-index.json");
        std::fs::write(&out, &json).expect("writable");

        println!("--- depoth documentation site -----------------------------");
        println!("pages                 {}", stats.docs);
        println!("sections              {}", stats.sections);
        println!("distinct terms        {}", stats.terms);
        println!("postings              {}", stats.postings);
        println!("markdown source       {markdown_bytes} bytes");
        println!("index json            {} bytes", stats.json_bytes);
        println!(
            "index / source        {:.2}x",
            stats.json_bytes as f64 / markdown_bytes as f64
        );
        println!("written to            {}", out.display());
        for query in ["navigation", "cache", "deploy", "prerender partial"] {
            let hits = index.search(query, 8);
            println!("query {query:<20} {} hits", hits.len());
            for hit in hits.iter().take(3) {
                println!("    {:>7.3}  {}", hit.score, hit.url);
            }
        }
        assert!(stats.docs > 0);
    }
}
