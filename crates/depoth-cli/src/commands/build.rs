//! `depoth build` -- compile the templates, then run them over the content.
//!
//! The two clocks are visible here on purpose. A site is a Rust binary: its
//! templates are compiled into it, and its content is read at run time. So a
//! build is literally two commands, and the summary reports their two
//! durations separately, because the whole design rests on the second one
//! staying short.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::time::{Duration, Instant};

use clap::Args;
use depoth::config::Mode;

use crate::commands::Outcome;
use crate::project::Project;
use crate::ui::{self, Cell, Diagnostic, Face, Table, Ui, ACCENT, DIM, PLAIN, STRONG};

#[derive(Args)]
pub struct BuildArgs {
    /// Where the built site goes [default: output_dir in depoth.yaml]
    #[arg(long, value_name = "DIR")]
    pub out: Option<PathBuf>,
    /// Deployment sub-path, e.g. /depoth for a GitHub Pages project site
    #[arg(long, value_name = "PATH")]
    pub base: Option<String>,
    /// Do not keep the partial navigation trees
    #[arg(long)]
    pub no_partials: bool,
    /// Compile with optimisations. Minutes, not seconds: for a deploy, never
    /// for authoring
    #[arg(long)]
    pub release: bool,
}

pub fn run(ui: &Ui, dir: Option<&Path>, args: BuildArgs) -> Outcome {
    let started = Instant::now();
    let project = Project::discover(dir)?;
    let mode = if args.release {
        Mode::Production
    } else {
        Mode::Development
    };
    let out = project.output_dir(args.out.as_deref());
    let partial_out = partial_dir(&out);
    // The sub-path is read by the site's own `build.rs`, so it is part of the
    // *compile*, not the render. Setting it on both is harmless and means a
    // forgetful future reader cannot get it half right.
    let base = normalise_base(args.base.as_deref());

    ui.heading("build", project.display(&project.config_path));

    let steps = ui.steps(&["config", "commands", "compile", "render"]);
    let pages = project.config.navigation.flatten().len();
    steps.done(
        "config",
        format!(
            "{} \u{b7} {pages} pages \u{b7} {}",
            project.display(&project.config_path),
            if args.release {
                "production"
            } else {
                "development"
            }
        ),
    );

    // 1. The pre-build hook. The framework proposes it and stays out of what
    //    it runs -- this is where a site wires up Tailwind without depoth
    //    knowing Tailwind exists.
    steps.begin("commands");
    let wanted: Vec<&str> = project
        .config
        .commands
        .iter()
        .filter(|c| mode.runs(c.mode))
        .map(|c| c.run.as_str())
        .collect();
    for run in &wanted {
        steps.begin(run);
        let output = shell(run, &project.root).map_err(|e| {
            vec![Diagnostic::error("command", format!("{run}: {e}"))
                .at(project.display(&project.config_path))]
        })?;
        if !output.status.success() {
            steps.end();
            ui.verbatim(&String::from_utf8_lossy(&output.stderr));
            return Err(vec![Diagnostic::error(
                "command",
                format!("`{run}` exited with {}", code_of(&output.status)),
            )
            .at(project.display(&project.config_path))
            .help("a pre-build command must succeed before anything is collected")]);
        }
    }
    steps.done(
        "commands",
        match wanted.len() {
            0 => "none declared".to_string(),
            1 => wanted[0].to_string(),
            n => format!("{n} run"),
        },
    );

    // 2. The slow clock. Compile only -- the binary is run separately below,
    //    so the two durations are each other's, not one blur.
    steps.begin("compile");
    let compile_started = Instant::now();
    let exe = compile(ui, &project, &base, args.release)?;
    let compiled = compile_started.elapsed();
    steps.done(
        "compile",
        format!(
            "{} in {}",
            if args.release { "release" } else { "debug" },
            ui::elapsed(compiled)
        ),
    );

    // 3. The fast clock.
    steps.begin("render");
    let render_started = Instant::now();
    empty(&out).map_err(|e| {
        vec![
            Diagnostic::error("output", format!("{}: {e}", out.display())).help(
                "the output directory is emptied first, so a renamed page cannot \
                 leave its old file behind",
            ),
        ]
    })?;
    let output = Command::new(&exe)
        .arg("build")
        .arg("--out")
        .arg(&out)
        .arg("--partial-out")
        .arg(&partial_out)
        .env("DEPOTH_BASE", &base)
        .current_dir(&project.root)
        .output()
        .map_err(|e| {
            vec![Diagnostic::error(
                "render",
                format!("{}: {e}", exe.display()),
            )]
        })?;
    if !output.status.success() {
        steps.end();
        ui.verbatim(&String::from_utf8_lossy(&output.stderr));
        return Err(vec![Diagnostic::error(
            "render",
            format!("the site binary exited with {}", code_of(&output.status)),
        )
        .at(project.display(&exe))]);
    }
    let rendered = render_started.elapsed();

    let mut partials = measure(&partial_out);
    if args.no_partials && partial_out.exists() {
        let _ = std::fs::remove_dir_all(&partial_out);
        partials = Measured::default();
    }
    let built = measure(&out);
    steps.done(
        "render",
        format!("{} files in {}", built.files, ui::elapsed(rendered)),
    );
    steps.end();

    report(
        ui,
        &project,
        &out,
        &built,
        &partials,
        args.no_partials,
        &base,
        compiled,
        rendered,
        started.elapsed(),
    );
    Ok(ExitCode::SUCCESS)
}

