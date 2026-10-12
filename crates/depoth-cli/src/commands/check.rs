//! `depoth check` -- the rules, and a non-zero exit when one does not hold.
//!
//! Three layers, cheapest first, and all of them run so that one command
//! reports every problem rather than the first one.
//!
//! 1. **The navigation against the content tree.** A slug with no file, and a
//!    file no slug names. Needs no build.
//! 2. **Frontmatter.** A page with no `title` has no sidebar label and no
//!    `<title>`, because the navigation deliberately names slugs only.
//! 3. **The built output**, through `depoth-test`. Every rule there is a bug
//!    that reached a human once: a dead link across a rename, a doubled
//!    navbar, a page whose heading levels skip.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Args;
use depoth::frontmatter;
use serde::Deserialize;

use crate::commands::Outcome;
use crate::project::Project;
use crate::ui::{self, Cell, Diagnostic, Severity, Snippet, Table, Ui, ACCENT, DIM, STRONG};

#[derive(Args)]
pub struct CheckArgs {
    /// Where the built site is [default: output_dir in depoth.yaml]
    #[arg(long, value_name = "DIR")]
    pub out: Option<PathBuf>,
    /// Deployment sub-path the output was built for
    #[arg(long, value_name = "PATH")]
    pub base: Option<String>,
    /// A selector that must match exactly once on every page; repeatable
    /// [default: "body > header", "main"]
    #[arg(long, value_name = "SELECTOR")]
    pub unique: Vec<String>,
    /// Do not read the built output; check only the sources
    #[arg(long)]
    pub sources_only: bool,
}

/// The one frontmatter key that is not optional.
///
/// Unknown keys are allowed on purpose: a site declares its own frontmatter
/// type, and a checker that rejected what the site accepts would be wrong
/// about the site rather than about the page.
#[derive(Deserialize)]
struct Front {
    #[allow(dead_code)]
    title: String,
}

pub fn run(ui: &Ui, dir: Option<&Path>, args: CheckArgs) -> Outcome {
    let project = Project::discover(dir)?;
    let out = project.output_dir(args.out.as_deref());

    ui.heading("check", project.display(&project.config_path));
    let steps = ui.steps(&["navigation", "frontmatter", "output"]);
    let mut found: Vec<Diagnostic> = Vec::new();

    // 1. The navigation against the content tree.
    steps.begin("navigation");
    let entries = project.config.navigation.flatten();
    let content = sources(&project.content_dir());
    let mut named: Vec<&str> = Vec::new();
    for entry in &entries {
        named.push(&entry.slug);
        if project.source_for(&entry.slug).is_none() {
            found.push(
                Diagnostic::error(
                    "nav/not-written",
                    format!("{} is in the navigation, but no file was found", entry.slug),
                )
                .at(project.display(&project.config_path))
                .help(format!(
                    "write {}/{}.md, or {}/{}/index.md",
                    project.config.content_dir.display(),
                    entry.slug,
                    project.config.content_dir.display(),
                    entry.slug
                )),
            );
        }
    }
    for slug in content.keys() {
        if !named.contains(&slug.as_str()) {
            found.push(
                Diagnostic::warning(
                    "nav/unreachable",
                    format!("{slug} is written, but no navigation entry names it"),
                )
                .at(project.display(&content[slug]))
                .note("the sidebar, prev/next and llms.txt are all built from the navigation"),
            );
        }
    }
    steps.done(
        "navigation",
        format!("{} entries \u{b7} {} files", entries.len(), content.len()),
    );

    // 2. Frontmatter. The only phase whose size is known in advance, so the
    //    only one that gets a bar rather than a spinner.
    steps.begin("frontmatter");
    let bar = steps.bar(content.len() as u64, "frontmatter");
    for path in content.values() {
        bar.inc(1);
        let Ok(source) = std::fs::read_to_string(path) else {
            continue;
        };
        let split = frontmatter::split(&source);
        if let Err(e) = frontmatter::parse::<Front>(&split, path) {
            let text = source.lines().nth(e.line.saturating_sub(1)).unwrap_or("");
            let mut d = Diagnostic::error("frontmatter", message_of(&e))
                .at(format!("{}:{}:{}", project.display(path), e.line, e.column))
                .snippet(Snippet {
                    line: e.line,
                    column: e.column,
                    text: text.to_string(),
                    span: e.field.as_ref().map(|f| f.len()).unwrap_or(1),
                });
            if e.message.contains("missing field `title`") {
                d = d.help(
                    "every page needs a title: the navigation names slugs only, \
                     and takes its label from here",
                );
            }
            found.push(d);
        }
    }
    bar.finish_and_clear();
    steps.resume();
    steps.done("frontmatter", format!("{} documents", content.len()));

    // 3. The built output.
    steps.begin("output");
    if args.sources_only {
        steps.done("output", "skipped (--sources-only)");
    } else if !out.is_dir() {
        steps.done("output", format!("{} is not built", project.display(&out)));
        found.push(
            Diagnostic::warning(
                "not-built",
                format!(
                    "{} does not exist, so no page was read",
                    project.display(&out)
                ),
            )
            .help("run `depoth build` first, or pass --sources-only"),
        );
    } else {
        let policy = policy(&args);
        match depoth_test::Site::open(&out) {
            Ok(site) => {
                let findings = site.check(&policy);
                steps.done("output", format!("{} pages", site.pages.len()));
                found.extend(findings.iter().map(|f| {
                    Diagnostic::error(f.rule, f.detail.clone())
                        .at(f.route.clone())
                        .note(project.display(&out))
                }));
            }
            Err(e) => {
                steps.done("output", "unreadable");
                found.push(
                    Diagnostic::error("output", format!("{}: {e}", project.display(&out)))
                        .help("run `depoth build` first"),
                );
            }
        }
    }
    steps.end();

    ui.gap();
    for d in &found {
        ui.diagnostic(d);
    }
    let errors = found
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .count();
    let warnings = found.len() - errors;

    if !found.is_empty() {
        ui.table_note(&tally(&found));
        ui.gap();
    }
    if errors > 0 {
        ui.fail(format!(
            "{} \u{b7} {}",
            ui::paint(STRONG, plural(errors, "error")),
            plural(warnings, "warning")
        ));
        Ok(ExitCode::FAILURE)
    } else if warnings > 0 {
        ui.warn(format!("no errors \u{b7} {}", plural(warnings, "warning")));
        Ok(ExitCode::SUCCESS)
    } else {
        ui.done(format!(
            "{} and {} clean",
            ui::paint(ACCENT, project.display(&project.content_dir())),
            ui::paint(ACCENT, project.display(&out))
        ));
        Ok(ExitCode::SUCCESS)
    }
}

