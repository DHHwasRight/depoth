//! Every rule, against a fixture carrying exactly the bug it looks for.
//!
//! A check that cannot fail is worse than no check, because it reads as
//! coverage. Each test here plants one defect and asserts that one finding
//! comes back.

use std::path::{Path, PathBuf};

use depoth_test::{Policy, Site};

/// Writes a site under `target/`, so fixtures need no temp-file dependency
/// and survive for inspection when a test fails.
fn build(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-sites")
        .join(name);
    let _ = std::fs::remove_dir_all(&root);
    for (path, body) in files {
        let full = root.join(path);
        std::fs::create_dir_all(full.parent().expect("has a parent")).expect("mkdir");
        std::fs::write(&full, body).expect("write");
    }
    root
}

fn page(title: &str, body: &str) -> String {
    format!(
        "<!doctype html>\n<html lang=\"en\"><head><title>{title}</title></head>\
         <body><header>nav</header><main><h1>{title}</h1>{body}</main></body></html>"
    )
}

fn findings(root: &Path, policy: &Policy) -> Vec<String> {
    Site::open(root)
        .expect("open")
        .check(policy)
        .iter()
        .map(ToString::to_string)
        .collect()
}

#[test]
fn a_correct_site_reports_nothing() {
    let root = build(
        "clean",
        &[
            (
                "index.html",
                &page("Home", "<a href=\"/guide/install\">Install</a>"),
            ),
            (
                "guide/install/index.html",
                &page("Install", "<a href=\"/\">Home</a><h2>Step</h2>"),
            ),
        ],
    );
    assert_eq!(findings(&root, &Policy::default()), Vec::<String>::new());
}

#[test]
fn a_dead_internal_link_is_found() {
    // The commonest failure by a distance: a page is renamed and the links to
    // it are not. Invisible to any test that loads one page.
    let root = build(
        "dead-link",
        &[(
            "index.html",
            &page("Home", "<a href=\"/guide/moved\">Gone</a>"),
        )],
    );
    let found = findings(&root, &Policy::default());
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("dead-link"), "{found:?}");
    assert!(found[0].contains("/guide/moved"), "{found:?}");
}

#[test]
fn a_relative_link_resolves_against_its_own_page() {
    let root = build(
        "relative",
        &[
            (
                "index.html",
                &page("Home", "<a href=\"guide/install\">x</a>"),
            ),
            (
                "guide/install/index.html",
                &page("Install", "<a href=\"../..\">up</a>"),
            ),
        ],
    );
    assert_eq!(findings(&root, &Policy::default()), Vec::<String>::new());
}

#[test]
fn duplicated_chrome_is_found() {
    // A 404 page served to a partial request once spliced a second navbar
    // into a live page. A count that should be one and is two.
    let root = build("doubled", &[(
        "index.html",
        "<!doctype html><html lang=\"en\"><head><title>Home</title></head>\
         <body><header>nav</header><header>nav again</header><main><h1>Home</h1></main></body></html>",
    )]);
    let found = findings(&root, &Policy::default());
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("duplicate-chrome"), "{found:?}");
    assert!(found[0].contains("matches 2 times"), "{found:?}");
}

#[test]
fn a_partial_written_where_a_page_belongs_is_found() {
    // The failure mode that produces the doubled navbar upstream of it: a
    // fragment ends up at a route a browser can navigate to.
    let root = build(
        "partial",
        &[("about/index.html", "<main><h1>About</h1></main>")],
    );
    let found = findings(&root, &Policy::default());
    assert!(
        found
            .iter()
            .any(|f| f.contains("incomplete-document") && f.contains("doctype")),
        "{found:?}"
    );
}

#[test]
fn a_link_into_a_fragment_is_found() {
    // A fragment is fetched, never navigated to. A link to one hands a reader
    // a page with no shell around it.
    let root = build(
        "frag-link",
        &[
            (
                "index.html",
                &page("Home", "<a href=\"/guide/_fragment.html\">x</a>"),
            ),
            ("guide/_fragment.html", "<main>body</main>"),
        ],
    );
    let found = findings(&root, &Policy::default());
    assert!(
        found.iter().any(|f| f.contains("links-into-a-partial")),
        "{found:?}"
    );
}