// -- compiling --------------------------------------------------------------

/// Builds the site crate and returns the binary cargo produced.
///
/// `--message-format=json-render-diagnostics` is the pairing that matters:
/// cargo still renders its errors as the text a Rust developer reads, on
/// stderr, and the machine-readable artifact records arrive separately on
/// stdout. That is how the binary's path is learned without guessing at
/// `target/debug/<name>`, which is wrong the moment a workspace moves its
/// target directory or a `[[bin]]` is renamed.
fn compile(
    ui: &Ui,
    project: &Project,
    base: &str,
    release: bool,
) -> Result<PathBuf, Vec<Diagnostic>> {
    let manifest = project.root.join("Cargo.toml");
    if !manifest.is_file() {
        return Err(vec![Diagnostic::error(
            "no-crate",
            format!(
                "no Cargo.toml beside {}",
                project.display(&project.config_path)
            ),
        )
        .at(project.display(&manifest))
        .note("a depoth site is a Rust binary: its templates are compiled into it")
        .help(
            "run `depoth new <name>` to get the layout, or build the crate yourself",
        )]);
    }

    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let mut cmd = Command::new(&cargo);
    cmd.arg("build")
        .arg("--manifest-path")
        .arg(&manifest)
        .arg("--message-format=json-render-diagnostics")
        .env("DEPOTH_BASE", base)
        // The one decision, handed to the subprocess so its diagnostics match
        // the rest of the transcript rather than guessing on their own.
        .env(
            "CARGO_TERM_COLOR",
            match ui.face() {
                Face::Terminal => "always",
                Face::Plain => "never",
            },
        )
        .current_dir(&project.root)
        .stdin(Stdio::null());
    if release {
        cmd.arg("--release");
    }
    let output = cmd.output().map_err(|e| {
        vec![
            Diagnostic::error("cargo", format!("{}: {e}", cargo.to_string_lossy()))
                .help("a depoth site is compiled by cargo; install Rust, or put cargo on PATH"),
        ]
    })?;
    if !output.status.success() {
        ui.verbatim(&String::from_utf8_lossy(&output.stderr));
        return Err(vec![Diagnostic::error(
            "compile",
            format!("cargo exited with {}", code_of(&output.status)),
        )
        .at(project.display(&manifest))]);
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    executable_in(&stdout).ok_or_else(|| {
        vec![
            Diagnostic::error("no-binary", "the site crate built no executable")
                .at(project.display(&manifest))
                .help("add a [[bin]] target, or a src/main.rs"),
        ]
    })
}

/// Pulls the last `"executable"` out of cargo's JSON records.
///
/// Hand-rolled rather than through serde: one field is wanted out of a stream
/// whose shape cargo documents as stable, and a JSON dependency here would be
/// a dependency in the shipped binary for a single string.
fn executable_in(stdout: &str) -> Option<PathBuf> {
    let mut found = None;
    for line in stdout.lines() {
        let Some(rest) = line.split("\"executable\":").nth(1) else {
            continue;
        };
        let rest = rest.trim_start();
        // A library or a build script records `"executable":null`.
        let Some(rest) = rest.strip_prefix('"') else {
            continue;
        };
        let mut path = String::new();
        let mut chars = rest.chars();
        while let Some(c) = chars.next() {
            match c {
                '"' => break,
                '\\' => match chars.next() {
                    Some('\\') => path.push('\\'),
                    Some('"') => path.push('"'),
                    Some('n') => path.push('\n'),
                    Some(other) => path.push(other),
                    None => break,
                },
                other => path.push(other),
            }
        }
        if !path.is_empty() {
            found = Some(PathBuf::from(path));
        }
    }
    found
}

// -- measuring --------------------------------------------------------------

#[derive(Debug, Default, Clone, Copy)]
struct Measured {
    pages: u64,
    assets: u64,
    bytes: u64,
    files: u64,
}

/// Counts what was actually written, rather than trusting what the renderer
/// said it wrote. The output directory is the artefact; everything else is a
/// claim about it.
fn measure(dir: &Path) -> Measured {
    let mut m = Measured::default();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            m.files += 1;
            if path.extension().is_some_and(|e| e == "html") {
                m.pages += 1;
            } else {
                m.assets += 1;
            }
            m.bytes += entry.metadata().map(|md| md.len()).unwrap_or(0);
        }
    }
    m
}

