//! The landing page, after docs.stripe.com.
//!
//! The shape of that page is three ideas. A hero that hands you a working
//! value — a command you can run — instead of a promise. A card grid where
//! each group owns one colour, so the grid separates rather than decorates.
//! A footer that helps rather than lists.
//!
//! Colour obeys the rule in PLAN §7: **sky is the accent and appears wherever
//! interaction lives** — the primary button, every link, every hover title,
//! and the hero wash it all sits in. Every other hue is bound to exactly one
//! meaning: mint is Getting started, lilac is Writing, peach is Shipping,
//! orchid is Background, and none of them appears anywhere else on the page.
//!
//! Layout and spacing are Tailwind utilities. `assets/css/landing.css` holds
//! only the three things a utility cannot say: the dot scrim over the hero
//! wash, the fog ellipse behind the headline, and the per-group hover glow.

use crate::data::Chrome;
use crate::views::docs::Header;

// ---------------------------------------------------------------------- slugs
//
// Every internal link on this page resolves here, so a change to the content
// tree is a change to this block and nowhere else. Trailing slashes, because
// the output is directories of index.html and that is the form the tests use.

const SITE_CSS: &str = concat!(env!("DEPOTH_BASE"), "/assets/css/site.css");
const URL_WHAT: &str = concat!(env!("DEPOTH_BASE"), "/docs/");
const URL_INSTALL: &str = concat!(env!("DEPOTH_BASE"), "/docs/install/");
const URL_CONFIG: &str = concat!(env!("DEPOTH_BASE"), "/docs/configuration/");
const URL_CONTENT: &str = concat!(env!("DEPOTH_BASE"), "/docs/content/");
const URL_NAV: &str = concat!(env!("DEPOTH_BASE"), "/docs/navigation/");
const URL_LAYOUTS: &str = concat!(env!("DEPOTH_BASE"), "/docs/layouts/");
const URL_PARTIALS: &str = concat!(env!("DEPOTH_BASE"), "/docs/partial-navigation/");
const URL_TESTING: &str = concat!(env!("DEPOTH_BASE"), "/docs/testing/");
const URL_DEPLOYING: &str = concat!(env!("DEPOTH_BASE"), "/docs/deploying/");
const URL_DESIGN: &str = concat!(env!("DEPOTH_BASE"), "/docs/design-notes/");

const URL_REPO: &str = "https://github.com/DHHwasRight/depoth";
const URL_ISSUES: &str = "https://github.com/DHHwasRight/depoth/issues";
const URL_DISCUSS: &str = "https://github.com/DHHwasRight/depoth/discussions";
const URL_RELEASES: &str = "https://github.com/DHHwasRight/depoth/releases";
const URL_LLMS: &str = concat!(env!("DEPOTH_BASE"), "/llms.txt");

/// The command the hero card hands over. Shown, and copied verbatim.
const BUILD_COMMAND: &str = "cargo run -p my-site -- build";

// ----------------------------------------------------------------------- data
//
// The page's own copy. It is static, so it is a const rather than a parameter:
// `Chrome` carries the site's identity and its external anchors, which is all
// this page needs from the pipeline.

pub struct Shortcut {
    pub label: &'static str,
    pub url: &'static str,
}

pub struct Column {
    pub heading: &'static str,
    pub links: &'static [Shortcut],
}

pub struct Entry {
    pub title: &'static str,
    pub blurb: &'static str,
    pub url: &'static str,
}

/// One card group. `hairline` and `tone` are the two places its hue is
/// allowed to appear: a 1px rule under the label, and the hover glow its
/// cards share. Both are written as literal class names so Tailwind's
/// scanner sees the hue and emits its variable.
pub struct Group {
    pub label: &'static str,
    pub hairline: &'static str,
    pub tone: &'static str,
    pub entries: &'static [Entry],
}

const COLUMNS: &[Column] = &[
    Column {
        heading: "Get going",
        links: &[
            Shortcut { label: "What depoth is", url: URL_WHAT },
            Shortcut { label: "Install depoth", url: URL_INSTALL },
            Shortcut { label: "Configure a site", url: URL_CONFIG },
        ],
    },
    Column {
        heading: "Author",
        links: &[
            Shortcut { label: "Write content", url: URL_CONTENT },
            Shortcut { label: "Declare navigation", url: URL_NAV },
            Shortcut { label: "Add a layout", url: URL_LAYOUTS },
        ],
    },
    Column {
        heading: "Go live",
        links: &[
            Shortcut { label: "Prerender partials", url: URL_PARTIALS },
            Shortcut { label: "Test every page", url: URL_TESTING },
            Shortcut { label: "Deploy the output", url: URL_DEPLOYING },
        ],
    },
];

