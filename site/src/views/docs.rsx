//! The documentation shell: header, sidebar, article, on-this-page rail.
//!
//! Layout and spacing are Tailwind utilities in the markup. Only the things a
//! utility cannot say — the gutter hatch, the rail geometry, the prose rhythm,
//! the code-block chrome, the callouts — live in `assets/css/docs.css`.
//!
//! Colour never appears here as a literal. Every utility that paints resolves
//! to a token from `theme.css` (`bg-ground`, `text-ink-soft`, `border-line`,
//! `text-accent`, `bg-sky-low`, …), so a palette change stays in one file.

use crate::data::{Chrome, Doc, Heading, Link, SidebarItem, Tab};

/// Shown in the header pill. `Chrome` carries no version field, so the shell
/// owns it; move it to the data layer when the pipeline starts stamping one.
const VERSION: &str = "v0.1.0";

/// Stylesheets. The pipeline decides the final names; these are the two the
/// page needs: the compiled Tailwind build (which has `theme.css` as its
/// theme) and this layout's own rules.
const SITE_CSS: &str = "/css/site.css";
const DOCS_CSS: &str = "/css/docs.css";

// ---------------------------------------------------------------- the page

/// One documentation page, whole document included.
pub fn DocsPage(chrome: &Chrome, doc: &Doc) -> Element {
    <!DOCTYPE html>
    <html lang="en">
        <head>
            <meta charset="utf-8" />
            <meta name="viewport" content="width=device-width, initial-scale=1" />
            <title>{doc.title} · {chrome.site_name}</title>
            if let Some(description) = &doc.description {
                <meta name="description" content={description} />
            }
            <link rel="stylesheet" href={SITE_CSS} />
            <link rel="stylesheet" href={DOCS_CSS} />
        </head>

        // The header is fixed, so the document pays for its own height: two
        // bars below `lg`, one at and above it.
        <body class="bg-ground text-ink font-sans antialiased pt-28 lg:pt-14">
            <a href="#main-content"
               class="sr-only focus:not-sr-only focus:fixed focus:top-2 focus:left-2 focus:z-60 focus:rounded-lg focus:border focus:border-line focus:bg-ground focus:px-3 focus:py-2 focus:text-sm">
                Skip to content
            </a>

            <Header chrome={chrome} />

            // Four tracks: sidebar, gutter, content, gutter. Below `lg` the
            // grid is off, the gutters are hidden and the sidebar is a sheet.
            <div class="lg:grid lg:grid-cols-[288px_40px_minmax(0,1fr)_40px] lg:items-start">
                <Sidebar items={&chrome.sidebar} />
                <div class="docs-gutter hidden lg:block lg:min-h-[calc(100dvh-56px)] lg:sticky lg:top-14 lg:h-[calc(100dvh-56px)]" aria-hidden="true"></div>

                <div class="min-w-0">
                    <div class="mx-auto w-full max-w-[672px] px-4 sm:px-6 xl:max-w-[1024px] xl:grid xl:grid-cols-[minmax(0,1fr)_288px] xl:gap-10 xl:items-start xl:px-0">
                        <main id="main-content" class="min-w-0 py-10 xl:pl-6">
                            <PageHead chrome={chrome} doc={doc} />
                            // Rendered markdown. The content pipeline owns this
                            // HTML; `docs.css` owns how it reads.
                            <div class="prose mt-10">{! doc.html }</div>
                            <PageFooter chrome={chrome} />
                        </main>

                        <Toc headings={&doc.headings} />
                    </div>

                    <SiteFooter chrome={chrome} />
                </div>

                <div class="docs-gutter hidden lg:block lg:min-h-[calc(100dvh-56px)] lg:sticky lg:top-14 lg:h-[calc(100dvh-56px)]" aria-hidden="true"></div>
            </div>

            // Closes the sheet when the sidebar is open below `lg`.
            <button type="button" class="docs-scrim lg:hidden" data-nav-close tabindex="-1" aria-hidden="true"></button>

            <script>
(function () {
  var root = document.documentElement;

  // The navigation sheet. State lives on <html> so CSS alone can position
  // the sidebar, the scrim and the page behind them.
  var toggle = document.querySelector('[data-nav-toggle]');
  function setNav(open) {
    root.setAttribute('data-nav', open ? 'open' : 'closed');
    if (toggle) toggle.setAttribute('aria-expanded', open ? 'true' : 'false');
  }
  if (toggle) {
    toggle.addEventListener('click', function () {
      setNav(root.getAttribute('data-nav') !== 'open');
    });
  }
  document.querySelectorAll('[data-nav-close]').forEach(function (el) {
    el.addEventListener('click', function () { setNav(false); });
  });
  document.addEventListener('keydown', function (e) {
    if (e.key === 'Escape') setNav(false);
  });
  var sidebar = document.getElementById('docs-sidebar');
  if (sidebar) {
    sidebar.addEventListener('click', function (e) {
      if (e.target.closest('a')) setNav(false);
    });
  }

  // Code blocks. Markdown gives us a bare <pre><code>; the chrome around it
  // is progressive enhancement, so the panel still reads without script.
  document.querySelectorAll('.prose pre').forEach(function (pre) {
    if (pre.closest('.code-block')) return;

    var figure = document.createElement('figure');
    figure.className = 'code-block';
    pre.parentNode.insertBefore(figure, pre);

    var bar = document.createElement('figcaption');
    bar.className = 'code-block__bar';

    var code = pre.querySelector('code');
    var name = pre.getAttribute('data-filename') || '';
    if (!name && code) {
      var match = /(?:language|lang)-([\w+#-]+)/.exec(code.className || '');
      if (match) name = match[1];
    }
    bar.appendChild(document.createTextNode(name));

    var copy = document.createElement('button');
    copy.type = 'button';
    copy.className = 'code-block__copy';
    copy.setAttribute('aria-label', 'Copy code');
    copy.innerHTML = '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="9" y="9" width="11" height="11" rx="2"></rect><path d="M5 15V5a2 2 0 0 1 2-2h8"></path></svg>';
    copy.addEventListener('click', function () {
      var text = code ? code.innerText : pre.innerText;
      navigator.clipboard.writeText(text).then(function () {
        copy.setAttribute('data-copied', 'true');
        setTimeout(function () { copy.removeAttribute('data-copied'); }, 1600);
      });
    });
    bar.appendChild(copy);

    figure.appendChild(bar);
    figure.appendChild(pre);
  });
})();
            </script>
        </body>
    </html>
}

// --------------------------------------------------------------- the header

/// The fixed 56px bar, plus the 56px breadcrumb bar that replaces the sidebar
/// trigger below `lg`.
pub fn Header(chrome: &Chrome) -> Element {
    <header class="fixed inset-x-0 top-0 z-50 bg-ground">
        <div class="flex h-14 items-center gap-4 border-b border-line px-4 sm:px-6">
            <a href="/" class="flex shrink-0 items-center gap-2.5">
                <span class="text-[15px] font-semibold tracking-tight text-ink">{chrome.site_name}</span>
                <span class="rounded-full border border-line px-2 py-0.5 text-[12px] leading-5 tabular-nums text-ink-soft">
                    {VERSION}
                </span>
            </a>

            <div class="ml-auto flex items-center gap-1 sm:gap-2">
                <button type="button" data-search
                        class="flex items-center gap-2 rounded-lg border border-line px-2.5 py-1.5 text-[13px] text-ink-soft transition-colors hover:border-ink-soft hover:text-ink">
                    <svg class="size-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" aria-hidden="true">
                        <circle cx="11" cy="11" r="7"></circle>
                        <path d="m20 20-3.5-3.5"></path>
                    </svg>
                    <span class="hidden sm:inline">Search docs</span>
                    <kbd class="hidden rounded border border-line px-1.5 font-sans text-[11px] leading-5 tabular-nums text-ink-soft sm:inline">⌘K</kbd>
                </button>

                <nav class="hidden items-center gap-1 md:flex" aria-label="Sections">
                    for tab in &chrome.tabs {
                        <a href={tab.url}
                           class={tab_class(tab)}
                           aria-current={aria_current(tab.current)}
                           target={link_target(tab.external)}>
                            {tab.label}
                        </a>
                    }
                </nav>

                <span class="hidden h-5 w-px bg-line md:block" aria-hidden="true"></span>

                for anchor in &chrome.anchors {
                    <a href={anchor.url} target="_blank" rel="noreferrer"
                       class="flex items-center gap-1.5 rounded-lg px-2 py-1.5 text-[13px] text-ink-soft transition-colors hover:text-ink"
                       aria-label={anchor.label}>
                        if is_github(anchor) {
                            <svg class="size-4" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
                                <path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.012 8.012 0 0 0 16 8c0-4.42-3.58-8-8-8Z"></path>
                            </svg>
                        } else {
                            <span class="hidden sm:inline">{anchor.label}</span>
                        }
                    </a>
                }
            </div>
        </div>

        // Second bar: the sidebar trigger and where you are. Below `lg` only.
        <div class="flex h-14 items-center gap-3 border-b border-line px-4 sm:px-6 lg:hidden">
            <button type="button" data-nav-toggle aria-expanded="false" aria-controls="docs-sidebar"
                    class="-ml-1 flex size-8 shrink-0 items-center justify-center rounded-lg text-ink-soft transition-colors hover:bg-surface hover:text-ink">
                <span class="text-base leading-none" aria-hidden="true">☰</span>
                <span class="sr-only">Open navigation</span>
            </button>
            <nav class="flex min-w-0 items-center gap-1.5 text-[13px] text-ink-soft" aria-label="Breadcrumb">
                for (i, step) in chrome.trail.iter().enumerate() {
                    if i > 0 {
                        <span class="shrink-0 text-line" aria-hidden="true">›</span>
                    }
                    <span class={crumb_class(i, chrome.trail.len())}>{step}</span>
                }
            </nav>
        </div>
    </header>
}

// -------------------------------------------------------------- the sidebar

/// The left rail. Sticky and scrolling under the header at `lg`; a sheet
/// behind the ☰ below it.
pub fn Sidebar(items: &[SidebarItem]) -> Element {
    <aside id="docs-sidebar"
           class="docs-sidebar p-6 lg:sticky lg:top-14 lg:max-h-[calc(100dvh-56px)] lg:overflow-y-auto">
        <nav aria-label="Documentation">
            for group in nav_groups(items) {
                <div class="mt-8 first:mt-0" style={depth_style(group.depth)}>
                    if let Some(label) = group.label {
                        <h2 class="font-mono text-xs leading-6 font-medium uppercase tracking-[0.1em] text-ink-soft">
                            {label}
                        </h2>
                    }
                    <ul class="docs-rail mt-2">
                        for link in &group.links {
                            <li class="docs-rail__item">
                                <a href={link.url}
                                   class="docs-rail__link"
                                   style={depth_style(link.depth)}
                                   aria-current={aria_current(link.current)}>
                                    {link.label}
                                </a>
                            </li>
                        }
                    </ul>
                </div>
            }
        </nav>
    </aside>
}

// ---------------------------------------------------------------- the page

/// Eyebrow, title, lead.
pub fn PageHead(chrome: &Chrome, doc: &Doc) -> Element {
    <div>
        if let Some(section) = chrome.trail.first() {
            <p class="font-mono text-xs leading-6 font-medium uppercase tracking-[0.1em] text-accent">
                {section}
            </p>
        }
        <h1 class="mt-2 text-[30px] leading-9 font-medium tracking-tight text-ink">{doc.title}</h1>
        if let Some(description) = &doc.description {
            <p class="mt-6 text-base leading-7 text-ink-soft">{description}</p>
        }
    </div>
}

/// Previous and next, as two tiles. The relation on line one, the page it
/// leads to in bold on line two.
pub fn PageFooter(chrome: &Chrome) -> Element {
    <nav class="mt-16 grid gap-3 border-t border-line pt-8 sm:grid-cols-2" aria-label="Pagination">
        if let Some(prev) = &chrome.prev {
            <a href={prev.url} rel="prev"
               class="flex flex-col gap-1 rounded-xl border border-line px-4 py-3 text-[13px] leading-5 text-ink-soft transition-colors hover:border-accent hover:bg-surface">
                <span>Previous</span>
                <span class="font-semibold text-ink">{prev.label}</span>
            </a>
        } else {
            <span></span>
        }
        if let Some(next) = &chrome.next {
            <a href={next.url} rel="next"
               class="flex flex-col gap-1 rounded-xl border border-line px-4 py-3 text-[13px] leading-5 text-ink-soft transition-colors hover:border-accent hover:bg-surface sm:items-end sm:text-right">
                <span>Next</span>
                <span class="font-semibold text-ink">{next.label}</span>
            </a>
        }
    </nav>
}

/// The thin strip under the article.
pub fn SiteFooter(chrome: &Chrome) -> Element {
    <footer class="mx-auto w-full max-w-[672px] border-t border-line px-4 py-8 text-[13px] leading-5 text-ink-soft sm:px-6 xl:max-w-[1024px] xl:px-0">
        <div class="flex flex-wrap items-center gap-x-4 gap-y-2 xl:pl-6">
            <span>{chrome.site_name}</span>
            <span class="text-line" aria-hidden="true">·</span>
            <span>{VERSION}</span>
            for anchor in &chrome.anchors {
                <a href={anchor.url} class="transition-colors hover:text-ink">{anchor.label}</a>
            }
        </div>
    </footer>
}

// -------------------------------------------------------------- on this page

/// The right rail: the same shape as the sidebar, one level deeper. `xl` only.
pub fn Toc(headings: &[Heading]) -> Element {
    if !headings.is_empty() {
        <aside class="hidden xl:block xl:sticky xl:top-14 xl:max-h-[calc(100dvh-56px)] xl:overflow-y-auto xl:py-10 xl:pr-6">
            <h2 class="font-mono text-xs leading-6 font-medium uppercase tracking-[0.1em] text-ink-soft">
                On this page
            </h2>
            <nav class="mt-2" aria-label="On this page">
                <ul class="docs-rail">
                    for node in toc_tree(headings) {
                        <li class="docs-rail__item">
                            <a href={anchor_href(&node.heading.id)} class="docs-rail__link">
                                {node.heading.text}
                            </a>
                            if !node.children.is_empty() {
                                <ul class="docs-rail docs-rail--sub">
                                    for child in &node.children {
                                        <li class="docs-rail__item">
                                            <a href={anchor_href(&child.id)} class="docs-rail__link">
                                                {child.text}
                                            </a>
                                        </li>
                                    }
                                </ul>
                            }
                        </li>
                    }
                </ul>
            </nav>
        </aside>
    }
}

// ------------------------------------------------------------------ helpers
//
// Branching that produces a value lives out here: inside a template body `if`
// opens a markup conditional, so `let x = if …` never binds.

fn aria_current(current: bool) -> &'static str {
    // `false` is the ARIA default and the CSS hook is `[aria-current="page"]`,
    // so the attribute can be emitted unconditionally.
    if current {
        "page"
    } else {
        "false"
    }
}

fn link_target(external: bool) -> &'static str {
    if external {
        "_blank"
    } else {
        "_self"
    }
}

