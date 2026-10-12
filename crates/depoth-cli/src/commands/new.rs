//! `depoth new` -- scaffold a site.
//!
//! Interactive when a person is watching and an answer is missing. **Every
//! answer is also a flag**, so the prompt is a convenience and never the only
//! route: the same site can be produced by a script, a Dockerfile or a CI job
//! with no terminal attached. With no terminal and no name, the command fails
//! naming the argument rather than blocking on a read that will never return.

use std::path::Path;
use std::process::{Command, ExitCode, Stdio};

use clap::{Args, ValueEnum};
use inquire::ui::{Color, RenderConfig, StyleSheet, Styled};
use inquire::{Confirm, Select, Text};

use crate::commands::Outcome;
use crate::ui::{Cell, Diagnostic, Table, Ui, ACCENT, DIM, STRONG};

#[derive(Args)]
pub struct NewArgs {
    /// Directory to create, and the cargo package name
    #[arg(value_name = "NAME")]
    pub name: Option<String>,
    /// Site name, shown in the navbar and appended to every page title
    #[arg(long, value_name = "TEXT")]
    pub title: Option<String>,
    /// One sentence about the site
    #[arg(long, value_name = "TEXT")]
    pub description: Option<String>,
    /// Absolute origin. Without it there is no sitemap and no canonical URL
    #[arg(long, value_name = "URL")]
    pub base_url: Option<String>,
    /// Which navigation to start from
    #[arg(long, value_enum, value_name = "SHAPE")]
    pub layout: Option<Shape>,
    /// Initialise a git repository
    #[arg(long)]
    pub git: bool,
    /// Do not initialise a git repository
    #[arg(long, conflicts_with = "git")]
    pub no_git: bool,
    /// Write into a directory that already exists
    #[arg(long)]
    pub force: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Shape {
    /// One page, no tabs. Grows into anything
    Minimal,
    /// A tab with two groups, the shape a documentation site settles into
    Docs,
}

impl std::fmt::Display for Shape {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Shape::Minimal => f.write_str("minimal  -- one page, no tabs"),
            Shape::Docs => f.write_str("docs     -- a tab with two groups"),
        }
    }
}

/// The answers, however they arrived.
struct Answers {
    name: String,
    title: String,
    description: String,
    base_url: String,
    shape: Shape,
    git: bool,
}

pub fn run(ui: &Ui, dir: Option<&Path>, args: NewArgs) -> Outcome {
    let answers = ask(ui, &args)?;
    // `-C` is where to create it. Everywhere else it means "the site root",
    // and here there is no site yet, so it is the directory to put one in.
    let root = dir.unwrap_or(Path::new(".")).join(&answers.name);
    if root.exists() && !args.force {
        return Err(vec![Diagnostic::error(
            "exists",
            format!("{} already exists", root.display()),
        )
        .help("pass --force to write into it anyway, or choose another name")]);
    }

    ui.heading("new", &answers.name);
    let files = scaffold(&answers);
    let mut table = Table::new();
    for (path, contents) in &files {
        let full = root.join(path);
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent).map_err(|e| io_error(parent, &e))?;
        }
        std::fs::write(&full, contents).map_err(|e| io_error(&full, &e))?;
        table.row(vec![
            Cell::styled(format!("{}/{path}", answers.name), ACCENT),
            Cell::styled(format!("{} bytes", contents.len()), DIM).right(),
        ]);
    }
    ui.table_note(&table);

    if answers.git {
        let ok = Command::new("git")
            .arg("init")
            .arg("--quiet")
            .current_dir(&root)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if ok {
            ui.note(format!("  {}", crate::ui::paint(DIM, "git repository")));
        } else {
            ui.warn("git init did not run; the files are written either way");
        }
    }

    ui.gap();
    ui.done(format!(
        "{} {}",
        crate::ui::paint(crate::ui::PLAIN, "created"),
        crate::ui::paint(ACCENT, root.display())
    ));
    ui.gap();
    let mut next = Table::new().indent(4);
    next.row(vec![
        Cell::styled(format!("cd {}", answers.name), STRONG),
        Cell::new(""),
    ]);
    next.row(vec![
        Cell::styled("depoth routes", STRONG),
        Cell::styled("see the navigation", DIM),
    ]);
    next.row(vec![
        Cell::styled("depoth build", STRONG),
        Cell::styled("compile the templates, render the content", DIM),
    ]);
    ui.table_note(&next);
    Ok(ExitCode::SUCCESS)
}