const GROUPS: &[Group] = &[
    Group {
        label: "Getting started",
        hairline: "bg-mint-base",
        tone: "g-mint",
        entries: &[
            Entry {
                title: "What depoth is",
                blurb: "A static content framework in Rust, built on compiled templates and prerendered partial navigation.",
                url: URL_WHAT,
            },
            Entry {
                title: "Installing depoth",
                blurb: "A site is a Rust crate, so nothing is installed globally. You add three dependencies and build.",
                url: URL_INSTALL,
            },
            Entry {
                title: "Configuration",
                blurb: "depoth.yaml, key by key: identity, directories, navigation, redirects, commands and bundles.",
                url: URL_CONFIG,
            },
        ],
    },
    Group {
        label: "Writing",
        hairline: "bg-lilac-base",
        tone: "g-lilac",
        entries: &[
            Entry {
                title: "Writing content",
                blurb: "Frontmatter keys, the markdown body, and how a mistake in a header reports a file, a line and a column.",
                url: URL_CONTENT,
            },
            Entry {
                title: "Navigation",
                blurb: "One tree, declared outside the content, yielding the sidebar, the tab bar, breadcrumbs and prev/next.",
                url: URL_NAV,
            },
            Entry {
                title: "Layouts and templates",
                blurb: "Templates are rsx. They compile into the binary, which buys type checking and costs a rebuild.",
                url: URL_LAYOUTS,
            },
        ],
    },
    Group {
        label: "Shipping",
        hairline: "bg-peach-base",
        tone: "g-peach",
        entries: &[
            Entry {
                title: "Partial navigation",
                blurb: "Every route is prerendered as a document and as smaller trees, so a navigation replaces only what changed.",
                url: URL_PARTIALS,
            },
            Entry {
                title: "Testing a built site",
                blurb: "Six rules asserted over the output HTML in process, and the three claims that still need a browser.",
                url: URL_TESTING,
            },
            Entry {
                title: "Deploying a site",
                blurb: "Building for a static host, publishing to GitHub Pages, and serving the partial trees through nginx.",
                url: URL_DEPLOYING,
            },
        ],
    },
    Group {
        label: "Background",
        hairline: "bg-orchid-base",
        tone: "g-orchid",
        entries: &[
            Entry {
                title: "Design notes",
                blurb: "The decisions worth knowing: the two clocks, the type as the schema, propose rather than choose, no backend.",
                url: URL_DESIGN,
            },
        ],
    },
];

// Icons are single stroke paths, so one SVG renders all five.
const ICON_SUPPORT: &str = "M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18Zm0 5.5a3.5 3.5 0 1 0 0 7 3.5 3.5 0 0 0 0-7ZM5.6 5.6l3.9 3.9M14.5 14.5l3.9 3.9M18.4 5.6l-3.9 3.9M9.5 14.5l-3.9 3.9";
const ICON_CHAT: &str = "M4 5h16v11H9.5L4.5 20V5Z";
const ICON_CHANGELOG: &str = "M4 5h16M4 10h10M4 15h13M4 20h7";
const ICON_SOURCE: &str = "m9 8-5 4 5 4M15 8l5 4-5 4";
const ICON_LLMS: &str = "M7 3h7l4 4v14H7V3Zm7 0v4h4M10 13h5M10 17h5";

// -------------------------------------------------------------------- helpers
//
// Branching that produces a value lives in plain functions: inside a template
// body an `if` opens a markup conditional, so `let x = if …` never binds.

/// The site's own source, preferring a navbar anchor if one points at it.
fn source_url(chrome: &Chrome) -> &str {
    for anchor in &chrome.anchors {
        if anchor.url.contains("github.com") {
            return &anchor.url;
        }
    }
    URL_REPO
}

fn page_title(chrome: &Chrome) -> String {
    format!("{} documentation", chrome.site_name)
}

// ------------------------------------------------------------------ the page

