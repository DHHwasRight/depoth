//! Assertions over a built site.
//!
//! The output of a static site generator is HTML on disk, so almost every
//! claim worth making about it can be checked by reading that HTML. This crate
//! does that, in process, with no browser.
//!
//! The rules here are not invented. Each one is a bug that reached a human in
//! an earlier generation of this project, and every one of them was **between**
//! pages rather than inside one: dead links across a rename, a doubled navbar
//! when a 404 was served to a partial request, a page that printed its own
//! JavaScript. A test that loads one page and asserts about one page sees none
//! of them.
//!
//! What still needs a browser -- layout at a breakpoint, a swap actually
//! happening, a visual baseline -- stays in Playwright. Everything else is
//! here, because it runs in milliseconds and costs no dependencies at runtime.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use scraper::{Html, Selector};

/// One built page.
pub struct Page {
    /// URL path, e.g. `/guide/install`.
    pub route: String,
    pub path: PathBuf,
    pub source: String,
    dom: Html,
}

impl Page {
    pub fn parse(route: String, path: PathBuf, source: String) -> Self {
        let dom = Html::parse_document(&source);
        Self {
            route,
            path,
            source,
            dom,
        }
    }

    fn selector(sel: &str) -> Selector {
        Selector::parse(sel).unwrap_or_else(|e| panic!("bad selector {sel:?}: {e:?}"))
    }

    /// How many elements match. The workhorse: most chrome bugs are a count
    /// that should be one and is two.
    pub fn count(&self, sel: &str) -> usize {
        self.dom.select(&Self::selector(sel)).count()
    }

    /// Collapsed text of the first match.
    pub fn text(&self, sel: &str) -> Option<String> {
        self.dom.select(&Self::selector(sel)).next().map(|el| {
            el.text()
                .collect::<String>()
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
        })
    }

    /// Every value of `attr` on elements matching `sel`.
    pub fn attrs(&self, sel: &str, attr: &str) -> Vec<String> {
        self.dom
            .select(&Self::selector(sel))
            .filter_map(|el| el.value().attr(attr))
            .map(str::to_string)
            .collect()
    }

    /// Heading levels in document order, e.g. `[1, 2, 2, 3]`.
    pub fn heading_levels(&self) -> Vec<u8> {
        self.dom
            .select(&Self::selector("h1, h2, h3, h4, h5, h6"))
            .filter_map(|el| el.value().name().strip_prefix('h')?.parse().ok())
            .collect()
    }

    pub fn has_doctype(&self) -> bool {
        self.source
            .trim_start()
            .to_ascii_lowercase()
            .starts_with("<!doctype html")
    }
}

/// A rule that did not hold.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Finding {
    pub route: String,
    pub rule: &'static str,
    pub detail: String,
}

impl std::fmt::Display for Finding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: [{}] {}", self.route, self.rule, self.detail)
    }
}

/// What "correct" means for this particular site.
pub struct Policy {
    /// Selectors that must match exactly once on every page. This is the
    /// doubled-navbar rule, and it is the cheapest high-value check there is.
    pub unique: Vec<String>,
    /// Routes exempt from the complete-document rule.
    pub allow_partial: Vec<String>,
    /// Link prefixes to leave alone, e.g. a search endpoint.
    pub ignore_links: Vec<String>,
    /// Sub-path the site is deployed under, e.g. `/depoth` on GitHub Pages.
    ///
    /// Without this the harness checks a different artefact from the one that
    /// ships, which is how a publish mode quietly stops being tested.
    pub base: String,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            unique: vec!["body > header".into(), "main".into()],
            allow_partial: Vec::new(),
            ignore_links: Vec::new(),
            base: String::new(),
        }
    }
}

/// A built site, loaded into memory.
pub struct Site {
    pub root: PathBuf,
    pub pages: Vec<Page>,
    /// Every file under the output root, as a URL path.
    files: BTreeSet<String>,
}

