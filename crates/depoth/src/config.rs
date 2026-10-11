//! Site configuration.
//!
//! One file, `depoth.yaml`, owning the whole shell: identity, directories,
//! navigation, redirects. YAML rather than TOML because the navigation tree is
//! recursive and nests five deep in real sites, which TOML cannot express
//! readably; and because frontmatter is already YAML, so a project carries one
//! parser and one set of quoting rules rather than two.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::nav::Navigation;

/// Everything a site declares about itself.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Shown in the navbar and appended to every page title.
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    /// Absolute origin. Without it there is no sitemap and no canonical URL.
    #[serde(default)]
    pub base_url: String,

    #[serde(default = "d_content")]
    pub content_dir: PathBuf,
    #[serde(default = "d_output")]
    pub output_dir: PathBuf,
    #[serde(default = "d_public")]
    pub public_dir: PathBuf,
    #[serde(default = "d_assets")]
    pub assets_dir: PathBuf,

    pub navigation: Navigation,

    #[serde(default)]
    pub redirects: Vec<Redirect>,
    /// Shell commands run before assets are collected. The framework proposes
    /// the hook and stays out of what it runs -- this is where a site wires up
    /// Tailwind, or anything else, without depoth knowing it exists.
    #[serde(default)]
    pub commands: Vec<Command>,
    /// Published assets concatenated into one file. A bundle consumes its
    /// sources, so nothing leaves a stray copy behind.
    #[serde(default)]
    pub bundles: Vec<Bundle>,
}

fn d_content() -> PathBuf { PathBuf::from("content") }
fn d_output() -> PathBuf { PathBuf::from("dist") }
fn d_public() -> PathBuf { PathBuf::from("public") }
fn d_assets() -> PathBuf { PathBuf::from("assets") }

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Redirect {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Command {
    pub run: String,
    /// `development` or `production`; absent means both.
    #[serde(default)]
    pub mode: Option<Mode>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bundle {
    pub output: String,
    pub sources: Vec<String>,
    #[serde(default)]
    pub mode: Option<Mode>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Development,
    Production,
}

impl Mode {
    /// Whether something gated on `gate` runs in this mode. An absent gate
    /// runs in both.
    pub fn runs(self, gate: Option<Mode>) -> bool {
        gate.is_none_or(|m| m == self)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("{path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{path}: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: serde_norway::Error,
    },
    #[error("{0}")]
    Navigation(#[from] crate::nav::NavError),
}

impl Config {
    pub fn parse(source: &str, path: &Path) -> Result<Self, ConfigError> {
        let config: Config =
            serde_norway::from_str(source).map_err(|source| ConfigError::Parse {
                path: path.to_path_buf(),
                source,
            })?;
        // Fail here rather than at the first missing page, so a malformed tree
        // is one error naming the node instead of a cascade.
        config.navigation.validate()?;
        Ok(config)
    }

    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let source = std::fs::read_to_string(path).map_err(|source| ConfigError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        Self::parse(&source, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indoc::indoc;

    fn parse(src: &str) -> Result<Config, ConfigError> {
        Config::parse(src, Path::new("depoth.yaml"))
    }

    #[test]
    fn a_minimal_config_takes_its_defaults() {
        let config = parse(indoc! {"
            name: Depoth
            navigation:
              pages: [index]
        "})
        .expect("parse");
        assert_eq!(config.name, "Depoth");
        assert_eq!(config.content_dir, PathBuf::from("content"));
        assert_eq!(config.output_dir, PathBuf::from("dist"));
    }

    #[test]
    fn an_unknown_key_is_an_error_rather_than_silence() {
        // A typo in a config key that is quietly ignored costs more time than
        // any error message does.
        let err = parse(indoc! {"
            name: Depoth
            base_ur1: https://example.com
            navigation:
              pages: [index]
        "})
        .expect_err("should reject");
        assert!(format!("{err}").contains("base_ur1"), "{err}");
    }

    #[test]
    fn a_mode_gate_absent_means_both() {
        assert!(Mode::Development.runs(None));
        assert!(Mode::Production.runs(None));
        assert!(Mode::Production.runs(Some(Mode::Production)));
        assert!(!Mode::Production.runs(Some(Mode::Development)));
    }
}
