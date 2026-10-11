//! Splitting a document into its YAML header and its body.
//!
//! Done here rather than by a crate for one reason: a frontmatter parser that
//! hands back only the YAML has already thrown away where the YAML started, so
//! every error it can produce is relative to a block the author cannot see.
//! Keeping the offset is the whole difference between
//!
//! ```text
//! line 2 column 9: invalid type: string "3", expected u64
//! ```
//!
//! and
//!
//! ```text
//! guide/install.md:3:9  weight: invalid type: string "3", expected u64
//! ```

use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;

const FENCE: &str = "---";

/// A document, split but not yet interpreted.
#[derive(Debug, PartialEq, Eq)]
pub struct Split<'a> {
    /// The YAML between the fences. Empty when there is no header.
    pub front: &'a str,
    /// Line of the file the YAML starts on, 1-based.
    pub front_line: usize,
    /// Everything after the closing fence.
    pub body: &'a str,
    /// Line of the file the body starts on, 1-based.
    pub body_line: usize,
}

/// Splits `source` on its leading `---` fences.
///
/// A document with no header is not an error: it is a document with no header,
/// and the body starts at line 1.
pub fn split(source: &str) -> Split<'_> {
    let text = source.strip_prefix('\u{feff}').unwrap_or(source);
    let Some(rest) = strip_fence_line(text) else {
        return Split {
            front: "",
            front_line: 1,
            body: text,
            body_line: 1,
        };
    };
    // The closing fence is a line that is exactly `---`.
    let mut offset = 0;
    for line in rest.split_inclusive('\n') {
        if line.trim_end_matches(['\r', '\n']) == FENCE {
            let front = &rest[..offset];
            let body = &rest[offset + line.len()..];
            return Split {
                front,
                front_line: 2,
                body,
                // The opening fence is line 1, the header runs to line
                // 1 + N, the closing fence is line 2 + N.
                body_line: front.lines().count() + 3,
            };
        }
        offset += line.len();
    }
    // An opening fence with no closing one. Treat the whole file as a body:
    // guessing that the author meant a header would hide the real mistake.
    Split {
        front: "",
        front_line: 1,
        body: text,
        body_line: 1,
    }
}

fn strip_fence_line(text: &str) -> Option<&str> {
    let rest = text.strip_prefix(FENCE)?;
    match rest
        .strip_prefix("\r\n")
        .or_else(|| rest.strip_prefix('\n'))
    {
        Some(rest) => Some(rest),
        // `---` as the entire file, or `---foo`: not a fence.
        None if rest.is_empty() => Some(rest),
        None => None,
    }
}

/// A frontmatter problem, already positioned in the file.
#[derive(Debug)]
pub struct FrontError {
    pub path: PathBuf,
    /// 1-based, counted from the start of the file.
    pub line: usize,
    pub column: usize,
    /// Dotted path to the offending field, where serde could name one.
    pub field: Option<String>,
    pub message: String,
}

impl std::fmt::Display for FrontError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}:{} ", self.path.display(), self.line, self.column)?;
        match &self.field {
            Some(field) => write!(f, "{field}: {}", self.message),
            None => write!(f, "{}", self.message),
        }
    }
}

impl std::error::Error for FrontError {}

/// Deserialises a split document's header, reporting positions in file
/// coordinates rather than header coordinates.
pub fn parse<T: DeserializeOwned>(split: &Split<'_>, path: &Path) -> Result<T, FrontError> {
    let deserializer = yaml_serde::Deserializer::from_str(split.front);
    // serde_path_to_error names the field, which YAML's own error does not
    // once the failure is nested inside a sequence or a map.
    match serde_path_to_error::deserialize(deserializer) {
        Ok(value) => Ok(value),
        Err(err) => {
            let field = match err.path().to_string() {
                p if p.is_empty() || p == "." => None,
                p => Some(p),
            };
            let inner = err.into_inner();
            let (line, column) = match inner.location() {
                // The header starts at `front_line`, and YAML counts from 1
                // inside it, so the two overlap by one line.
                Some(loc) => (split.front_line + loc.line() - 1, loc.column()),
                None => (split.front_line, 1),
            };
            Err(FrontError {
                path: path.to_path_buf(),
                line,
                column,
                field,
                message: strip_location(&inner.to_string()),
            })
        }
    }
}

