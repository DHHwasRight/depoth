//! The navigation tree.
//!
//! One declaration, outside the content, yielding the sidebar, the tab bar,
//! breadcrumbs and prev/next. The model is Mintlify's, and so is its one
//! inviolable rule: **a node carries exactly one kind of child**. A tab holds
//! groups or pages, never both. That rule is why a 489-page site stays a
//! readable file, and why a tree can be validated rather than merely parsed.
//!
//! Navigation names slugs only. Every title comes from the page's own
//! frontmatter, so a title is never written twice -- which means the sidebar
//! cannot be built until content is loaded. The build order is: discover
//! content, read frontmatter, resolve navigation, render.

use serde::Deserialize;

/// The root of the tree. Exactly one of `tabs`, `groups` or `pages`.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Navigation {
    #[serde(default)]
    pub tabs: Option<Vec<Tab>>,
    #[serde(default)]
    pub groups: Option<Vec<Group>>,
    #[serde(default)]
    pub pages: Option<Vec<Node>>,
    /// Links shown on every page regardless of section. External only --
    /// an anchor is a link, not a container.
    #[serde(default)]
    pub anchors: Vec<Anchor>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tab {
    pub tab: String,
    #[serde(default)]
    pub icon: Option<String>,
    /// Makes the tab a plain external link rather than a container.
    #[serde(default)]
    pub href: Option<String>,
    #[serde(default)]
    pub groups: Option<Vec<Group>>,
    #[serde(default)]
    pub pages: Option<Vec<Node>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Group {
    pub group: String,
    #[serde(default)]
    pub icon: Option<String>,
    /// A short badge beside the group name.
    #[serde(default)]
    pub tag: Option<String>,
    /// Clicking the group title opens this page.
    #[serde(default)]
    pub root: Option<String>,
    /// Nested groups are accordions, closed unless this is set. A top-level
    /// group is always open, which is Mintlify's rule and a good one: the
    /// outermost level is a section heading, not a control.
    #[serde(default)]
    pub expanded: bool,
    #[serde(default)]
    pub pages: Vec<Node>,
}

/// An entry under `pages`: either a slug, or a nested group. Nothing else --
/// in particular there is no external link here, because a link belongs to a
/// tab or an anchor.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum Node {
    Page(String),
    Group(Box<Group>),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Anchor {
    pub anchor: String,
    pub href: String,
    #[serde(default)]
    pub icon: Option<String>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum NavError {
    #[error("navigation declares {found:?}; a node carries exactly one kind of child")]
    MultipleChildKinds { found: Vec<&'static str> },
    #[error("navigation declares no children; add one of tabs, groups or pages")]
    NoChildren,
    #[error("tab {tab:?} declares {found:?}; a node carries exactly one kind of child")]
    TabMultipleChildKinds { tab: String, found: Vec<&'static str> },
    #[error("tab {tab:?} has no pages and no href")]
    TabEmpty { tab: String },
    #[error("page {slug:?} appears more than once in the navigation")]
    DuplicatePage { slug: String },
}

/// One page, flattened out of the tree in reading order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub slug: String,
    /// The tab it sits under, if the site has tabs.
    pub tab: Option<String>,
    /// Enclosing group names, outermost first. The breadcrumb.
    pub trail: Vec<String>,
    /// Position in the whole flattened order.
    pub index: usize,
}

impl Navigation {
    fn root_kinds(&self) -> Vec<&'static str> {
        let mut found = Vec::new();
        if self.tabs.is_some() { found.push("tabs"); }
        if self.groups.is_some() { found.push("groups"); }
        if self.pages.is_some() { found.push("pages"); }
        found
    }

    /// Checks the one-child-kind rule and that no page is listed twice.
    pub fn validate(&self) -> Result<(), NavError> {
        match self.root_kinds().as_slice() {
            [] => return Err(NavError::NoChildren),
            [_] => {}
            found => {
                return Err(NavError::MultipleChildKinds { found: found.to_vec() })
            }
        }
        for tab in self.tabs.iter().flatten() {
            let mut found = Vec::new();
            if tab.groups.is_some() { found.push("groups"); }
            if tab.pages.is_some() { found.push("pages"); }
            if found.len() > 1 {
                return Err(NavError::TabMultipleChildKinds {
                    tab: tab.tab.clone(),
                    found,
                });
            }
            if found.is_empty() && tab.href.is_none() {
                return Err(NavError::TabEmpty { tab: tab.tab.clone() });
            }
        }
        let entries = self.flatten();
        let mut seen = std::collections::HashSet::new();
        for entry in &entries {
            if !seen.insert(entry.slug.as_str()) {
                return Err(NavError::DuplicatePage { slug: entry.slug.clone() });
            }
        }
        Ok(())
    }

    /// Every page, in reading order. This is what prev/next, the sidebar and
    /// `llms.txt` are all built from, so they cannot disagree.
    pub fn flatten(&self) -> Vec<Entry> {
        let mut out = Vec::new();
        if let Some(tabs) = &self.tabs {
            for tab in tabs {
                let name = Some(tab.tab.clone());
                for group in tab.groups.iter().flatten() {
                    walk_group(group, &name, &mut Vec::new(), &mut out);
                }
                for node in tab.pages.iter().flatten() {
                    walk_node(node, &name, &mut Vec::new(), &mut out);
                }
            }
        }
        for group in self.groups.iter().flatten() {
            walk_group(group, &None, &mut Vec::new(), &mut out);
        }
        for node in self.pages.iter().flatten() {
            walk_node(node, &None, &mut Vec::new(), &mut out);
        }
        for (index, entry) in out.iter_mut().enumerate() {
            entry.index = index;
        }
        out
    }
}

fn walk_group(
    group: &Group,
    tab: &Option<String>,
    trail: &mut Vec<String>,
    out: &mut Vec<Entry>,
) {
    trail.push(group.group.clone());
    // A group's own root page comes before its children, which is the order a
    // reader meets them.
    if let Some(root) = &group.root {
        out.push(Entry {
            slug: root.clone(),
            tab: tab.clone(),
            trail: trail.clone(),
            index: 0,
        });
    }
    for node in &group.pages {
        walk_node(node, tab, trail, out);
    }
    trail.pop();
}

fn walk_node(node: &Node, tab: &Option<String>, trail: &mut Vec<String>, out: &mut Vec<Entry>) {
    match node {
        Node::Page(slug) => out.push(Entry {
            slug: slug.clone(),
            tab: tab.clone(),
            trail: trail.clone(),
            index: 0,
        }),
        Node::Group(group) => walk_group(group, tab, trail, out),
    }
}

/// The page before and after `slug`, within its own tab. Prev/next crossing a
/// tab boundary would walk a reader out of the section they are reading.
pub fn neighbours<'a>(entries: &'a [Entry], slug: &str) -> (Option<&'a Entry>, Option<&'a Entry>) {
    let Some(at) = entries.iter().position(|e| e.slug == slug) else {
        return (None, None);
    };
    let tab = &entries[at].tab;
    let prev = at
        .checked_sub(1)
        .map(|i| &entries[i])
        .filter(|e| &e.tab == tab);
    let next = entries.get(at + 1).filter(|e| &e.tab == tab);
    (prev, next)
}

#[cfg(test)]
mod tests {
    use super::*;
    use indoc::indoc;

    fn nav(src: &str) -> Navigation {
        serde_norway::from_str(src).expect("parse")
    }

    const TABBED: &str = indoc! {"
        tabs:
          - tab: Guide
            groups:
              - group: Introduction
                pages: [guide/welcome, guide/why]
              - group: Get started
                root: guide/start
                pages:
                  - group: Self-hosted
                    pages: [guide/self-hosted/overview, guide/self-hosted/docker]
                  - guide/cloud
          - tab: API
            groups:
              - group: Reference
                pages: [api/intro]
    "};

    #[test]
    fn flattening_follows_reading_order_and_keeps_the_trail() {
        let entries = nav(TABBED).flatten();
        let slugs: Vec<_> = entries.iter().map(|e| e.slug.as_str()).collect();
        assert_eq!(
            slugs,
            [
                "guide/welcome",
                "guide/why",
                "guide/start",
                "guide/self-hosted/overview",
                "guide/self-hosted/docker",
                "guide/cloud",
                "api/intro",
            ]
        );
        // The trail is the breadcrumb, outermost group first.
        let docker = &entries[4];
        assert_eq!(docker.trail, ["Get started", "Self-hosted"]);
        assert_eq!(docker.tab.as_deref(), Some("Guide"));
        assert_eq!(docker.index, 4);
    }

    #[test]
    fn a_group_root_page_comes_before_its_children() {
        let entries = nav(TABBED).flatten();
        let start = entries.iter().position(|e| e.slug == "guide/start").unwrap();
        let first_child = entries
            .iter()
            .position(|e| e.slug == "guide/self-hosted/overview")
            .unwrap();
        assert!(start < first_child);
    }

    #[test]
    fn neighbours_do_not_cross_a_tab_boundary() {
        let entries = nav(TABBED).flatten();
        // Last page of the Guide tab: there is a page after it in the flat
        // order, but it belongs to another tab.
        let (prev, next) = neighbours(&entries, "guide/cloud");
        assert_eq!(prev.map(|e| e.slug.as_str()), Some("guide/self-hosted/docker"));
        assert_eq!(next, None, "next should stop at the tab boundary");

        let (prev, next) = neighbours(&entries, "api/intro");
        assert_eq!(prev, None, "prev should stop at the tab boundary");
        assert_eq!(next, None);
    }

    #[test]
    fn a_node_carrying_two_kinds_of_child_is_rejected() {
        let err = nav(indoc! {"
            tabs:
              - tab: Guide
                groups: [{group: A, pages: [a]}]
                pages: [b]
        "})
        .validate()
        .expect_err("should reject");
        assert_eq!(
            err,
            NavError::TabMultipleChildKinds {
                tab: "Guide".into(),
                found: vec!["groups", "pages"],
            }
        );
    }

    #[test]
    fn a_page_listed_twice_is_rejected() {
        // Two sidebar entries for one page makes prev/next ambiguous and the
        // active state flicker between them.
        let err = nav(indoc! {"
            groups:
              - group: A
                pages: [intro, setup]
              - group: B
                pages: [setup]
        "})
        .validate()
        .expect_err("should reject");
        assert_eq!(err, NavError::DuplicatePage { slug: "setup".into() });
    }

    #[test]
    fn an_empty_tab_without_a_link_is_rejected() {
        let err = nav("tabs: [{tab: Ghost}]").validate().expect_err("reject");
        assert_eq!(err, NavError::TabEmpty { tab: "Ghost".into() });
    }

    #[test]
    fn a_tab_that_is_only_a_link_is_allowed() {
        nav(indoc! {"
            tabs:
              - tab: Knowledge base
                href: https://example.com
              - tab: Guide
                pages: [index]
        "})
        .validate()
        .expect("an external tab needs no pages");
    }
}