impl Site {
    /// Reads every `.html` file under `root`, plus an index of every other
    /// file so that asset links can be resolved.
    pub fn open(root: impl AsRef<Path>) -> std::io::Result<Self> {
        let root = root.as_ref().to_path_buf();
        let mut pages = Vec::new();
        let mut files = BTreeSet::new();
        let mut stack = vec![root.clone()];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir)? {
                let path = entry?.path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                let rel = path.strip_prefix(&root).expect("under root");
                let url = format!("/{}", rel.to_string_lossy().replace('\\', "/"));
                files.insert(url.clone());
                if path.extension().is_some_and(|e| e == "html") {
                    let source = std::fs::read_to_string(&path)?;
                    pages.push(Page::parse(route_of(&url), path, source));
                }
            }
        }
        pages.sort_by(|a, b| a.route.cmp(&b.route));
        Ok(Self { root, pages, files })
    }

    pub fn page(&self, route: &str) -> Option<&Page> {
        self.pages.iter().find(|p| p.route == route)
    }

    /// Every rule, against every page. Returns findings rather than panicking
    /// so one run reports every problem -- a harness that stops at the first
    /// turns one build into ten.
    pub fn check(&self, policy: &Policy) -> Vec<Finding> {
        let mut out = Vec::new();
        for page in &self.pages {
            let partial_ok = policy.allow_partial.iter().any(|r| r == &page.route);
            if !partial_ok {
                self.check_document(page, &mut out);
                for sel in &policy.unique {
                    let n = page.count(sel);
                    if n != 1 {
                        out.push(self.finding(
                            page,
                            "duplicate-chrome",
                            format!("{sel:?} matches {n} times, expected exactly 1"),
                        ));
                    }
                }
            }
            self.check_links(page, policy, &mut out);
            self.check_headings(page, &mut out);
        }
        out.sort();
        out
    }

    fn finding(&self, page: &Page, rule: &'static str, detail: String) -> Finding {
        Finding {
            route: page.route.clone(),
            rule,
            detail,
        }
    }

    fn check_document(&self, page: &Page, out: &mut Vec<Finding>) {
        if !page.has_doctype() {
            out.push(self.finding(
                page,
                "incomplete-document",
                "no <!doctype html>; a partial was written where a page belongs".into(),
            ));
        }
        if page.count("html[lang]") == 0 {
            out.push(self.finding(page, "incomplete-document", "<html> has no lang".into()));
        }
        match page.text("title") {
            None => out.push(self.finding(page, "incomplete-document", "no <title>".into())),
            Some(t) if t.trim().is_empty() => {
                out.push(self.finding(page, "incomplete-document", "<title> is empty".into()))
            }
            Some(_) => {}
        }
        let h1 = page.count("h1");
        if h1 != 1 {
            out.push(self.finding(
                page,
                "headings",
                format!("{h1} <h1> elements, expected exactly 1"),
            ));
        }
    }

    fn check_links(&self, page: &Page, policy: &Policy, out: &mut Vec<Finding>) {
        let mut targets: Vec<(&'static str, String)> = Vec::new();
        targets.extend(
            page.attrs("a[href]", "href")
                .into_iter()
                .map(|h| ("dead-link", h)),
        );
        for sel in ["img[src]", "script[src]", "link[href]", "source[src]"] {
            let attr = if sel.contains("href") { "href" } else { "src" };
            targets.extend(page.attrs(sel, attr).into_iter().map(|h| ("dead-asset", h)));
        }
        for (rule, raw) in targets {
            let Some(url) = self.resolve(&page.route, &raw) else {
                continue;
            };
            // A deployed sub-path is part of the URL and not of the file tree,
            // so it comes off before anything is looked up. An absolute link
            // that is missing it is a finding: on the host it would leave the
            // site.
            let url = if policy.base.is_empty() {
                url
            } else if let Some(rest) = url.strip_prefix(&policy.base) {
                if rest.is_empty() {
                    "/".to_string()
                } else {
                    rest.to_string()
                }
            } else {
                out.push(self.finding(
                    page,
                    rule,
                    format!(
                        "{raw:?} is absolute but omits the base path {:?}",
                        policy.base
                    ),
                ));
                continue;
            };
            if policy.ignore_links.iter().any(|p| url.starts_with(p)) {
                continue;
            }
            if rule == "dead-link" && url.contains("_fragment") {
                out.push(self.finding(
                    page,
                    "links-into-a-partial",
                    format!("{raw:?} -- a fragment is fetched, never navigated to"),
                ));
                continue;
            }
            if !self.exists(&url) {
                out.push(self.finding(
                    page,
                    rule,
                    format!("{raw:?} resolves to {url}, which was not built"),
                ));
            }
        }
    }

    fn check_headings(&self, page: &Page, out: &mut Vec<Finding>) {
        let levels = page.heading_levels();
        for pair in levels.windows(2) {
            if pair[1] > pair[0] + 1 {
                out.push(self.finding(
                    page,
                    "headings",
                    format!("h{} follows h{}, skipping a level", pair[1], pair[0]),
                ));
                break;
            }
        }
    }

    /// Turns an href into an absolute site path, or `None` when it points
    /// somewhere this crate has no business checking.
    fn resolve(&self, from: &str, raw: &str) -> Option<String> {
        let raw = raw.split('#').next()?.split('?').next()?;
        if raw.is_empty() || raw.starts_with("//") || raw.contains("://") {
            return None;
        }
        if raw.starts_with("mailto:") || raw.starts_with("tel:") || raw.starts_with("data:") {
            return None;
        }
        if raw.starts_with('/') {
            return Some(raw.to_string());
        }
        // Relative: resolve against the page's own directory.
        let base = from.rsplit_once('/').map(|(dir, _)| dir).unwrap_or("");
        let mut parts: Vec<&str> = base.split('/').filter(|s| !s.is_empty()).collect();
        for segment in raw.split('/') {
            match segment {
                "" | "." => {}
                ".." => {
                    parts.pop();
                }
                s => parts.push(s),
            }
        }
        Some(format!("/{}", parts.join("/")))
    }

    fn exists(&self, url: &str) -> bool {
        let trimmed = url.trim_end_matches('/');
        self.files.contains(url)
            || self.files.contains(&format!("{trimmed}/index.html"))
            || (url == "/" && self.files.contains("/index.html"))
    }

    /// Pages grouped by `<title>`, for spotting a template that stamped one
    /// title onto everything.
    pub fn titles(&self) -> BTreeMap<String, Vec<&str>> {
        let mut out: BTreeMap<String, Vec<&str>> = BTreeMap::new();
        for page in &self.pages {
            let title = page.text("title").unwrap_or_default();
            out.entry(title).or_default().push(&page.route);
        }
        out
    }
}

/// `/guide/install/index.html` -> `/guide/install`, `/index.html` -> `/`.
fn route_of(url: &str) -> String {
    if let Some(dir) = url.strip_suffix("/index.html") {
        if dir.is_empty() {
            "/".to_string()
        } else {
            dir.to_string()
        }
    } else {
        url.trim_end_matches(".html").to_string()
    }
}

/// Panics with every finding at once. The shape a test wants.
pub fn assert_clean(site: &Site, policy: &Policy) {
    let findings = site.check(policy);
    if !findings.is_empty() {
        let list = findings
            .iter()
            .map(|f| format!("  {f}"))
            .collect::<Vec<_>>()
            .join("\n");
        panic!(
            "{} finding(s) in {}:\n{list}",
            findings.len(),
            site.root.display()
        );
    }
}