/// Removes the trailing `at line N column M` that YAML appends, since the
/// position is already printed in file coordinates at the front.
fn strip_location(message: &str) -> String {
    match message.rfind(" at line ") {
        Some(at) => message[..at].to_string(),
        None => message.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indoc::indoc;
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Front {
        title: String,
        #[serde(default)]
        weight: u64,
    }

    #[test]
    fn a_header_and_a_body_are_separated() {
        let s = split("---\ntitle: Hi\n---\n# Body\n");
        assert_eq!(s.front, "title: Hi\n");
        assert_eq!(s.body, "# Body\n");
        assert_eq!(s.front_line, 2);
        assert_eq!(s.body_line, 4);
    }

    #[test]
    fn a_document_with_no_header_is_all_body() {
        let s = split("# Just markdown\n");
        assert_eq!(s.front, "");
        assert_eq!(s.body, "# Just markdown\n");
        assert_eq!(s.body_line, 1);
    }

    #[test]
    fn an_unclosed_fence_is_left_as_a_body() {
        // Guessing that the author meant a header would hide the real mistake,
        // which is the missing fence.
        let s = split("---\ntitle: Hi\n# Body\n");
        assert_eq!(s.front, "");
        assert_eq!(s.body, "---\ntitle: Hi\n# Body\n");
    }

    #[test]
    fn a_thematic_break_in_the_body_is_not_a_closing_fence() {
        let s = split("---\ntitle: Hi\n---\nabove\n\n---\n\nbelow\n");
        assert_eq!(s.front, "title: Hi\n");
        assert!(s.body.contains("below"), "{:?}", s.body);
    }

    #[test]
    fn a_byte_order_mark_does_not_hide_the_fence() {
        let s = split("\u{feff}---\ntitle: Hi\n---\nbody\n");
        assert_eq!(s.front, "title: Hi\n");
    }

    #[test]
    fn an_error_is_reported_in_file_coordinates() {
        // `weight` is on line 3 of the file and line 2 of the header. The
        // whole point of this module is that the message says 3.
        let source = indoc! {r#"
            ---
            title: Hi
            weight: "3"
            ---
            body
        "#};
        let err = parse::<Front>(&split(source), Path::new("guide/install.md"))
            .expect_err("should reject");
        assert_eq!(err.line, 3, "{err}");
        assert_eq!(err.field.as_deref(), Some("weight"), "{err}");
        assert!(format!("{err}").starts_with("guide/install.md:3:"), "{err}");
        assert!(
            !format!("{err}").contains("at line"),
            "position printed twice: {err}"
        );
    }

    #[test]
    fn an_unknown_key_names_the_alternatives() {
        let source = "---\ntitel: Hi\n---\n";
        let err = parse::<Front>(&split(source), Path::new("p.md")).expect_err("reject");
        assert!(format!("{err}").contains("titel"), "{err}");
        assert!(format!("{err}").contains("title"), "should suggest: {err}");
    }

    #[test]
    fn a_valid_header_round_trips() {
        let front: Front = parse(
            &split("---\ntitle: Hi\nweight: 3\n---\nbody\n"),
            Path::new("p.md"),
        )
        .expect("parse");
        assert_eq!(front.title, "Hi");
        assert_eq!(front.weight, 3);
    }
}

#[cfg(test)]
mod line_tests {
    use super::*;

    #[test]
    fn body_line_counts_both_fences() {
        for (source, expected) in [
            ("---\ntitle: Hi\n---\nbody\n", 4),
            ("---\n---\nbody\n", 3),
            ("---\na: 1\nb: 2\nc: 3\n---\nbody\n", 6),
        ] {
            let s = split(source);
            assert_eq!(s.body_line, expected, "for {source:?}");
            // The claim is checkable: the body really is on that line.
            let line = source.lines().nth(s.body_line - 1).expect("line exists");
            assert_eq!(line, "body", "for {source:?}");
        }
    }
}