// -- the answers ------------------------------------------------------------

fn ask(ui: &Ui, args: &NewArgs) -> Result<Answers, Vec<Diagnostic>> {
    if ui.interactive() {
        inquire::set_global_render_config(prompt_style());
    }

    let name = match &args.name {
        Some(name) => name.clone(),
        None if ui.interactive() => prompt(
            Text::new("Directory name")
                .with_help_message("lowercase, digits, - and _; also the cargo package name")
                .prompt(),
        )?,
        // The whole reason every answer is also a flag. A prompt here would
        // block forever in CI, which is worse than any error message.
        None => {
            return Err(vec![Diagnostic::error(
                "missing-argument",
                "no name, and no terminal to ask for one",
            )
            .help("pass it: `depoth new <NAME>`")
            .note(
                "every prompt this command would show has a flag: --title, \
                   --description, --base-url, --layout, --git/--no-git",
            )])
        }
    };
    check_name(&name)?;

    let default_title = title_case(&name);
    let title = match &args.title {
        Some(title) => title.clone(),
        None if ui.interactive() => prompt(
            Text::new("Site name")
                .with_default(&default_title)
                .with_help_message("shown in the navbar and appended to every page title")
                .prompt(),
        )?,
        None => default_title,
    };

    let description = match &args.description {
        Some(text) => text.clone(),
        None if ui.interactive() => prompt(
            Text::new("Description")
                .with_help_message("one sentence; used for the meta description. Enter to skip")
                .prompt(),
        )?,
        None => String::new(),
    };

    let base_url = match &args.base_url {
        Some(url) => url.clone(),
        None if ui.interactive() => prompt(
            Text::new("Base URL")
                .with_default("https://example.com")
                .with_help_message("the absolute origin; without it there is no sitemap")
                .prompt(),
        )?,
        None => "https://example.com".to_string(),
    };

    let shape = match args.layout {
        Some(shape) => shape,
        None if ui.interactive() => prompt(
            Select::new("Starting navigation", vec![Shape::Minimal, Shape::Docs])
                .with_help_message("one child kind per level; both shapes obey it")
                .prompt(),
        )?,
        None => Shape::Minimal,
    };

    let git = if args.git {
        true
    } else if args.no_git {
        false
    } else if ui.interactive() {
        prompt(
            Confirm::new("Initialise a git repository?")
                .with_default(true)
                .prompt(),
        )?
    } else {
        false
    };

    Ok(Answers {
        name,
        title,
        description,
        base_url,
        shape,
        git,
    })
}

/// Ctrl-C at a prompt is an answer, not a crash.
fn prompt<T>(result: inquire::error::InquireResult<T>) -> Result<T, Vec<Diagnostic>> {
    result.map_err(|e| match e {
        inquire::InquireError::OperationCanceled | inquire::InquireError::OperationInterrupted => {
            vec![Diagnostic::warning("cancelled", "nothing was written")]
        }
        other => vec![Diagnostic::error("prompt", other.to_string())
            .help("pass the answers as flags instead; every prompt has one")],
    })
}

/// inquire, in the same vocabulary as everything else: the accent on what the
/// reader is acting on, dim on the help, and no second colour.
fn prompt_style() -> RenderConfig<'static> {
    RenderConfig::default_colored()
        .with_prompt_prefix(Styled::new("?").with_fg(Color::LightCyan))
        .with_answered_prompt_prefix(Styled::new("\u{2713}").with_fg(Color::LightGreen))
        .with_highlighted_option_prefix(Styled::new("\u{203a}").with_fg(Color::LightCyan))
        .with_help_message(StyleSheet::new().with_fg(Color::DarkGrey))
        .with_answer(StyleSheet::new().with_fg(Color::LightCyan))
}