#[allow(clippy::too_many_arguments)]
fn report(
    ui: &Ui,
    project: &Project,
    out: &Path,
    built: &Measured,
    partials: &Measured,
    no_partials: bool,
    base: &str,
    compiled: Duration,
    rendered: Duration,
    total: Duration,
) {
    ui.gap();
    let mut table = Table::new();
    let mut row = |label: &str, value: String, aside: String| {
        table.row(vec![
            Cell::styled(label.to_string(), DIM),
            Cell::styled(value, STRONG).right(),
            Cell::styled(aside, DIM),
        ]);
    };
    row("pages", ui::thousands(built.pages), String::new());
    row("assets", ui::thousands(built.assets), String::new());
    row(
        "bytes",
        ui::thousands(built.bytes),
        ui::human_bytes(built.bytes),
    );
    if no_partials {
        row("partials", "\u{2014}".into(), "not published".into());
    } else if partials.files > 0 {
        row(
            "partials",
            ui::thousands(partials.bytes),
            format!("{} files", partials.files),
        );
    }
    if !base.is_empty() {
        row("base", base.to_string(), "deployment sub-path".into());
    }
    row("compile", ui::elapsed(compiled), "templates".into());
    row("render", ui::elapsed(rendered), "content".into());
    row("total", ui::elapsed(total), String::new());
    ui.table_note(&table);
    ui.gap();
    ui.done(format!(
        "{} {}",
        ui::paint(PLAIN, "built"),
        ui::paint(ACCENT, project.display(out))
    ));
}

// -- odds and ends ----------------------------------------------------------

/// `dist` -> `dist-partial`, beside it rather than inside it. A partial tree
/// inside the published one would be served as pages.
fn partial_dir(out: &Path) -> PathBuf {
    let name = out
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "dist".into());
    out.with_file_name(format!("{name}-partial"))
}

