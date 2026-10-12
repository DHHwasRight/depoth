//! depoth's own documentation, built with depoth.
//!
//! The site is the end-to-end test: it is rebuilt on every change, and its
//! output is checked by the same harness a user's site would use.

mod content;
mod data;
mod mdx;
mod views {
    rsx::load_dir!("src/views");
}

use std::path::Path;
use std::process::ExitCode;
use std::rc::Rc;

use depoth::config::Config;
use depoth::feeds;
use rsx::Template;

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| {
        eprintln!("error: {}: {e}", path.display());
        eprintln!("hint: run `npm run css` in site/ first");
        std::process::exit(1);
    })
}

fn main() -> ExitCode {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let config = match Config::load(&root.join("depoth.yaml")) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    let docs = match content::load(&root.join(&config.content_dir)) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    let entries = config.navigation.flatten();
    if let Err(problems) = content::check_navigation(&docs, &entries) {
        for p in problems {
            eprintln!("error: {p}");
        }
        return ExitCode::FAILURE;
    }

    let config = Rc::new(config);
    let docs = Rc::new(docs);
    let entries = Rc::new(entries);

    let mut binary: Vec<depoth::images::Asset> = Vec::new();
    let mut site = prerender::Site::new()
        // Only the bundled stylesheet is published. Everything else under
        // assets/css is an input -- the theme, the two component sheets, the
        // Tailwind entry -- and publishing an input ships bytes no page asks
        // for and invites someone to link the wrong one.
        .static_file(
            "/assets/css/site.css",
            read(&root.join("assets/css/site.css")),
        )
        .static_dir(root.join("public"))
        // GitHub Pages runs Jekyll unless told otherwise, and Jekyll drops
        // every path beginning with an underscore. Nothing here starts with
        // one today, which is exactly why this is easy to forget later.
        .static_file("/.nojekyll", "")
        .static_file("/llms.txt", llms_txt(&config, &docs, &entries))
        .static_file(
            "/robots.txt",
            feeds::robots(&config.base_url, "/sitemap.xml"),
        );

    // A sitemap of relative URLs is invalid rather than degraded, so it is
    // omitted entirely when there is no origin to resolve against.
    let feed_pages: Vec<feeds::Page> = entries
        .iter()
        .filter_map(|e| docs.get(&e.slug))
        .map(|d| feeds::Page {
            url: d.url.clone(),
            title: d.title.clone(),
            description: d.description.clone(),
            updated: None,
            hidden: false,
        })
        .collect();
    if let Some(xml) = feeds::sitemap(&config.base_url, &feed_pages) {
        site = site.static_file("/sitemap.xml", xml);
    }

    // Files that live beside the documents that reference them. Without this
    // a screenshot next to its page is silently dropped.
    for asset in content::colocated_assets(&root.join(&config.content_dir)) {
        match std::fs::read(&asset.source) {
            Ok(bytes) => match String::from_utf8(bytes) {
                Ok(text) => site = site.static_file(asset.url.clone(), text),
                // Binary assets are copied rather than written as text; the
                // generator's static_file takes a String, so a directory copy
                // is the right tool and the build does it below.
                Err(_) => binary.push(asset),
            },
            Err(e) => eprintln!("warning: {}: {e}", asset.source.display()),
        }
    }

    // No prerender layout: each template emits a whole document, head and
    // all. That is what makes every page work with JavaScript off, and it is
    // what the harness checks. Partial navigation would need a dp-layout
    // marker inside the templates; this site is published without partials.

    {
        let (c, d, e) = (Rc::clone(&config), Rc::clone(&docs), Rc::clone(&entries));
        site = site.route("/", move || {
            let chrome = content::chrome_for(&c, &d, &e, "docs");
            views::landing::Landing { chrome: &chrome }.render_to_string()
        });
    }

    // Every page is also published as the markdown it was written in.
    //
    // The cheapest high-value thing a documentation site can do: an agent or a
    // reader that finds `<url>.md` never has to parse the HTML to recover the
    // text, and "copy this page" becomes a link rather than a feature.
    for entry in entries.iter() {
        if let Some(doc) = docs.get(&entry.slug) {
            site = site.static_file(format!("/{}.md", entry.slug), doc.source.clone());
        }
    }

    for entry in entries.iter() {
        let (c, d, e) = (Rc::clone(&config), Rc::clone(&docs), Rc::clone(&entries));
        let slug = entry.slug.clone();
        let url = format!("/{slug}");
        site = site.route(&url, move || {
            let doc = d.get(&slug).expect("checked before the routes were built");
            let chrome = content::chrome_for(&c, &d, &e, &slug);
            views::docs::DocsPage {
                chrome: &chrome,
                doc,
            }
            .render_to_string()
        });
    }

    let code = prerender::run(site);
    // Binary co-located assets, copied after the tree is written.
    for asset in &binary {
        let target = root.join("dist").join(asset.url.trim_start_matches('/'));
        if let Some(parent) = target.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = std::fs::copy(&asset.source, &target) {
            eprintln!("warning: copying {}: {e}", asset.source.display());
        }
    }
    code
}

/// An index of the site as plain text, in navigation order.
///
/// The cheapest high-value thing a documentation site can publish: an agent
/// that finds this never has to parse the HTML.
fn llms_txt(
    config: &Config,
    docs: &std::collections::BTreeMap<String, data::Doc>,
    entries: &[depoth::nav::Entry],
) -> String {
    let mut out = format!("# {}\n\n", config.name);
    if let Some(description) = &config.description {
        out.push_str(&format!("> {description}\n\n"));
    }
    let mut tab = None;
    for entry in entries {
        if entry.tab != tab {
            tab = entry.tab.clone();
            if let Some(name) = &tab {
                out.push_str(&format!("## {name}\n\n"));
            }
        }
        if let Some(doc) = docs.get(&entry.slug) {
            // doc.url already carries any deployment sub-path, so the origin
            // is all that is wanted here.
            let base = config.base_url.trim_end_matches('/');
            // Point at the markdown, not the page: whatever reads this file
            // wants the text.
            out.push_str(&format!("- [{}]({}{}.md)", doc.title, base, doc.url));
            if let Some(description) = &doc.description {
                out.push_str(&format!(": {description}"));
            }
            out.push('\n');
        }
    }
    out
}