fn tab_class(tab: &Tab) -> &'static str {
    if tab.current {
        "rounded-lg px-2.5 py-1.5 text-[13px] font-semibold text-ink"
    } else {
        "rounded-lg px-2.5 py-1.5 text-[13px] text-ink-soft transition-colors hover:text-ink"
    }
}

fn crumb_class(index: usize, len: usize) -> &'static str {
    // The last step is the page itself; it gets the contrast and the ellipsis.
    if index + 1 == len {
        "truncate text-ink"
    } else {
        "hidden shrink-0 sm:inline"
    }
}

fn is_github(link: &Link) -> bool {
    link.label.eq_ignore_ascii_case("github") || link.url.contains("github.com")
}

fn anchor_href(id: &str) -> String {
    format!("#{id}")
}

/// Rail indentation, as a custom property the stylesheet multiplies out. A
/// style attribute keeps the depth out of the class list, which would
/// otherwise need one utility per level.
fn depth_style(depth: usize) -> String {
    format!("--rail-depth:{depth}")
}

// --- the sidebar, as groups --------------------------------------------

/// A run of links under one heading. `SidebarItem` arrives flat, and a rail
/// needs the run as a unit so the `border-left` can wrap it.
pub struct NavGroup<'a> {
    pub label: Option<&'a str>,
    pub depth: usize,
    pub links: Vec<NavLink<'a>>,
}

