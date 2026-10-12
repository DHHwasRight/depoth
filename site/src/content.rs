//! Loading the content tree and resolving it against the navigation.
//!
//! Order matters here and is forced by the design: navigation names slugs
//! only, and every title comes from the page's own frontmatter, so nothing
//! about the sidebar can be known until all the content is loaded.

use std::collections::BTreeMap;
use std::path::Path;

use depoth::config::Config;
use depoth::frontmatter;
use depoth::nav::{self, Entry};
use serde::Deserialize;

use crate::data::{Chrome, Doc, Heading, Link, SidebarItem, Tab};

/// The frontmatter keys a page may carry. Deliberately few: across a real
/// 489-page corpus, seven keys carried the whole site.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Front {
    title: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    sidebar_title: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Front(#[from] frontmatter::FrontError),
    #[error("{slug}: named in the navigation, but {path} does not exist")]
    Missing { slug: String, path: String },
}

/// The prefix every absolute URL carries, empty unless the site is deployed
/// under a sub-path. Set at compile time by `build.rs`.
pub fn base_path() -> &'static str {
    env!("DEPOTH_BASE")
}

/// Reads every `.md` under `root`, keyed by slug.
pub fn load(root: &Path) -> Result<BTreeMap<String, Doc>, LoadError> {
    let mut docs = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        if !dir.exists() {
            continue;
        }
        for entry in std::fs::read_dir(&dir)? {
            let path = entry?.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().is_none_or(|e| e != "md") {
                continue;
            }
            let source = std::fs::read_to_string(&path)?;
            let split = frontmatter::split(&source);
            let front: Front = frontmatter::parse(&split, &path)?;
            let rendered = depoth::markdown::render(split.body);

            let rel = path
                .strip_prefix(root)
                .expect("under root")
                .with_extension("");
            let slug = rel.to_string_lossy().replace('\\', "/");
            let slug = slug.strip_suffix("/index").unwrap_or(&slug).to_string();
            docs.insert(
                slug.clone(),
                Doc {
                    url: format!("{}/{slug}", base_path()),
                    slug,
                    title: front.title,
                    description: front.description,
                    sidebar_title: front.sidebar_title,
                    // The author writes /docs/navigation; where the site is
                    // deployed is not their problem.
                    html: depoth::markdown::rebase(&rendered.html, base_path()),
                    headings: rendered
                        .headings
                        .into_iter()
                        .map(|h| Heading {
                            level: h.level,
                            id: h.id,
                            text: h.text,
                        })
                        .collect(),
                },
            );
        }
    }
    Ok(docs)
}

/// Everything the layout needs for one page.
pub fn chrome_for(
    config: &Config,
    docs: &BTreeMap<String, Doc>,
    entries: &[Entry],
    current: &str,
) -> Chrome {
    let here = entries.iter().find(|e| e.slug == current);
    let tab_here = here.and_then(|e| e.tab.clone());

    let tabs = config
        .navigation
        .tabs
        .iter()
        .flatten()
        .map(|t| Tab {
            label: t.tab.clone(),
            url: match &t.href {
                Some(href) => href.clone(),
                None => entries
                    .iter()
                    .find(|e| e.tab.as_deref() == Some(t.tab.as_str()))
                    .map(|e| format!("{}/{}", base_path(), e.slug))
                    .unwrap_or_else(|| format!("{}/", base_path())),
            },
            current: Some(&t.tab) == tab_here.as_ref(),
            external: t.href.is_some(),
        })
        .collect();

    // The sidebar shows only the current tab, which is what makes a tab a tab.
    let mut sidebar = Vec::new();
    let mut last_trail: Vec<String> = Vec::new();
    for entry in entries.iter().filter(|e| e.tab == tab_here) {
        for (depth, group) in entry.trail.iter().enumerate() {
            if last_trail.get(depth) != Some(group) {
                sidebar.push(SidebarItem::Group {
                    label: group.clone(),
                    depth,
                });
            }
        }
        last_trail = entry.trail.clone();
        if let Some(doc) = docs.get(&entry.slug) {
            sidebar.push(SidebarItem::Link {
                label: doc.nav_label().to_string(),
                url: doc.url.clone(),
                current: entry.slug == current,
                depth: entry.trail.len(),
            });
        }
    }

    let (prev, next) = nav::neighbours(entries, current);
    let link = |e: &Entry| {
        docs.get(&e.slug).map(|d| Link {
            label: d.title.clone(),
            url: d.url.clone(),
        })
    };

    Chrome {
        site_name: config.name.clone(),
        tabs,
        sidebar,
        trail: here.map(|e| e.trail.clone()).unwrap_or_default(),
        prev: prev.and_then(link),
        next: next.and_then(link),
        anchors: config
            .navigation
            .anchors
            .iter()
            .map(|a| Link {
                label: a.anchor.clone(),
                url: a.href.clone(),
            })
            .collect(),
    }
}

/// Fails the build when the navigation names a page that was never written.
pub fn check_navigation(
    docs: &BTreeMap<String, Doc>,
    entries: &[Entry],
) -> Result<(), Vec<LoadError>> {
    let missing: Vec<_> = entries
        .iter()
        .filter(|e| !docs.contains_key(&e.slug))
        .map(|e| LoadError::Missing {
            slug: e.slug.clone(),
            path: format!("content/{}.md", e.slug),
        })
        .collect();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(missing)
    }
}
