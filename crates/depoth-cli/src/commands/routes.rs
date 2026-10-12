//! `depoth routes` -- the resolved navigation, as a tree.
//!
//! Modelled on `bin/rails routes`, which is cheap to build and
//! disproportionately useful: the navigation is declared in one file as slugs,
//! every title comes from somewhere else, and the shape a reader will actually
//! see is not obvious from reading either. Printing it answers "is this page
//! reachable?", "which file is behind this URL?" and "did my rename land?"
//! without a build.

use std::path::Path;
use std::process::ExitCode;

use clap::Args;
use depoth::nav::{Group, Navigation, Node};

use crate::commands::Outcome;
use crate::project::Project;
use crate::ui::{Cell, Table, Ui, ACCENT, BAD, DIM, PLAIN, STRONG};

#[derive(Args)]
pub struct RoutesArgs {
    /// Print one row per page with no tree, for grep
    #[arg(long)]
    pub flat: bool,
}

pub fn run(ui: &Ui, dir: Option<&Path>, args: RoutesArgs) -> Outcome {
    let project = Project::discover(dir)?;
    let roots = build(&project.config.navigation);

    ui.heading("routes", project.display(&project.config_path));

    let mut table = Table::new();
    table.row(vec![
        Cell::styled(if args.flat { "PAGE" } else { "NAVIGATION" }, DIM),
        Cell::styled("URL", DIM),
        Cell::styled("SOURCE", DIM),
    ]);

    let mut pages = 0usize;
    let mut missing = 0usize;
    let mut rows = Vec::new();
    for (i, node) in roots.iter().enumerate() {
        if args.flat {
            flatten(node, &mut rows);
        } else {
            walk(node, "", i + 1 == roots.len(), true, &mut rows);
        }
    }
    for row in rows {
        match &row.kind {
            Kind::Page { slug } => {
                pages += 1;
                let source = project.source_for(slug);
                if source.is_none() {
                    missing += 1;
                }
                table.row(vec![
                    Cell::styled(row.prefix, DIM).then(row.label, PLAIN),
                    Cell::styled(url_of(slug), ACCENT),
                    match source {
                        Some(path) => Cell::styled(project.display(&path), DIM),
                        None => Cell::styled("not written", BAD),
                    },
                ]);
            }
            Kind::External { href } => {
                table.row(vec![
                    Cell::styled(row.prefix, DIM).then(row.label, PLAIN),
                    Cell::styled(href.clone(), ACCENT),
                    Cell::styled("external", DIM),
                ]);
            }
            Kind::Section => {
                table.row(vec![
                    Cell::styled(row.prefix, DIM).then(row.label, STRONG),
                    Cell::new(""),
                    Cell::new(""),
                ]);
            }
        }
    }
    ui.table(&table);

    let anchors = &project.config.navigation.anchors;
    if !anchors.is_empty() {
        ui.say("");
        let mut table = Table::new();
        table.row(vec![
            Cell::styled("ANCHOR", DIM),
            Cell::styled("HREF", DIM),
            Cell::new(""),
        ]);
        for anchor in anchors {
            table.row(vec![
                Cell::new(anchor.anchor.clone()),
                Cell::styled(anchor.href.clone(), ACCENT),
                Cell::styled("every page", DIM),
            ]);
        }
        ui.table(&table);
    }

    ui.gap();
    let mut summary = format!("{pages} pages");
    if !anchors.is_empty() {
        summary.push_str(&format!(" \u{b7} {} anchors", anchors.len()));
    }
    if missing > 0 {
        ui.warn(format!(
            "{summary} \u{b7} {missing} named in the navigation but not written"
        ));
        ui.note("  run `depoth check` to see the file each one wants");
    } else {
        ui.done(summary);
    }
    Ok(ExitCode::SUCCESS)
}

// -- the tree ---------------------------------------------------------------

#[derive(Clone)]
enum Kind {
    Section,
    Page { slug: String },
    External { href: String },
}

struct Entry {
    label: String,
    kind: Kind,
    children: Vec<Entry>,
}