#[test]
fn a_missing_asset_is_found() {
    let root = build(
        "asset",
        &[(
            "index.html",
            &page("Home", "<img src=\"/img/missing.png\" alt=\"x\">"),
        )],
    );
    let found = findings(&root, &Policy::default());
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("dead-asset"), "{found:?}");
}

#[test]
fn a_skipped_heading_level_is_found() {
    let root = build(
        "headings",
        &[("index.html", &page("Home", "<h3>Too deep</h3>"))],
    );
    let found = findings(&root, &Policy::default());
    assert!(
        found.iter().any(|f| f.contains("skipping a level")),
        "{found:?}"
    );
}

#[test]
fn an_external_link_is_left_alone() {
    let root = build(
        "external",
        &[(
            "index.html",
            &page(
                "Home",
                "<a href=\"https://example.com/x\">x</a>\
                       <a href=\"mailto:a@b.c\">mail</a><a href=\"#top\">top</a>",
            ),
        )],
    );
    assert_eq!(findings(&root, &Policy::default()), Vec::<String>::new());
}

#[test]
fn one_run_reports_every_problem() {
    // A harness that stops at the first finding turns one build into ten.
    let root = build(
        "many",
        &[(
            "index.html",
            "<!doctype html><html lang=\"en\"><head><title></title></head>\
         <body><main><a href=\"/gone\">x</a><h1>a</h1><h1>b</h1><h3>c</h3></main></body></html>",
        )],
    );
    let found = findings(&root, &Policy::default());
    let rules: Vec<_> = [
        "incomplete-document",
        "dead-link",
        "headings",
        "duplicate-chrome",
    ]
    .into_iter()
    .filter(|r| found.iter().any(|f| f.contains(r)))
    .collect();
    assert_eq!(
        rules,
        [
            "incomplete-document",
            "dead-link",
            "headings",
            "duplicate-chrome"
        ],
        "every rule should report in one pass, got {found:?}"
    );
}

#[test]
fn titles_group_by_value_so_a_stamped_template_shows_up() {
    let root = build(
        "titles",
        &[
            ("index.html", &page("Depoth", "<a href=\"/a\">a</a>")),
            ("a/index.html", &page("Depoth", "")),
        ],
    );
    let site = Site::open(&root).expect("open");
    let titles = site.titles();
    assert_eq!(titles["Depoth"], ["/", "/a"], "both pages share one title");
}

#[test]
fn a_base_path_is_stripped_before_lookup() {
    // The site ships under a sub-path on GitHub Pages, so its links carry one
    // and its files do not. Checking the wrong one of those is how a publish
    // mode quietly stops being tested.
    let root = build(
        "based",
        &[
            (
                "index.html",
                &page("Home", "<a href=\"/depoth/docs/\">Docs</a>"),
            ),
            (
                "docs/index.html",
                &page("Docs", "<a href=\"/depoth/\">Home</a>"),
            ),
        ],
    );
    let policy = Policy {
        base: "/depoth".into(),
        ..Policy::default()
    };
    assert_eq!(findings(&root, &policy), Vec::<String>::new());
}

#[test]
fn an_absolute_link_missing_the_base_path_is_found() {
    // Without this the rule above would pass by doing nothing, which is how
    // the first version of it shipped.
    let root = build(
        "unbased",
        &[
            ("index.html", &page("Home", "<a href=\"/docs/\">Docs</a>")),
            ("docs/index.html", &page("Docs", "")),
        ],
    );
    let policy = Policy {
        base: "/depoth".into(),
        ..Policy::default()
    };
    let found = findings(&root, &policy);
    assert!(
        found.iter().any(|f| f.contains("omits the base path")),
        "a link without the prefix must be reported, got {found:?}"
    );
}
