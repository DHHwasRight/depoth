//! Heading anchors.
//!
//! GitHub's algorithm, reimplemented in thirty lines rather than taken from a
//! crate. The obvious candidate, `slug`, routes every non-ASCII character
//! through `deunicode`, which is actively wrong for documentation:
//!
//! | heading | here | `slug` |
//! |---|---|---|
//! | `日本語のドキュメント` | `日本語のドキュメント` | `ri-ben-yu-nodokiyumento` |
//! | `emoji 🚀 heading` | `emoji--heading` | `emoji-rocket-heading` |
//! | `snake_case_name` | `snake_case_name` | `snake-case-name` |
//!
//! It also has no collision handling at all, and a page with two `Overview`
//! headings is ordinary.

use std::collections::HashMap;

/// Assigns ids, remembering what it has already given out.
#[derive(Debug, Default)]
pub struct Anchors {
    seen: HashMap<String, usize>,
}

impl Anchors {
    pub fn new() -> Self {
        Self::default()
    }

    /// The id for `text`, unique within this page.
    ///
    /// A repeat gets `-1`, `-2` and so on, counted across the whole page --
    /// including headings inside tabs, which a reader cannot see at the same
    /// time but a link can still point at.
    pub fn id_for(&mut self, text: &str) -> String {
        let base = slugify(text);
        match self.seen.get_mut(&base) {
            None => {
                self.seen.insert(base.clone(), 0);
                base
            }
            Some(count) => {
                *count += 1;
                let n = *count;
                format!("{base}-{n}")
            }
        }
    }
}

/// Lowercase; keep letters, numbers, marks, `_` and `-`; spaces become `-`;
/// everything else is dropped. Non-ASCII letters are kept as themselves.
pub fn slugify(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        if ch.is_whitespace() {
            out.push('-');
        } else if ch.is_alphanumeric() || ch == '_' || ch == '-' {
            out.extend(ch.to_lowercase());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_matches_the_cases_that_distinguish_it_from_deunicode() {
        assert_eq!(slugify("日本語のドキュメント"), "日本語のドキュメント");
        assert_eq!(slugify("emoji 🚀 heading"), "emoji--heading");
        assert_eq!(slugify("snake_case_name"), "snake_case_name");
        assert_eq!(slugify("Hello, World!"), "hello-world");
        // Two spaces survive as two hyphens once the ampersand is dropped,
        // which is what GitHub does. It looks like a bug and is not.
        assert_eq!(slugify("Fees & billing"), "fees--billing");
    }

    #[test]
    fn repeats_are_numbered_rather_than_colliding() {
        let mut a = Anchors::new();
        assert_eq!(a.id_for("Overview"), "overview");
        assert_eq!(a.id_for("Overview"), "overview-1");
        assert_eq!(a.id_for("Overview"), "overview-2");
        assert_eq!(a.id_for("Other"), "other");
    }

    #[test]
    fn punctuation_only_headings_do_not_produce_an_empty_duplicate() {
        let mut a = Anchors::new();
        // Both slug to "", so the second must still be distinguishable.
        assert_eq!(a.id_for("!!!"), "");
        assert_eq!(a.id_for("???"), "-1");
    }
}