/// A flattened row: the tree rule that leads to it, and its own label.
struct Row {
    prefix: String,
    label: String,
    kind: Kind,
}

/// Reads the declaration rather than `Navigation::flatten`, because the tree
/// is the thing being printed: flattening deliberately throws away the
/// difference between a group and the pages inside it.
fn build(nav: &Navigation) -> Vec<Entry> {
    if let Some(tabs) = &nav.tabs {
        return tabs
            .iter()
            .map(|tab| match &tab.href {
                // A tab that is only a link is a leaf, and saying so is the
                // point: it has no pages and never will.
                Some(href) => Entry {
                    label: tab.tab.clone(),
                    kind: Kind::External { href: href.clone() },
                    children: Vec::new(),
                },
                None => Entry {
                    label: tab.tab.clone(),
                    kind: Kind::Section,
                    children: tab
                        .groups
                        .iter()
                        .flatten()
                        .map(from_group)
                        .chain(tab.pages.iter().flatten().map(from_node))
                        .collect(),
                },
            })
            .collect();
    }
    if let Some(groups) = &nav.groups {
        return groups.iter().map(from_group).collect();
    }
    nav.pages.iter().flatten().map(from_node).collect()
}

fn from_group(group: &Group) -> Entry {
    let mut children: Vec<Entry> = Vec::new();
    // A group's own landing page comes first, which is the order the reader
    // meets it and the order `Navigation::flatten` uses.
    if let Some(root) = &group.root {
        children.push(Entry {
            label: leaf_of(root).to_string(),
            kind: Kind::Page { slug: root.clone() },
            children: Vec::new(),
        });
    }
    children.extend(group.pages.iter().map(from_node));
    Entry {
        label: group.group.clone(),
        kind: Kind::Section,
        children,
    }
}

fn from_node(node: &Node) -> Entry {
    match node {
        Node::Page(slug) => Entry {
            label: leaf_of(slug).to_string(),
            kind: Kind::Page { slug: slug.clone() },
            children: Vec::new(),
        },
        Node::Group(group) => from_group(group),
    }
}

/// Where a slug is served. The file convention makes a bare `index` the root;
/// a nested one, `guides/index`, is already `guides` by the time the
/// navigation names it.
fn url_of(slug: &str) -> String {
    if slug == "index" {
        "/".to_string()
    } else {
        format!("/{slug}")
    }
}

/// `docs/self-hosted/docker` -> `docker`. The trail is already the tree, so
/// repeating it in every label would be noise three levels deep.
fn leaf_of(slug: &str) -> &str {
    slug.rsplit('/').next().unwrap_or(slug)
}

/// The standard tree walk. A root draws no rule; everything below inherits
/// the rule its parent drew, which is what keeps a five-deep navigation
/// readable.
fn walk(entry: &Entry, prefix: &str, last: bool, root: bool, out: &mut Vec<Row>) {
    let (branch, carry) = match (root, last) {
        (true, _) => ("", ""),
        (false, true) => ("\u{2514}\u{2500} ", "   "),
        (false, false) => ("\u{251c}\u{2500} ", "\u{2502}  "),
    };
    out.push(Row {
        prefix: format!("{prefix}{branch}"),
        label: entry.label.clone(),
        kind: entry.kind.clone(),
    });
    let child_prefix = format!("{prefix}{carry}");
    let n = entry.children.len();
    for (i, child) in entry.children.iter().enumerate() {
        walk(child, &child_prefix, i + 1 == n, false, out);
    }
}

/// One row per page, full slug, no rules: a line that stands on its own under
/// grep. A section heading carries nothing a grep wants, so it is dropped.
fn flatten(entry: &Entry, out: &mut Vec<Row>) {
    if !matches!(entry.kind, Kind::Section) {
        out.push(Row {
            prefix: String::new(),
            label: match &entry.kind {
                Kind::Page { slug } => slug.clone(),
                _ => entry.label.clone(),
            },
            kind: entry.kind.clone(),
        });
    }
    for child in &entry.children {
        flatten(child, out);
    }
}