/// Trims the sub-path to the one spelling the rest of the tree expects:
/// a leading slash, no trailing one, and empty when there is none.
fn normalise_base(base: Option<&str>) -> String {
    let base = base.unwrap_or("").trim().trim_end_matches('/');
    if base.is_empty() {
        String::new()
    } else if base.starts_with('/') {
        base.to_string()
    } else {
        format!("/{base}")
    }
}

/// Empties the output directory before a build.
///
/// A rename that leaves its old file behind produces a site that serves two
/// versions of a page and a harness that passes. This has already happened
/// once in this project's history -- a stylesheet shipped twice for weeks --
/// which is why it is a step rather than a note in a readme.
///
/// The guard exists because the path is user-supplied: `--out .` in a site
/// root would otherwise delete the site.
fn empty(out: &Path) -> std::io::Result<()> {
    if !out.exists() {
        return std::fs::create_dir_all(out);
    }
    for marker in [crate::project::MANIFEST, "Cargo.toml", ".git"] {
        if out.join(marker).exists() {
            return Err(std::io::Error::other(format!(
                "refusing to empty a directory holding {marker}"
            )));
        }
    }
    for entry in std::fs::read_dir(out)? {
        let path = entry?.path();
        if path.is_dir() {
            std::fs::remove_dir_all(path)?;
        } else {
            std::fs::remove_file(path)?;
        }
    }
    Ok(())
}

fn shell(command: &str, dir: &Path) -> std::io::Result<std::process::Output> {
    #[cfg(windows)]
    let mut cmd = {
        let mut c = Command::new("cmd");
        c.arg("/C").arg(command);
        c
    };
    #[cfg(not(windows))]
    let mut cmd = {
        let mut c = Command::new("sh");
        c.arg("-c").arg(command);
        c
    };
    cmd.current_dir(dir).stdin(Stdio::null()).output()
}

fn code_of(status: &std::process::ExitStatus) -> String {
    match status.code() {
        Some(code) => format!("status {code}"),
        None => "a signal".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_executable_is_the_last_non_null_one() {
        let stdout = concat!(
            r#"{"reason":"compiler-artifact","executable":null,"target":{}}"#,
            "\n",
            r#"{"reason":"compiler-artifact","executable":"/t/debug/build-script-build"}"#,
            "\n",
            r#"{"reason":"compiler-artifact","executable":"/t/debug/my-site"}"#,
            "\n",
            r#"{"reason":"build-finished","success":true}"#,
            "\n",
        );
        assert_eq!(
            executable_in(stdout),
            Some(PathBuf::from("/t/debug/my-site"))
        );
    }

    #[test]
    fn a_path_with_an_escape_survives_the_extractor() {
        let stdout = r#"{"executable":"/a b/c\"d\\e/site"}"#;
        assert_eq!(
            executable_in(stdout),
            Some(PathBuf::from("/a b/c\"d\\e/site"))
        );
    }

    #[test]
    fn a_build_that_produced_nothing_runnable_is_not_a_path() {
        assert_eq!(executable_in(r#"{"executable":null}"#), None);
        assert_eq!(executable_in(""), None);
    }

    #[test]
    fn the_sub_path_has_one_spelling() {
        assert_eq!(normalise_base(None), "");
        assert_eq!(normalise_base(Some("")), "");
        assert_eq!(normalise_base(Some("/")), "");
        assert_eq!(normalise_base(Some("depoth")), "/depoth");
        assert_eq!(normalise_base(Some("/depoth/")), "/depoth");
    }

    #[test]
    fn the_partial_tree_sits_beside_the_published_one() {
        assert_eq!(
            partial_dir(Path::new("/s/dist")),
            PathBuf::from("/s/dist-partial")
        );
    }

    #[test]
    fn emptying_refuses_a_directory_that_holds_a_project() {
        let dir = std::env::temp_dir().join(format!("depoth-empty-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("Cargo.toml"), "").unwrap();
        let err = empty(&dir).expect_err("should refuse");
        assert!(err.to_string().contains("Cargo.toml"), "{err}");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