/// Findings by rule, worst first. The list above is for fixing one thing; this
/// is for seeing which thing there are forty of.
fn tally(found: &[Diagnostic]) -> Table {
    let mut counts: BTreeMap<(Severity, String), usize> = BTreeMap::new();
    for d in found {
        let code = d.code.clone().unwrap_or_else(|| "-".into());
        *counts.entry((d.severity, code)).or_default() += 1;
    }
    let mut rows: Vec<_> = counts.into_iter().collect();
    rows.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let mut table = Table::new();
    for ((severity, code), n) in rows {
        table.row(vec![
            Cell::styled(n.to_string(), STRONG).right(),
            Cell::styled(code, severity.style()),
            Cell::styled(
                match severity {
                    Severity::Error => "error",
                    Severity::Warning => "warning",
                },
                DIM,
            ),
        ]);
    }
    table
}

fn policy(args: &CheckArgs) -> depoth_test::Policy {
    let mut policy = depoth_test::Policy {
        base: args
            .base
            .as_deref()
            .unwrap_or_default()
            .trim_end_matches('/')
            .to_string(),
        ..Default::default()
    };
    if !args.unique.is_empty() {
        policy.unique = args.unique.clone();
    }
    policy
}

/// Every `.md` under `root`, keyed by the slug the navigation would name it
/// by. `docs/index.md` and `docs.md` are both `docs`, which is the convention
/// the site loader uses and the reason a section index needs no special case.
fn sources(root: &Path) -> BTreeMap<String, PathBuf> {
    let mut out = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
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
            if path.extension().is_none_or(|e| e != "md") {
                continue;
            }
            let Ok(rel) = path.strip_prefix(root) else {
                continue;
            };
            let slug = rel.with_extension("").to_string_lossy().replace('\\', "/");
            let slug = slug.strip_suffix("/index").unwrap_or(&slug).to_string();
            out.insert(slug, path);
        }
    }
    out
}

/// `FrontError`'s own `Display` repeats the path and the position, which the
/// `-->` line already carries.
fn message_of(e: &frontmatter::FrontError) -> String {
    match &e.field {
        Some(field) => format!("{field}: {}", e.message),
        None => e.message.clone(),
    }
}

fn plural(n: usize, noun: &str) -> String {
    if n == 1 {
        format!("1 {noun}")
    } else {
        format!("{n} {noun}s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_section_index_and_a_leaf_get_the_same_shape_of_slug() {
        let root = std::env::temp_dir().join(format!("depoth-sources-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("docs")).unwrap();
        std::fs::write(root.join("docs/index.md"), "").unwrap();
        std::fs::write(root.join("docs/install.md"), "").unwrap();
        std::fs::write(root.join("notes.md"), "").unwrap();
        let found = sources(&root);
        let slugs: Vec<&str> = found.keys().map(String::as_str).collect();
        assert_eq!(slugs, ["docs", "docs/install", "notes"]);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn counting_says_one_error_not_one_errors() {
        assert_eq!(plural(0, "error"), "0 errors");
        assert_eq!(plural(1, "error"), "1 error");
        assert_eq!(plural(2, "warning"), "2 warnings");
    }
}
