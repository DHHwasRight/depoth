//! Asset pipeline: run pre-build commands, collect, bundle, minify,
//! fingerprint.
//!
//! Two directories, two rules. `assets/` is processed: its files are bundled,
//! minified and renamed to carry a content hash. `public/` is copied verbatim
//! by whoever copies it and never reaches this module. The split is not a
//! style preference -- a fingerprinted URL is only useful if something
//! rewrites the reference to it, and `robots.txt`, `favicon.ico`, and anything
//! a third party links to from outside the site must keep the name it was
//! given.
//!
//! The order is fixed and each step pays for the one before it:
//!
//! 1. **Empty `.depoth/generated`.** A file a command produced in a previous
//!    build, under a mode gate that does not fire this time, must not survive
//!    into this one. This is trap 2 in `PLAN.md` -- the predecessor's doc site
//!    shipped two copies of its stylesheet for weeks.
//! 2. **Run `[[command]]`s**, each through a shell, working directory the
//!    project root. This is the whole of "propose, don't choose": Tailwind,
//!    esbuild, or a shell one-liner all wire up here and depoth never learns
//!    their names.
//! 3. **Collect** `assets_dir`, then `.depoth/generated` over the top of it,
//!    so a generated file wins at the same logical path.
//! 4. **Bundle**, which consumes its sources rather than copying them.
//! 5. **Minify**, then **fingerprint**, then write.
//!
//! A basename beginning with `_` marks an input. It can be a bundle source or
//! be `@import`ed by one, but it is never published on its own.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256, Sha384};
use walkdir::WalkDir;

use crate::config::{Bundle, Command, Config, Mode};

/// Where `[[command]]`s are expected to write, relative to the project root.
///
/// Inside the project rather than in a temp directory so that a build can be
/// inspected after it ran, and inside a dot directory so that a site's own
/// `.gitignore` needs one line.
pub const GENERATED_DIR: &str = ".depoth/generated";

/// Hex characters of the SHA-256 kept in a filename. Eight is Hugo's and
/// Vite's width: 4 billion values against a few hundred assets.
const FINGERPRINT_LEN: usize = 8;

// ---------------------------------------------------------------------------
// Manifest
// ---------------------------------------------------------------------------

/// One published file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asset {
    /// Path as written to the output directory, hash included.
    pub published: String,
    /// `sha384-<base64>`, ready to drop into an `integrity` attribute.
    pub integrity: String,
    /// Size of the published bytes.
    pub bytes: usize,
}

/// Logical path to published path, for templates to ask with the name the
/// author wrote rather than the name the build chose.
#[derive(Debug, Clone, Default)]
pub struct Manifest {
    entries: BTreeMap<String, Asset>,
}

impl Manifest {
    pub fn new() -> Self {
        Self::default()
    }

    /// The published path for `logical`, or `logical` itself when the pipeline
    /// never saw it.
    ///
    /// The passthrough is deliberate and load-bearing: `public/` is not
    /// fingerprinted and not in here, so `/favicon.ico` has to come back out
    /// unchanged instead of becoming a missing-asset error. The cost is that a
    /// typo in an `assets/` path is silent -- which is the right trade, since
    /// the alternative breaks every reference to a verbatim file.
    pub fn get<'a>(&'a self, logical: &'a str) -> &'a str {
        match self.entries.get(logical) {
            Some(asset) => &asset.published,
            None => logical,
        }
    }

    /// The full record, or `None` for a path the pipeline never saw.
    pub fn asset(&self, logical: &str) -> Option<&Asset> {
        self.entries.get(logical)
    }

    /// `sha384-<base64>` for a fingerprinted asset.
    ///
    /// `None` for anything else, because subresource integrity on a file that
    /// can change under a stable URL breaks the page the next time it changes.
    /// Only a fingerprinted path is safe to pin.
    pub fn integrity(&self, logical: &str) -> Option<&str> {
        self.entries.get(logical).map(|a| a.integrity.as_str())
    }