/// The landing page. One container, four bands: hero, quick links, the card
/// grid, the help strip.
pub fn Landing(chrome: &Chrome) -> Element {
    <!DOCTYPE html>
    <html lang="en">
        <head>
            <meta charset="utf-8" />
            <meta name="viewport" content="width=device-width, initial-scale=1" />
            <title>{page_title(chrome)}</title>
            <meta name="description" content="A static content framework in Rust. Templates compile; content does not." />
            <link rel="stylesheet" href={SITE_CSS} />
        </head>
        <body class="bg-ground text-ink font-sans antialiased pt-28 lg:pt-14">
            <a href="#main-content" class="docs-skip">Skip to content</a>
            <Header chrome={chrome} />
            <main id="main-content" class="landing relative isolate overflow-x-clip">
        <div class="hero-wash" aria-hidden="true"></div>

        <div class="mx-auto w-[70vw] max-w-[1175px] px-12 py-8 max-[1000px]:w-[90vw] max-[1000px]:p-6">
            <Hero chrome={chrome} />
            <QuickLinks />

            <h2 id="docs" class="mb-6 text-[22px] leading-7 font-semibold tracking-tight">
                Documentation
            </h2>
            for group in GROUPS {
                <DocsGroup group={group} />
            }

            <HelpStrip chrome={chrome} />
        </div>

        // The copy button is revealed by this script, so a page with
        // JavaScript off shows a command it can select rather than a button
        // that does nothing.
        <script>
            document.querySelectorAll("[data-copy]").forEach(function (button) {
                button.hidden = false;
                button.addEventListener("click", function () {
                    navigator.clipboard.writeText(button.getAttribute("data-copy")).then(function () {
                        var was = button.textContent;
                        button.textContent = "Copied";
                        setTimeout(function () { button.textContent = was; }, 1200);
                    });
                });
            });
        </script>
        </main>
        </body>
    </html>
}

/// Headline, one filled action, one text link, and the utility card beside
/// them. The card is the highest-value element on the page, so it holds a
/// command rather than a picture.
pub fn Hero(chrome: &Chrome) -> Element {
    <section class="mb-[100px] flex items-start gap-10 max-[800px]:flex-col max-[800px]:gap-3">
        <div class="relative flex-[1_1_66%]">
            <div class="hero-fog" aria-hidden="true"></div>
            <div class="relative z-[1]">
                <h1 class="text-[40px] leading-[48px] font-semibold tracking-tight max-[800px]:text-[32px] max-[800px]:leading-[38px]">
                    {page_title(chrome)}
                </h1>
                <p class="mt-3 max-w-[34ch] text-[24px] leading-[28px] font-normal text-ink-soft max-[800px]:text-[20px] max-[800px]:leading-[26px]">
                    A static content framework in Rust. Templates compile, content does not,
                    and there is no backend.
                </p>
                <div class="mt-7 flex flex-wrap items-center gap-6">
                    <a href={URL_INSTALL}
                       class="inline-flex items-center rounded-md bg-accent px-4 py-2 text-[15px] font-semibold text-ground no-underline transition-opacity hover:opacity-90">
                        Get started
                    </a>
                    <a href="#docs"
                       class="text-[15px] font-medium text-accent underline decoration-dashed decoration-1 underline-offset-4">
                        Browse the documentation
                    </a>
                </div>
            </div>
        </div>

        <HeroCard />
    </section>
}

/// The utility card: a content edit never waits for a compiler, and this is
/// the command that proves it. The two numbers are the measured ones from
/// PLAN §4.1 — nothing else on this page claims a benchmark.
pub fn HeroCard() -> Element {
    <aside class="hero-card w-[352px] flex-[1_1_33%] rounded-lg border border-line bg-surface/70 p-4 backdrop-blur-sm max-[1000px]:hidden">
        <div class="flex items-center justify-between gap-3">
            <span class="text-[12px] font-semibold tracking-wide text-ink-soft uppercase">
                Build a site
            </span>
            <button type="button"
                    hidden
                    data-copy={BUILD_COMMAND}
                    class="rounded px-1 text-[12px] font-semibold text-accent">
                Copy
            </button>
        </div>

        <code class="mt-3 block overflow-x-auto rounded-md bg-sunken px-3 py-2 font-mono text-[13px] whitespace-pre text-ink">
            {BUILD_COMMAND}
        </code>

        <dl class="mt-4 space-y-1 text-[13px] leading-5">
            <div class="flex items-baseline justify-between gap-3">
                <dt class="text-ink-soft">Template edit, rebuild</dt>
                <dd class="font-mono">0.50 s</dd>
            </div>
            <div class="flex items-baseline justify-between gap-3">
                <dt class="text-ink-soft">Content edit, re-run</dt>
                <dd class="font-mono">0.16 s</dd>
            </div>
        </dl>
    </aside>
}

