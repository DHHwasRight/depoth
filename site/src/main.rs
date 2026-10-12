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
        .static_file("/llms.txt", llms_txt(&config, &docs, &entries));

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

    prerender::run(site)
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
            let base = config.base_url.trim_end_matches('/');
            out.push_str(&format!("- [{}]({}{})", doc.title, base, doc.url));
            if let Some(description) = &doc.description {
                out.push_str(&format!(": {description}"));
            }
            out.push('\n');
        }
    }
    out
}