    /// Every entry, ordered by logical path.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Asset)> {
        self.entries.iter().map(|(k, v)| (k.as_str(), v))
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// What the pipeline did, for the build log.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    /// `[[command]]`s whose mode gate let them run.
    pub commands_run: usize,
    /// `[[bundle]]`s whose mode gate let them run.
    pub bundles_built: usize,
    /// Files written to the output directory.
    pub files_published: usize,
    /// Bytes read, before bundling and minification.
    pub bytes_in: u64,
    /// Bytes written.
    pub bytes_out: u64,
}

/// A finished pipeline run.
#[derive(Debug, Clone, Default)]
pub struct Build {
    pub manifest: Manifest,
    pub report: Report,
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum AssetError {
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("command `{run}` could not start: {source}")]
    Spawn {
        run: String,
        #[source]
        source: std::io::Error,
    },
    /// Names the command, because a build that fails with a bare exit code
    /// sends you reading the config to work out which of five it was.
    #[error("command `{run}` failed ({status}){stderr}")]
    Command {
        run: String,
        status: String,
        stderr: String,
    },
    #[error("bundle `{output}`: no such source `{missing}`")]
    MissingSource { output: String, missing: String },
    #[error("{path}: {message}")]
    Minify { path: String, message: String },
}

// ---------------------------------------------------------------------------
// Pipeline
// ---------------------------------------------------------------------------

/// Reads `assets_dir`, `[[command]]` and `[[bundle]]`; writes `output_dir`.
#[derive(Debug)]
pub struct Pipeline<'a> {
    config: &'a Config,
    root: PathBuf,
    mode: Mode,
}

impl<'a> Pipeline<'a> {
    pub fn new(config: &'a Config, root: impl Into<PathBuf>, mode: Mode) -> Self {
        Self {
            config,
            root: root.into(),
            mode,
        }
    }

    /// The directory commands are expected to write into.
    pub fn generated_dir(&self) -> PathBuf {
        self.root.join(GENERATED_DIR)
    }

    /// Runs every step and writes the result under `output_dir`.
    pub fn run(&self) -> Result<Build, AssetError> {
        let mut report = Report::default();

        self.reset_generated()?;
        report.commands_run = self.run_commands()?;

        let mut files = self.collect(&mut report)?;
        report.bundles_built = self.bundle(&mut files)?;

        // Inputs leave the set only here, after bundling: a bundle is allowed
        // to name an `_` file as a source, and that is the common case.
        files.retain(|logical, _| !is_input(logical));

        self.publish(files, &mut report)
    }

    // -- step 1 -------------------------------------------------------------

    fn reset_generated(&self) -> Result<(), AssetError> {
        let dir = self.generated_dir();
        if dir.exists() {
            fs::remove_dir_all(&dir).map_err(|source| AssetError::Io {
                path: dir.clone(),
                source,
            })?;
        }
        // Created even when no command runs, so a command added later has
        // somewhere to write without a `mkdir -p` of its own.
        fs::create_dir_all(&dir).map_err(|source| AssetError::Io { path: dir, source })
    }

    // -- step 2 -------------------------------------------------------------

    fn run_commands(&self) -> Result<usize, AssetError> {
        let mut ran = 0;
        for command in &self.config.commands {
            if !self.mode.runs(command.mode) {
                continue;
            }
            self.run_command(command)?;
            ran += 1;
        }
        Ok(ran)
    }

    fn run_command(&self, command: &Command) -> Result<(), AssetError> {
        let (shell, flag) = if cfg!(windows) {
            ("cmd", "/C")
        } else {
            ("sh", "-c")
        };

        // Captured rather than inherited so that the failure message carries
        // the tool's own diagnostic. A build that says "exit status: 1" and
        // nothing else is a build you have to re-run by hand to debug.
        let output = std::process::Command::new(shell)
            .arg(flag)
            .arg(&command.run)
            .current_dir(&self.root)
            .output()
            .map_err(|source| AssetError::Spawn {
                run: command.run.clone(),
                source,
            })?;

        if output.status.success() {
            return Ok(());
        }

        let mut tail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if tail.is_empty() {
            tail = String::from_utf8_lossy(&output.stdout).trim().to_string();
        }
        Err(AssetError::Command {
            run: command.run.clone(),
            status: output.status.to_string(),
            stderr: if tail.is_empty() {
                String::new()
            } else {
                format!(":\n{tail}")
            },
        })
    }