pub struct NavLink<'a> {
    pub label: &'a str,
    pub url: &'a str,
    pub current: bool,
    pub depth: usize,
}

/// Split the flat sidebar into one group per heading. Links before the first
/// heading become a leading group with no heading of its own.
pub fn nav_groups(items: &[SidebarItem]) -> Vec<NavGroup<'_>> {
    let mut groups: Vec<NavGroup<'_>> = Vec::new();

    for item in items {
        match item {
            SidebarItem::Group { label, depth } => groups.push(NavGroup {
                label: Some(label.as_str()),
                depth: *depth,
                links: Vec::new(),
            }),
            SidebarItem::Link {
                label,
                url,
                current,
                depth,
            } => {
                if groups.is_empty() {
                    groups.push(NavGroup {
                        label: None,
                        depth: 0,
                        links: Vec::new(),
                    });
                }
                let base = groups.last().map_or(0, |g| g.depth);
                if let Some(group) = groups.last_mut() {
                    group.links.push(NavLink {
                        label: label.as_str(),
                        url: url.as_str(),
                        current: *current,
                        depth: depth.saturating_sub(base),
                    });
                }
            }
        }
    }

    groups.retain(|group| !group.links.is_empty() || group.label.is_some());
    groups
}

// --- the rail, as a tree ------------------------------------------------

/// One top-level heading and everything nested under it.
pub struct TocNode<'a> {
    pub heading: &'a Heading,
    pub children: Vec<&'a Heading>,
}

/// Two levels, which is as deep as a rail stays readable. The shallowest
/// level present is the top, whether a page starts at `h2` or at `h3`.
pub fn toc_tree(headings: &[Heading]) -> Vec<TocNode<'_>> {
    let top = headings.iter().map(|h| h.level).min().unwrap_or(2);
    let mut nodes: Vec<TocNode<'_>> = Vec::new();

    for heading in headings {
        if heading.level == top || nodes.is_empty() {
            nodes.push(TocNode {
                heading,
                children: Vec::new(),
            });
        } else if let Some(node) = nodes.last_mut() {
            node.children.push(heading);
        }
    }

    nodes
}