/// Three columns of shortcuts, above the full grid. They repeat destinations
/// the grid also reaches, phrased as the task rather than the page.
pub fn QuickLinks() -> Element {
    <section class="mb-12 grid grid-cols-3 gap-4 max-[1000px]:hidden">
        for column in COLUMNS {
            <div>
                <h2 class="mb-2 text-[14px] leading-5 font-semibold">{column.heading}</h2>
                <ul class="space-y-1">
                    for link in column.links {
                        <li>
                            <a href={link.url} class="text-[14px] leading-5 text-accent hover:underline">
                                {link.label}
                            </a>
                        </li>
                    }
                </ul>
            </div>
        }
    </section>
}

/// One group: a muted label, a hairline in the group's own hue, then its
/// cards. The hue reaches the cards through `tone`, which carries the two
/// glow colours the hover state uses.
pub fn DocsGroup(group: &Group) -> Element {
    <section class={group_class(group)}>
        <h3 class="text-[14px] leading-5 text-ink-soft">{group.label}</h3>
        <div class={hairline_class(group)} aria-hidden="true"></div>
        <div class="grid auto-rows-fr grid-cols-[repeat(auto-fill,minmax(min(300px,100%),1fr))] gap-4">
            for entry in group.entries {
                <DocsCard entry={entry} />
            }
        </div>
    </section>
}

/// A whole card is the link, so the hit area is the card.
pub fn DocsCard(entry: &Entry) -> Element {
    <a href={entry.url}
       class="doc-card group block rounded-[6px] border border-line bg-ground p-4 no-underline">
        <span class="doc-card__title block text-[16px] leading-6 font-semibold text-ink transition-colors">
            {entry.title}
        </span>
        <span class="mt-1 block text-[14px] leading-5 font-normal text-ink-soft">
            {entry.blurb}
        </span>
    </a>
}

/// Five ways to get help, not a link farm. Stripe's footer is a help strip,
/// and that is the better idea.
pub fn HelpStrip(chrome: &Chrome) -> Element {
    <footer class="mt-16 flex gap-6 border-t border-line pt-8 max-[800px]:flex-col max-[800px]:gap-5">
        <HelpItem icon={ICON_SUPPORT}
                  label="Support"
                  sentence="Report a bug, or ask for a feature."
                  url={URL_ISSUES} />
        <HelpItem icon={ICON_CHAT}
                  label="Discussions"
                  sentence="Ask a question where others can read the answer."
                  url={URL_DISCUSS} />
        <HelpItem icon={ICON_CHANGELOG}
                  label="Changelog"
                  sentence="See what each release changed."
                  url={URL_RELEASES} />
        <HelpItem icon={ICON_SOURCE}
                  label="Source"
                  sentence="Read the framework, and this site with it."
                  url={source_url(chrome)} />
        <HelpItem icon={ICON_LLMS}
                  label="llms.txt"
                  sentence="Every page as Markdown, for a model to read."
                  url={URL_LLMS} />
    </footer>
}

pub fn HelpItem(icon: &str, label: &str, sentence: &str, url: &str) -> Element {
    <div class="flex flex-1 items-start gap-3">
        <svg viewBox="0 0 24 24"
             width="20"
             height="20"
             fill="none"
             stroke="currentColor"
             stroke-width="1.5"
             aria-hidden="true"
             class="mt-0.5 shrink-0 text-ink-soft">
            <path d={icon} stroke-linecap="round" stroke-linejoin="round" />
        </svg>
        <div>
            <a href={url} class="text-[14px] leading-5 font-semibold text-accent hover:underline">
                {label}
            </a>
            <p class="mt-0.5 text-[13px] leading-5 text-ink-soft">{sentence}</p>
        </div>
    </div>
}

// ---------------------------------------------------------- class composition

fn group_class(group: &Group) -> String {
    format!("mb-12 {}", group.tone)
}

fn hairline_class(group: &Group) -> String {
    format!("mt-2 mb-4 h-px {}", group.hairline)
}