    // -- step 3 -------------------------------------------------------------

    fn collect(&self, report: &mut Report) -> Result<BTreeMap<String, Vec<u8>>, AssetError> {
        let mut files = BTreeMap::new();
        // Source order matters: generated second, so a command that rewrites
        // a checked-in file wins at the same logical path.
        self.collect_dir(&self.root.join(&self.config.assets_dir), &mut files)?;
        self.collect_dir(&self.generated_dir(), &mut files)?;
        report.bytes_in = files.values().map(|b| b.len() as u64).sum();
        Ok(files)
    }

    fn collect_dir(
        &self,
        dir: &Path,
        files: &mut BTreeMap<String, Vec<u8>>,
    ) -> Result<(), AssetError> {
        if !dir.is_dir() {
            // A site with no `assets/` is ordinary, not broken.
            return Ok(());
        }
        // Sorted so that two runs over the same tree report in the same order.
        for entry in WalkDir::new(dir).sort_by_file_name() {
            let entry = entry.map_err(|e| AssetError::Io {
                path: e.path().unwrap_or(dir).to_path_buf(),
                source: e
                    .into_io_error()
                    .unwrap_or_else(|| std::io::Error::other("directory walk failed")),
            })?;
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            let Ok(relative) = path.strip_prefix(dir) else {
                continue;
            };
            let body = fs::read(path).map_err(|source| AssetError::Io {
                path: path.to_path_buf(),
                source,
            })?;
            files.insert(logical_path(relative), body);
        }
        Ok(())
    }

    // -- step 4 -------------------------------------------------------------

    fn bundle(&self, files: &mut BTreeMap<String, Vec<u8>>) -> Result<usize, AssetError> {
        let mut built = 0;
        for bundle in &self.config.bundles {
            if !self.mode.runs(bundle.mode) {
                continue;
            }
            self.build_bundle(bundle, files)?;
            built += 1;
        }
        Ok(built)
    }

    fn build_bundle(
        &self,
        bundle: &Bundle,
        files: &mut BTreeMap<String, Vec<u8>>,
    ) -> Result<(), AssetError> {
        let output = normalize(&bundle.output);
        let mut body: Vec<u8> = Vec::new();
        let mut consumed = Vec::with_capacity(bundle.sources.len());

        for source in &bundle.sources {
            let key = normalize(source);
            // A missing source is an error rather than a skip. Silently
            // dropping it ships a stylesheet that is missing a third of
            // itself and looks like a CSS bug for an afternoon.
            let Some(part) = files.get(&key) else {
                return Err(AssetError::MissingSource {
                    output: output.clone(),
                    missing: key,
                });
            };
            if !body.is_empty() {
                // A newline, because a source whose last line is `//` a
                // comment would otherwise eat the first line of the next one.
                body.push(b'\n');
            }
            body.extend_from_slice(part);
            consumed.push(key);
        }

        // After the loop, so that a bundle naming a missing source leaves the
        // set untouched and the build fails on the first error rather than on
        // a consequence of it.
        for key in consumed {
            // An output listed among its own sources concatenates first and
            // then survives, which is why this runs before the insert.
            files.remove(&key);
        }
        files.insert(output, body);
        Ok(())
    }

    // -- step 5 -------------------------------------------------------------

    fn publish(
        &self,
        files: BTreeMap<String, Vec<u8>>,
        report: &mut Report,
    ) -> Result<Build, AssetError> {
        let out_root = self.root.join(&self.config.output_dir);
        let mut manifest = Manifest::new();

        for (logical, body) in files {
            let body = self.minify(&logical, body)?;
            let published = fingerprinted(&logical, &short_hash(&body));

            let path = out_root.join(published.trim_start_matches('/'));
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|source| AssetError::Io {
                    path: parent.to_path_buf(),
                    source,
                })?;
            }
            fs::write(&path, &body).map_err(|source| AssetError::Io {
                path: path.clone(),
                source,
            })?;