/// A cargo package name and a directory name at once, so the rules are the
/// stricter of the two.
fn check_name(name: &str) -> Result<(), Vec<Diagnostic>> {
    let bad = |why: &str| {
        Err(vec![Diagnostic::error("name", format!("{name:?} {why}"))
            .help(
                "lowercase letters, digits, - and _, starting with a letter",
            )])
    };
    if name.is_empty() {
        return bad("is empty");
    }
    if name.contains(['/', '\\', ' ']) {
        return bad("is a path, not a name");
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
    {
        return bad("is not a cargo package name");
    }
    if !name.starts_with(|c: char| c.is_ascii_lowercase()) {
        return bad("does not start with a letter");
    }
    Ok(())
}

/// `my-notes` -> `My Notes`. A starting point, not a decision: it is offered
/// as the default of a prompt the author can overwrite.
fn title_case(name: &str) -> String {
    name.split(['-', '_'])
        .filter(|w| !w.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn io_error(path: &Path, e: &std::io::Error) -> Vec<Diagnostic> {
    vec![Diagnostic::error("write", format!("{e}")).at(path.display().to_string())]
}

// -- the files --------------------------------------------------------------

fn scaffold(a: &Answers) -> Vec<(&'static str, String)> {
    let mut files = vec![
        ("Cargo.toml", cargo_toml(a)),
        ("depoth.yaml", depoth_yaml(a)),
        (".gitignore", GITIGNORE.to_string()),
        ("src/main.rs", MAIN_RS.replace("__TITLE__", &a.title)),
        ("public/robots.txt", robots_txt(a)),
    ];
    match a.shape {
        Shape::Minimal => files.push((
            "content/index.md",
            page_md(&a.title, &a.description, MINIMAL_BODY),
        )),
        Shape::Docs => {
            files.push((
                "content/index.md",
                page_md(&a.title, &a.description, DOCS_BODY),
            ));
            files.push((
                "content/install.md",
                page_md("Install", "Getting the thing onto a machine.", INSTALL_BODY),
            ));
            files.push((
                "content/writing.md",
                page_md("Writing", "How a document becomes a page.", WRITING_BODY),
            ));
        }
    }
    files
}

fn cargo_toml(a: &Answers) -> String {
    format!(
        "[package]\n\
         name = {name:?}\n\
         version = \"0.1.0\"\n\
         edition = \"2021\"\n\
         \n\
         # A site is a binary. Its templates are compiled into it and its content\n\
         # is read at run time, which is why a prose edit costs a re-run and only\n\
         # an edit to src/ costs a compile.\n\
         [dependencies]\n\
         depoth = {{ git = \"https://github.com/DHHwasRight/depoth.git\" }}\n\
         prerender = {{ git = \"https://github.com/DHHwasRight/prerender.git\", branch = \"main\" }}\n\
         rsx = {{ git = \"https://github.com/DHHwasRight/rsx.git\", branch = \"main\" }}\n\
         serde = {{ version = \"1\", features = [\"derive\"] }}\n",
        name = a.name
    )
}

fn depoth_yaml(a: &Answers) -> String {
    let navigation = match a.shape {
        Shape::Minimal => "navigation:\n  pages:\n    - index\n".to_string(),
        Shape::Docs => "\
navigation:
  tabs:
    - tab: Documentation
      groups:
        - group: Getting started
          pages:
            - index
            - install
        - group: Writing
          pages:
            - writing
"
        .to_string(),
    };
    format!(
        "# Everything the site declares about itself.\n\
         #\n\
         # YAML rather than TOML because the navigation tree is recursive and\n\
         # nests five deep in real sites, and because frontmatter is already\n\
         # YAML -- one parser, one set of quoting rules.\n\
         name: {name}\n\
         description: {description}\n\
         # The origin only. A deployment sub-path belongs to `depoth build --base`.\n\
         base_url: {base_url}\n\
         \n\
         content_dir: content\n\
         output_dir: dist\n\
         public_dir: public\n\
         assets_dir: assets\n\
         \n\
         # One tree, outside the content, yielding the sidebar, prev/next and\n\
         # llms.txt. It names slugs only: every title comes from the page's own\n\
         # frontmatter, so a title is never written twice.\n\
         #\n\
         # One rule, and it is the reason a 489-page site stays a readable file:\n\
         # a node carries exactly one kind of child.\n\
         {navigation}",
        name = yaml_scalar(&a.title),
        description = yaml_scalar(&a.description),
        base_url = yaml_scalar(&a.base_url),
    )
}

/// Quotes only what has to be quoted, so the file stays readable.
fn yaml_scalar(text: &str) -> String {
    if text.is_empty() {
        return "\"\"".to_string();
    }
    if text.contains([':', '#', '\'', '"', '\n']) || text.trim() != text {
        format!("{text:?}")
    } else {
        text.to_string()
    }
}

fn robots_txt(a: &Answers) -> String {
    format!(
        "User-agent: *\nAllow: /\n\nSitemap: {}/sitemap.xml\n",
        a.base_url.trim_end_matches('/')
    )
}

fn page_md(title: &str, description: &str, body: &str) -> String {
    let mut front = format!("---\ntitle: {}\n", yaml_scalar(title));
    if !description.is_empty() {
        front.push_str(&format!("description: {}\n", yaml_scalar(description)));
    }
    front.push_str("---\n\n");
    front.push_str(body);
    front
}

const GITIGNORE: &str = "/target\n/dist\n/dist-partial\n";

const MINIMAL_BODY: &str = "\
# Hello

This file is content. It is **not** compiled: the binary reads it at run time,
so changing this sentence costs a re-run and not a compile.

The navigation in `depoth.yaml` names this page by its slug, `index`, and takes
its label from the `title` above. A title is never written in two places.

Run `depoth routes` to see the tree, and `depoth build` to render it.
";

const DOCS_BODY: &str = "\
# Hello

This is the landing page of a documentation site. Two more pages sit beside it,
one per group in `depoth.yaml`.

Run `depoth routes` to see the tree the navigation resolves to, and
`depoth check` to have every rule applied to the built output.
";

const INSTALL_BODY: &str = "\
# Install

Replace this with the real instructions. The page exists so the navigation has
something to point at, and so `depoth check` has a second page to crawl links
between -- which is where the bugs are.
";

const WRITING_BODY: &str = "\
# Writing

Every file under `content/` is a page. Its slug is its path without the
extension, so `content/guides/first.md` is `guides/first`, and
`content/guides/index.md` is `guides`.

A page becomes reachable when the navigation names its slug. Until then
`depoth check` reports it as unreachable rather than quietly publishing it.
";

/// The compiled half of a site.
///
/// Deliberately one file and no `.rsx` templates: a scaffold that someone has
/// to repair before it builds teaches nothing. The HTML here is a starting
/// point to delete, and `rsx` is already a dependency for when it is.
const MAIN_RS: &str = r##"//! __TITLE__, built with depoth.
//!
//! Templates compile; content does not. This file is the compiled half: it
//! reads `content/` at run time, so a prose edit costs a re-run and only an
//! edit here costs a compile.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::ExitCode;
use std::rc::Rc;

use depoth::config::Config;
use depoth::frontmatter;
use depoth::nav::Entry;
use serde::Deserialize;

/// The frontmatter a page may carry. Deliberately few keys: across a real
/// 489-page corpus, seven carried the whole site.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Front {
    title: String,
    #[serde(default)]
    description: Option<String>,
}

/// One loaded document.
struct Doc {
    title: String,
    description: Option<String>,
    html: String,
    source: String,
}

/// Everything the pages need, shared by every route.
struct Data {
    config: Config,
    docs: BTreeMap<String, Doc>,
    entries: Vec<Entry>,
    /// Deployment sub-path, from `depoth build --base`. Empty at the root.
    base: String,
}

fn main() -> ExitCode {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let base = std::env::var("DEPOTH_BASE").unwrap_or_default();
    let base = base.trim_end_matches('/').to_string();

    let config = match Config::load(&root.join("depoth.yaml")) {
        Ok(config) => config,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    let docs = match load(&root.join(&config.content_dir)) {
        Ok(docs) => docs,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    // Navigation names slugs only, and every title comes from the page itself,
    // so the tree cannot be resolved until the content is loaded. A slug with
    // no file fails the build rather than leaving a blank sidebar entry.
    let entries = config.navigation.flatten();
    for entry in &entries {
        if !docs.contains_key(&entry.slug) {
            eprintln!(
                "error: {}: named in the navigation, but no file was found",
                entry.slug
            );
            return ExitCode::FAILURE;
        }
    }

    let data = Rc::new(Data {
        config,
        docs,
        entries,
        base,
    });

    let mut site = prerender::Site::new().static_dir(root.join(&data.config.public_dir));

    // Every page is also published as the markdown it was written in: the
    // cheapest high-value thing a content site can do, because whatever reads
    // it never has to parse the HTML back.
    for entry in data.entries.iter() {
        if let Some(doc) = data.docs.get(&entry.slug) {
            site = site.static_file(format!("/{}.md", entry.slug), doc.source.clone());
        }
    }

    for entry in data.entries.iter() {
        let data = Rc::clone(&data);
        let slug = entry.slug.clone();
        site = site.route(&route_of(&slug), move || Ok(page(&data, &slug)));
    }

    prerender::run(site)
}

/// Reads every `.md` under `root`, keyed by the slug the navigation names.
fn load(root: &Path) -> Result<BTreeMap<String, Doc>, String> {
    let mut docs = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let listing = std::fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        for entry in listing {
            let path = entry.map_err(|e| e.to_string())?.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().is_none_or(|e| e != "md") {
                continue;
            }
            let source =
                std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            let split = frontmatter::split(&source);
            let front: Front = frontmatter::parse(&split, &path).map_err(|e| e.to_string())?;
            let rendered = depoth::markdown::render(split.body);

            let rel = path
                .strip_prefix(root)
                .expect("under root")
                .with_extension("");
            let slug = rel.to_string_lossy().replace('\\', "/");
            // `content/guides/index.md` is `guides`, not `guides/index`.
            let slug = slug.strip_suffix("/index").unwrap_or(&slug).to_string();

            docs.insert(
                slug,
                Doc {
                    title: front.title,
                    description: front.description,
                    html: rendered.html,
                    source,
                },
            );
        }
    }
    Ok(docs)
}

/// Where a slug is written. `index` is the root.
fn route_of(slug: &str) -> String {
    if slug == "index" {
        "/".to_string()
    } else {
        format!("/{slug}")
    }
}

/// What a link to a slug says: the route, behind any deployment sub-path.
fn url_of(base: &str, slug: &str) -> String {
    let route = route_of(slug);
    if base.is_empty() {
        route
    } else {
        format!("{base}{}", route.trim_end_matches('/'))
    }
}

/// One whole document, head and all.
///
/// There is no shared layout on purpose: every page carrying its own `<head>`
/// is what makes it work with JavaScript off, which is a test rather than a
/// guideline. Replace this with `.rsx` templates when it stops being enough.
fn page(data: &Data, slug: &str) -> String {
    let doc = data
        .docs
        .get(slug)
        .expect("checked before the routes were built");

    let mut nav = String::new();
    for entry in &data.entries {
        let other = &data.docs[&entry.slug];
        let current = if entry.slug == slug {
            " aria-current=\"page\""
        } else {
            ""
        };
        nav.push_str(&format!(
            "<li><a href=\"{}\"{current}>{}</a></li>",
            url_of(&data.base, &entry.slug),
            escape(&other.title)
        ));
    }

    // The author writes `/install`; where the site is deployed is not their
    // problem, so the sub-path is added here and in one place.
    let body = depoth::markdown::rebase(&doc.html, &data.base);
    let description = doc.description.as_deref().unwrap_or_default();

    format!(
        concat!(
            "<!doctype html>\n",
            "<html lang=\"en\">\n",
            "<head>\n",
            "<meta charset=\"utf-8\">\n",
            "<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n",
            "<title>{title} \u{2014} {site}</title>\n",
            "<meta name=\"description\" content=\"{description}\">\n",
            "</head>\n",
            "<body>\n",
            "<header><a href=\"{home}\">{site}</a><nav><ul>{nav}</ul></nav></header>\n",
            "<main>{body}</main>\n",
            "<footer><a href=\"{markdown}\">Read this page as Markdown</a></footer>\n",
            "</body>\n",
            "</html>\n",
        ),
        title = escape(&doc.title),
        site = escape(&data.config.name),
        description = escape(description),
        home = url_of(&data.base, "index"),
        nav = nav,
        body = body,
        markdown = format!("{}/{slug}.md", data.base),
    )
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
"##;

#[cfg(test)]
mod tests {
    use super::*;

    fn answers(shape: Shape) -> Answers {
        Answers {
            name: "my-notes".into(),
            title: "My Notes".into(),
            description: "Things worth keeping.".into(),
            base_url: "https://example.com".into(),
            shape,
            git: false,
        }
    }

    #[test]
    fn a_name_is_a_cargo_package_name() {
        assert!(check_name("my-notes").is_ok());
        assert!(check_name("notes_2").is_ok());
        assert!(check_name("").is_err());
        assert!(check_name("My Notes").is_err());
        assert!(check_name("../escape").is_err());
        assert!(check_name("2notes").is_err());
    }

    #[test]
    fn the_default_title_is_a_starting_point() {
        assert_eq!(title_case("my-notes"), "My Notes");
        assert_eq!(title_case("depoth"), "Depoth");
        assert_eq!(title_case("a_b-c"), "A B C");
    }

    #[test]
    fn yaml_is_quoted_only_where_it_has_to_be() {
        assert_eq!(yaml_scalar("Depoth"), "Depoth");
        assert_eq!(yaml_scalar(""), "\"\"");
        assert_eq!(
            yaml_scalar("https://example.com"),
            "\"https://example.com\""
        );
    }

    #[test]
    fn the_scaffold_declares_what_it_writes() {
        let files = scaffold(&answers(Shape::Docs));
        let names: Vec<&str> = files.iter().map(|(p, _)| *p).collect();
        assert_eq!(
            names,
            [
                "Cargo.toml",
                "depoth.yaml",
                ".gitignore",
                "src/main.rs",
                "public/robots.txt",
                "content/index.md",
                "content/install.md",
                "content/writing.md",
            ]
        );
    }

    #[test]
    fn the_generated_config_parses_and_validates() {
        // The scaffold is checked against the real parser rather than by
        // eye: a site that does not load is worse than no scaffold.
        for shape in [Shape::Minimal, Shape::Docs] {
            let a = answers(shape);
            let yaml = depoth_yaml(&a);
            let config = depoth::config::Config::parse(&yaml, Path::new("depoth.yaml"))
                .unwrap_or_else(|e| panic!("{shape:?}: {e}\n{yaml}"));
            assert_eq!(config.name, "My Notes");
            // Every slug the navigation names must be a file the scaffold
            // writes, or the generated binary refuses to build the site.
            let files = scaffold(&a);
            for entry in config.navigation.flatten() {
                let wanted = format!("content/{}.md", entry.slug);
                assert!(
                    files.iter().any(|(p, _)| *p == wanted),
                    "{shape:?}: navigation names {} but nothing writes {wanted}",
                    entry.slug
                );
            }
        }
    }

    #[test]
    fn every_scaffolded_page_has_the_frontmatter_the_loader_demands() {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Front {
            #[allow(dead_code)]
            title: String,
            #[serde(default)]
            #[allow(dead_code)]
            description: Option<String>,
        }
        for (path, contents) in scaffold(&answers(Shape::Docs)) {
            if !path.starts_with("content/") {
                continue;
            }
            let split = depoth::frontmatter::split(&contents);
            depoth::frontmatter::parse::<Front>(&split, Path::new(path))
                .unwrap_or_else(|e| panic!("{path}: {e}"));
        }
    }
}
