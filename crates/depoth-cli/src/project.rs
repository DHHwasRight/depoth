//! Finding the site, and the few facts every command needs about it.

use std::path::{Path, PathBuf};

use depoth::config::{Config, ConfigError};

use crate::ui::{Diagnostic, Snippet};

/// The file that marks a directory as a depoth site.
pub const MANIFEST: &str = "depoth.yaml";

/// A located, parsed site.
pub struct Project {
    /// The directory holding `depoth.yaml`. Every path below is relative to it.
    pub root: PathBuf,
    pub config_path: PathBuf,
    pub config: Config,
}

impl Project {
    /// Walks up from `start` looking for `depoth.yaml`, the way cargo looks for
    /// `Cargo.toml`. A command run three directories into `content/` should
    /// still work.
    pub fn discover(start: Option<&Path>) -> Result<Self, Vec<Diagnostic>> {
        let start = match start {
            Some(dir) => dir.to_path_buf(),
            None => std::env::current_dir().map_err(|e| {
                vec![Diagnostic::error(
                    "cwd",
                    format!("the current directory is unreadable: {e}"),
                )]
            })?,
        };
        let root = find_root(&start).ok_or_else(|| {
            vec![Diagnostic::error(
                "no-site",
                format!("no {MANIFEST} in {} or any parent", start.display()),
            )
            .help("run `depoth new <name>` to make one, or pass `--dir <DIR>`")]
        })?;
        Self::open(&root)
    }

    /// Loads the site rooted at `root`.
    pub fn open(root: &Path) -> Result<Self, Vec<Diagnostic>> {
        let config_path = root.join(MANIFEST);
        // Read for the error path only: a parse failure quotes the line it
        // is about, and a success has the parsed config instead.
        let source = std::fs::read_to_string(&config_path).unwrap_or_default();
        match Config::load(&config_path) {
            Ok(config) => Ok(Self {
                root: root.to_path_buf(),
                config_path,
                config,
            }),
            Err(e) => Err(vec![config_diagnostic(&e, &config_path, &source)]),
        }
    }

    /// Where the build writes, as an absolute path. `--out` overrides the
    /// config; a relative override is relative to the site root, not the
    /// working directory, so `depoth -C site build --out tmp` is unambiguous.
    pub fn output_dir(&self, over: Option<&Path>) -> PathBuf {
        let dir = over.unwrap_or(self.config.output_dir.as_path());
        self.root.join(dir)
    }

    pub fn content_dir(&self) -> PathBuf {
        self.root.join(&self.config.content_dir)
    }

    /// The markdown behind a navigation slug, if it was written.
    ///
    /// Two spellings, because a section index is naturally `docs/index.md` and
    /// a leaf is naturally `docs/install.md`, and the navigation names both
    /// the same way.
    pub fn source_for(&self, slug: &str) -> Option<PathBuf> {
        let content = self.content_dir();
        let candidates = [
            content.join(format!("{slug}.md")),
            content.join(slug).join("index.md"),
        ];
        candidates.into_iter().find(|p| p.is_file())
    }

    /// A path to show a human: relative to the site root when it is inside it,
    /// absolute when it is not.
    pub fn display(&self, path: &Path) -> String {
        path.strip_prefix(&self.root)
            .unwrap_or(path)
            .display()
            .to_string()
    }
}

fn find_root(start: &Path) -> Option<PathBuf> {
    let mut dir = start.canonicalize().unwrap_or_else(|_| start.to_path_buf());
    loop {
        if dir.join(MANIFEST).is_file() {
            return Some(dir);
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// Turns a config failure into a diagnostic that quotes the offending line.
///
/// `yaml_serde` reports a line and column, which is the whole reason the plan
/// chose it; throwing that away and printing the bare message would make a
/// typo in a 1,100-line navigation file a search rather than a jump.
fn config_diagnostic(error: &ConfigError, path: &Path, source: &str) -> Diagnostic {
    let location = match error {
        ConfigError::Parse { source: e, .. } => e.location(),
        _ => None,
    };
    let message = match error {
        // The outer `{path}: {source}` wrapping is this renderer's job now.
        ConfigError::Parse { source: e, .. } => strip_location(&e.to_string()),
        ConfigError::Read { source: e, .. } => e.to_string(),
        ConfigError::Navigation(e) => e.to_string(),
    };
    let code = match error {
        ConfigError::Navigation(_) => "navigation",
        _ => "config",
    };
    let mut d = Diagnostic::error(code, message);
    match location {
        Some(at) => {
            let line = at.line();
            let text = source.lines().nth(line.saturating_sub(1)).unwrap_or("");
            d = d
                .at(format!("{}:{line}:{}", path.display(), at.column()))
                .snippet(Snippet {
                    line,
                    column: at.column(),
                    text: text.to_string(),
                    span: 1,
                });
        }
        None => d = d.at(path.display().to_string()),
    }
    if matches!(error, ConfigError::Navigation(_)) {
        d = d.help("a node carries exactly one kind of child: tabs, groups or pages");
    }
    d
}

/// `yaml_serde` appends ` at line 4 column 1` to its messages. The location is
/// already in the `-->` line, and saying it twice reads as a stutter.
fn strip_location(message: &str) -> String {
    match message.find(" at line ") {
        Some(at) => message[..at].to_string(),
        None => message.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_duplicated_location_is_said_once() {
        assert_eq!(
            strip_location("unknown field `base_ur1` at line 4 column 1"),
            "unknown field `base_ur1`"
        );
        assert_eq!(strip_location("no location here"), "no location here");
    }
}