            report.files_published += 1;
            report.bytes_out += body.len() as u64;
            manifest.entries.insert(
                logical,
                Asset {
                    published,
                    integrity: integrity(&body),
                    bytes: body.len(),
                },
            );
        }

        Ok(Build {
            manifest: manifest.clone(),
            report: report.clone(),
        })
    }

    /// Minifies CSS and JS in production only.
    ///
    /// Development skips it for two reasons: it is the slowest step in a loop
    /// that is supposed to feel instant, and a minified stylesheet is one you
    /// cannot read in devtools. The cost is that a syntax error a minifier
    /// would catch surfaces at the production build instead of at the dev one.
    fn minify(&self, logical: &str, body: Vec<u8>) -> Result<Vec<u8>, AssetError> {
        if self.mode != Mode::Production {
            return Ok(body);
        }
        let extension = logical.rsplit('.').next().unwrap_or_default();
        if !matches!(extension, "css" | "js" | "mjs" | "cjs") {
            return Ok(body);
        }
        // Not valid UTF-8 means not source we can parse; pass it through
        // rather than fail on it.
        let Ok(source) = std::str::from_utf8(&body) else {
            return Ok(body);
        };

        let minified = if extension == "css" {
            prerender::assets::minify_css(source, logical)
        } else {
            prerender::assets::minify_js(source, logical)
        };

        match minified {
            Ok(text) => Ok(text.into_bytes()),
            Err(e) => Err(AssetError::Minify {
                path: logical.to_string(),
                message: e.to_string(),
            }),
        }
    }
}

// ---------------------------------------------------------------------------
// Paths and hashes
// ---------------------------------------------------------------------------

/// A path relative to an asset root, as a site-absolute URL path.
fn logical_path(relative: &Path) -> String {
    let mut out = String::from("/");
    for (i, part) in relative.components().enumerate() {
        if i > 0 {
            out.push('/');
        }
        out.push_str(&part.as_os_str().to_string_lossy());
    }
    out
}

/// Accepts `app.css`, `./app.css` and `/app.css` as the same thing, so a
/// config does not have to guess which spelling the pipeline wants.
fn normalize(path: &str) -> String {
    let trimmed = path.trim_start_matches("./").trim_start_matches('/');
    format!("/{trimmed}")
}

/// Whether any segment begins with `_`.
///
/// Every segment, not only the filename: `_partials/` is a directory of inputs
/// and publishing its contents because the leaf names look innocent would
/// defeat the convention.
fn is_input(logical: &str) -> bool {
    logical.split('/').any(|seg| seg.starts_with('_'))
}

/// `/styles/theme.css` with hash `48f9bcc2` becomes
/// `/styles/theme-48f9bcc2.css`.
///
/// Before the extension rather than after it, because a server decides the
/// content type from the extension and `theme.css-48f9bcc2` is served as
/// `application/octet-stream`.
fn fingerprinted(logical: &str, hash: &str) -> String {
    let (dir, file) = match logical.rfind('/') {
        Some(i) => logical.split_at(i + 1),
        None => ("", logical),
    };
    match file.rfind('.') {
        // `i > 0` leaves a dotfile alone: `.htaccess` has no extension, it has
        // a name that starts with a dot.
        Some(i) if i > 0 => {
            let (stem, extension) = file.split_at(i);
            format!("{dir}{stem}-{hash}{extension}")
        }
        _ => format!("{dir}{file}-{hash}"),
    }
}

