//! The contract between the pipeline, the templates and the content.
//!
//! Written before any of the three, so they can be built independently.

/// A heading lifted out of a document, for the on-this-page rail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heading {
    pub level: u8,
    pub id: String,
    pub text: String,
}

/// One documentation page, after its markdown has been rendered.
#[derive(Debug, Clone)]
pub struct Doc {
    /// URL path, e.g. `/docs/install`.
    pub url: String,
    /// Slug as written in the navigation, e.g. `docs/install`.
    pub slug: String,
    pub title: String,
    pub description: Option<String>,
    /// Shorter label for the sidebar, when the title is too long for it.
    pub sidebar_title: Option<String>,
    /// Rendered body.
    pub html: String,
    pub headings: Vec<Heading>,
}

impl Doc {
    pub fn nav_label(&self) -> &str {
        self.sidebar_title.as_deref().unwrap_or(&self.title)
    }
}

/// One entry in the rendered sidebar: either a group heading or a link.
#[derive(Debug, Clone)]
pub enum SidebarItem {
    Group {
        label: String,
        depth: usize,
    },
    Link {
        label: String,
        url: String,
        current: bool,
        depth: usize,
    },
}

/// Everything a layout needs that is not the page itself.
#[derive(Debug, Clone)]
pub struct Chrome {
    pub site_name: String,
    /// Tabs across the top, with the active one marked.
    pub tabs: Vec<Tab>,
    /// The sidebar for the current tab.
    pub sidebar: Vec<SidebarItem>,
    /// Breadcrumb trail, outermost first.
    pub trail: Vec<String>,
    pub prev: Option<Link>,
    pub next: Option<Link>,
    /// External links shown on every page.
    pub anchors: Vec<Link>,
}

#[derive(Debug, Clone)]
pub struct Link {
    pub label: String,
    pub url: String,
}

#[derive(Debug, Clone)]
pub struct Tab {
    pub label: String,
    pub url: String,
    pub current: bool,
    pub external: bool,
}