/// First eight hex characters of the SHA-256 of the contents.
fn short_hash(body: &[u8]) -> String {
    let digest = Sha256::digest(body);
    let mut out = String::with_capacity(FINGERPRINT_LEN);
    for byte in digest.iter() {
        if out.len() >= FINGERPRINT_LEN {
            break;
        }
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// `sha384-<standard base64>`, the form an `integrity` attribute takes.
fn integrity(body: &[u8]) -> String {
    format!("sha384-{}", base64(&Sha384::digest(body)))
}

/// Standard base64 with padding.
///
/// Hand-written rather than a dependency: this is the only base64 in the
/// crate, the input is a fixed 48 bytes, and the alternative is a crate in the
/// tree for sixteen lines.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let packed = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[(packed >> 18) as usize & 0x3f] as char);
        out.push(ALPHABET[(packed >> 12) as usize & 0x3f] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[(packed >> 6) as usize & 0x3f] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[packed as usize & 0x3f] as char
        } else {
            '='
        });
    }
    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use indoc::indoc;

    /// A fixture tree under the workspace `target/`, so the suite needs no
    /// temp-file dependency and a failing test leaves its evidence on disk.
    fn fixture(name: &str) -> PathBuf {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-assets")
            .join(name);
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("create fixture");
        root
    }

    fn write(root: &Path, relative: &str, body: &str) {
        let path = root.join(relative);
        fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        fs::write(path, body).expect("write");
    }

    fn config(extra: &str) -> Config {
        let source = format!("name: Test\nnavigation:\n  pages: [index]\n{extra}");
        Config::parse(&source, Path::new("depoth.yaml")).expect("parse config")
    }

    fn run(root: &Path, config: &Config, mode: Mode) -> Result<Build, AssetError> {
        Pipeline::new(config, root, mode).run()
    }

    #[test]
    fn a_file_is_fingerprinted_and_the_manifest_maps_it() {
        let root = fixture("fingerprint");
        write(&root, "assets/styles/theme.css", "body{color:red}");

        let build = run(&root, &config(""), Mode::Development).expect("run");

        let published = build.manifest.get("/styles/theme.css");
        assert_ne!(published, "/styles/theme.css", "should be renamed");
        assert!(
            published.starts_with("/styles/theme-") && published.ends_with(".css"),
            "{published}"
        );
        // The hash goes before the extension, eight hex wide.
        let stem = published
            .trim_start_matches("/styles/theme-")
            .trim_end_matches(".css");
        assert_eq!(stem.len(), FINGERPRINT_LEN, "{published}");
        assert!(stem.chars().all(|c| c.is_ascii_hexdigit()), "{published}");

        assert!(root.join("dist").join(&published[1..]).is_file());
        assert_eq!(build.report.files_published, 1);
    }

    #[test]
    fn an_unknown_path_comes_back_unchanged() {
        // `public/` is never fingerprinted, so a reference to it must survive
        // a manifest lookup rather than become an error.
        let manifest = Manifest::new();
        assert_eq!(manifest.get("/favicon.ico"), "/favicon.ico");
        assert!(manifest.integrity("/favicon.ico").is_none());
    }

    #[test]
    fn an_underscore_prefixed_file_is_not_published() {
        let root = fixture("underscore");
        write(&root, "assets/app.css", "body{color:red}");
        write(&root, "assets/_variables.css", "--c: red;");
        write(&root, "assets/_partials/reset.css", "*{margin:0}");

        let build = run(&root, &config(""), Mode::Development).expect("run");

        assert_eq!(build.manifest.len(), 1, "{:?}", build.manifest);
        assert!(build.manifest.asset("/app.css").is_some());
        assert_eq!(build.manifest.get("/_variables.css"), "/_variables.css");
        // A directory beginning with `_` hides its contents too.
        assert!(build.manifest.asset("/_partials/reset.css").is_none());
    }

    #[test]
    fn a_bundle_consumes_its_sources() {
        let root = fixture("bundle");
        write(&root, "assets/_base.css", "a{}");
        write(&root, "assets/layout.css", "b{}");
        write(&root, "assets/other.css", "c{}");

        let build = run(
            &root,
            &config(indoc! {"
                bundles:
                  - output: /app.css
                    sources: [/_base.css, /layout.css]
            "}),
            Mode::Development,
        )
        .expect("run");

        // The bundle is published; neither source is, including the one that
        // would have shipped on its own.
        assert!(build.manifest.asset("/app.css").is_some());
        assert!(build.manifest.asset("/layout.css").is_none());
        assert!(build.manifest.asset("/_base.css").is_none());
        assert!(build.manifest.asset("/other.css").is_some());
        assert_eq!(build.report.bundles_built, 1);

        // Sources joined in declaration order, separated by a newline.
        let path = root.join("dist").join(&build.manifest.get("/app.css")[1..]);
        assert_eq!(fs::read_to_string(path).expect("read"), "a{}\nb{}");
    }

    #[test]
    fn a_missing_bundle_source_names_itself() {
        let root = fixture("bundle-missing");
        write(&root, "assets/layout.css", "b{}");

        let err = run(
            &root,
            &config(indoc! {"
                bundles:
                  - output: /app.css
                    sources: [/layout.css, /typo.css]
            "}),
            Mode::Development,
        )
        .expect_err("should fail");

        let message = err.to_string();
        assert!(message.contains("/typo.css"), "{message}");
        assert!(message.contains("/app.css"), "{message}");
    }

    #[test]
    fn a_bundle_honours_its_mode_gate() {
        let root = fixture("bundle-gated");
        write(&root, "assets/layout.css", "b{}");

        let build = run(
            &root,
            &config(indoc! {"
                bundles:
                  - output: /app.css
                    sources: [/layout.css]
                    mode: production
            "}),
            Mode::Development,
        )
        .expect("run");

        assert_eq!(build.report.bundles_built, 0);
        assert!(build.manifest.asset("/app.css").is_none());
        // The source was not consumed, so it still ships on its own.
        assert!(build.manifest.asset("/layout.css").is_some());
    }

    #[test]
    fn a_mode_gated_command_does_not_run_in_the_other_mode() {
        let config = config(indoc! {"
            commands:
              - run: printf 'dev{}' > .depoth/generated/only-dev.css
                mode: development
              - run: printf 'always{}' > .depoth/generated/always.css
        "});

        let dev = fixture("command-dev");
        let build = run(&dev, &config, Mode::Development).expect("dev run");
        assert_eq!(build.report.commands_run, 2);
        assert!(build.manifest.asset("/only-dev.css").is_some());
        assert!(build.manifest.asset("/always.css").is_some());

        let prod = fixture("command-prod");
        let build = run(&prod, &config, Mode::Production).expect("prod run");
        assert_eq!(build.report.commands_run, 1);
        assert!(build.manifest.asset("/only-dev.css").is_none());
        assert!(build.manifest.asset("/always.css").is_some());
    }

    #[test]
    fn a_failing_command_fails_the_build_and_names_itself() {
        let root = fixture("command-fails");

        let err = run(
            &root,
            &config(indoc! {"
                commands:
                  - run: echo 'tailwind exploded' >&2; exit 3
            "}),
            Mode::Development,
        )
        .expect_err("should fail");

        let message = err.to_string();
        assert!(message.contains("exit 3"), "names the command: {message}");
        assert!(
            message.contains("tailwind exploded"),
            "carries stderr: {message}"
        );
    }

    #[test]
    fn the_generated_directory_is_emptied_first() {
        let root = fixture("generated-reset");
        write(
            &root,
            ".depoth/generated/stale.css",
            "from a previous build",
        );

        let build = run(&root, &config(""), Mode::Development).expect("run");

        assert!(build.manifest.asset("/stale.css").is_none());
        assert!(!root.join(".depoth/generated/stale.css").exists());
        // Emptied, not removed: a command must have somewhere to write.
        assert!(root.join(".depoth/generated").is_dir());
    }

    #[test]
    fn a_generated_file_wins_over_the_same_path_in_assets() {
        let root = fixture("generated-wins");
        write(&root, "assets/app.css", "checked in");

        let build = run(
            &root,
            &config(indoc! {"
                commands:
                  - run: printf 'generated' > .depoth/generated/app.css
            "}),
            Mode::Development,
        )
        .expect("run");

        let path = root.join("dist").join(&build.manifest.get("/app.css")[1..]);
        assert_eq!(fs::read_to_string(path).expect("read"), "generated");
    }

    #[test]
    fn identical_contents_hash_identically_and_one_byte_does_not() {
        let same = short_hash(b"body{color:red}");
        assert_eq!(same, short_hash(b"body{color:red}"));
        assert_ne!(same, short_hash(b"body{color:req}"));
        assert_eq!(same.len(), FINGERPRINT_LEN);

        // And end to end: the same file in two projects gets the same name.
        let a = fixture("hash-a");
        let b = fixture("hash-b");
        let c = fixture("hash-c");
        write(&a, "assets/theme.css", "body{color:red}");
        write(&b, "assets/theme.css", "body{color:red}");
        write(&c, "assets/theme.css", "body{color:req}");

        let config = config("");
        let name = |root: &Path| {
            run(root, &config, Mode::Development)
                .expect("run")
                .manifest
                .get("/theme.css")
                .to_string()
        };

        assert_eq!(name(&a), name(&b));
        assert_ne!(name(&a), name(&c));
    }

    #[test]
    fn a_fingerprinted_asset_carries_subresource_integrity() {
        let root = fixture("integrity");
        write(&root, "assets/app.js", "export const a = 1;\n");

        let build = run(&root, &config(""), Mode::Development).expect("run");

        let sri = build.manifest.integrity("/app.js").expect("integrity");
        assert!(sri.starts_with("sha384-"), "{sri}");
        // 48 bytes of SHA-384 are 64 base64 characters, no padding needed.
        assert_eq!(sri.len(), "sha384-".len() + 64, "{sri}");

        // Known vector, so a change to the encoder is caught rather than
        // being compared against itself.
        assert_eq!(base64(b"abc"), "YWJj", "base64 of a whole group");
        assert_eq!(base64(b"ab"), "YWI=", "one byte of padding");
        assert_eq!(base64(b"a"), "YQ==", "two bytes of padding");
    }

    #[test]
    fn production_minifies_and_development_does_not() {
        let source = ".a {\n  color : red ;\n}\n";

        let dev = fixture("minify-dev");
        write(&dev, "assets/app.css", source);
        let build = run(&dev, &config(""), Mode::Development).expect("dev run");
        assert_eq!(build.report.bytes_out, source.len() as u64);

        let prod = fixture("minify-prod");
        write(&prod, "assets/app.css", source);
        let build = run(&prod, &config(""), Mode::Production).expect("prod run");
        assert!(
            build.report.bytes_out < source.len() as u64,
            "{:?}",
            build.report
        );
        let path = prod.join("dist").join(&build.manifest.get("/app.css")[1..]);
        assert_eq!(fs::read_to_string(path).expect("read"), ".a{color:red}");
    }

    #[test]
    fn a_broken_stylesheet_fails_the_production_build() {
        let root = fixture("minify-broken");
        write(&root, "assets/app.css", ".a { color: ; !! }");

        let err = run(&root, &config(""), Mode::Production).expect_err("should fail");
        assert!(err.to_string().contains("/app.css"), "{err}");
    }

    #[test]
    fn an_absent_assets_directory_is_not_an_error() {
        let root = fixture("no-assets");
        let build = run(&root, &config(""), Mode::Development).expect("run");
        assert!(build.manifest.is_empty());
        assert_eq!(build.report, Report::default());
    }

    #[test]
    fn a_config_path_is_accepted_with_or_without_a_leading_slash() {
        assert_eq!(normalize("app.css"), "/app.css");
        assert_eq!(normalize("./app.css"), "/app.css");
        assert_eq!(normalize("/app.css"), "/app.css");
    }

    #[test]
    fn the_hash_goes_before_the_extension() {
        assert_eq!(
            fingerprinted("/styles/theme.css", "48f9bcc2"),
            "/styles/theme-48f9bcc2.css"
        );
        assert_eq!(fingerprinted("/LICENSE", "48f9bcc2"), "/LICENSE-48f9bcc2");
        assert_eq!(
            fingerprinted("/.htaccess", "48f9bcc2"),
            "/.htaccess-48f9bcc2"
        );
    }
}
