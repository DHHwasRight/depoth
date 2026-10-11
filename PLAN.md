# Depoth

A static content framework in Rust. Successor to the Crystal depoth, standing
on three libraries that already exist: [rsx](https://github.com/DHHwasRight/rsx)
for compiled templates, [prerender](https://github.com/DHHwasRight/prerender)
for layouts and the partial trees, and
[dp-swap](https://github.com/DHHwasRight/htmx) for the client.

Two sentences carry the whole design. **Templates compile; content does not.**
**There is no backend.**

## 1. Why there is a second one

The Crystal depoth reached 0.2.0, 315 specs, and one real 489-page site built
on it. It is feature-complete against its own plan. Everything in §2 is
inherited from it rather than invented here, and §3 exists because its
predecessor paid for those findings once already.

What a rewrite buys: rsx, prerender and dp-swap are published and tested, and
dp-swap has since grown `dp-layout` narrowing and header-mode depth
negotiation, which the Crystal sites never used. What it costs: Rust compiles
slower than Crystal, which lands squarely on ADR-2 and is this plan's main
open risk.

## 2. Decisions carried forward

Each row is a decision the Crystal project made, argued and shipped. The
delta column is what changes in Rust.

| # | Decision | Delta here |
|---|---|---|
| 1 | **Compile templates, do not interpret them.** An interpreter is a second, weaker implementation of a compiler that already exists. | Free. rsx compiles `.rsx` into the binary. |
| 2 | **Two build graphs on two clocks.** A template edit pays a compile; a content edit re-runs the already-built binary. Measured there: 2.0 s against 23 ms. | Measured here: **0.5 s**. See §4.1. |
| 3 | **File-based routes, content decoupled from them.** Hugo's lookup order is powerful and unpredictable; you cannot answer "which template rendered this page?" without a debug flag. | Port `page_router`'s conventions; match with `matchit`. |
| 4 | **The type is the schema.** Generating structs from a schema file is circular: reading the schema means compiling it, and its output is what needs compiling. | Better here. A derive plus serde gives coercion and messages for free. |
| 5 | **Dynamic routes without `getStaticPaths`.** A page declares the collection type as its parameter; the paths are a consequence of the type, so there is no second declaration to keep in step. | Same. |
| 6 | **One markdown extension mechanism, ranked.** A render hook first, because it takes markup out of every document at once. A component call inside a document last. | Same ranking, but the call must nest and must work inline. See §4.2. |
| 7 | **Islands are ES modules mounted into server-rendered markup.** Anything exporting `mount(el, props)`. A module that fails to load leaves the rendered markup alone, which is the reason for rendering it. | Same. |
| 8 | **Partial navigation is prerendered, never served.** | Free, and better: prerender writes one tree per depth and dp-swap negotiates. |
| 9 | **Every page works with JavaScript off.** A fragment is never the only representation of a page. This is a test, not a guideline. | Same, enforced by `depoth check` and by Playwright. |
| 10 | **Propose, don't choose.** Choosing a CSS framework or a bundler is the site's decision. Grep the Crystal source for "tailwind" and you get nothing. | Same. The doc site uses Tailwind; the framework does not know it exists. |
| 11 | **No backend, ever.** Output is files. No request handling, no compute at request time. | Same. |
| 12 | **Assets: `assets/` is processed and fingerprinted, `public/` is verbatim.** A leading `_` marks an input, bundled into whatever imports it and never published. | Same convention. |
| 13 | **Syntax highlighting at build time, stylesheet opt-in.** A site with no code on it ships no CSS for code. | `syntect` + `two-face` in place of the chroma port. |

### 6.2 Corrections to what was already written

- **The declared MSRV was false, twice.** It said 1.74, I raised it to 1.82 to
  satisfy clippy, and both were wrong: `oxc_parser` 0.153 declares
  **rust-version 1.97.0** and edition 2024, and `comrak` 0.55 declares 1.85.
  Now 1.97, which is what the tree actually requires.
- **`serde_norway` is stale** -- no release since December 2024 and its repo
  redirects. `yaml_serde` 0.10 has the identical `Error::location()` API and
  is maintained by the YAML organisation. Migrate before more code depends on
  it.
- **Drop `gray_matter`.** Splitting `---` fences is about ten lines, and doing
  it ourselves lets us keep the byte offset of the frontmatter block -- which
  is the difference between "line 3 of the frontmatter" and `page.md:5`.
- **Do not add `slug`.** It routes non-ASCII through `deunicode`, turning a
  Japanese heading into Mandarin readings and an emoji into `rocket`, and it
  has no collision dedupe. comrak's own `Anchorizer` is 30 lines and matches
  GitHub.
- **Do not add `minify-html`.** It hard-depends on `oxc_minifier` 0.95 against
  our 0.153, so cargo would compile both oxc stacks. The payoff after brotli
  is under 1% and on one real page it made the compressed output larger.

## 3. Traps, paid for once already

Carried from the Crystal project's appendix. Each cost real time there.

1. **The render cache must be versioned.** It served pre-highlighting HTML when
   the renderer changed and the bodies did not. Bump the signature whenever
   anything about how markdown renders changes.
2. **Empty the output directory first.** The doc site shipped two copies of its
   stylesheet for weeks — an old unfingerprinted one from when it lived in
   `public/`.
3. **A failing build must not kill the watcher.** "If building fails, we output
   an error message and wait for more changes, as one might bring things back
   to normal."
4. **A document's mistake must report, not crash.** Content loading sat outside
   the handler that turns a validation failure into a message, so a missing
   component raised ten frames of framework.
5. **Verify a process is live before profiling it.** A zombie from a timed-out
   run produced a "pathological" four-minute build that was really 5.8 s, and a
   fix was written for it before the mistake was found.

## 4. Open questions

### 4.1 Does the two-clock design survive Rust's compiler? — **yes, measured**

ADR-2's whole value is that a content author never waits for a compiler. The
Crystal project made a template edit cost 2.0 s, and the fear was that Rust
would be several times worse and take the template clock out of interactive
range.

Measured with `probe/` — thirteen templates, a layout, control flow, a
component call — on a warm `target/`:

| | |
|---|---|
| Cold build, every dependency | 21.3 s |
| **Template edit, rebuild** | **0.50 s** |
| No-op rebuild | 0.36 s |
| Re-run the built binary | 0.16 s |
| Release build | 49.0 s |

A template edit is **four times faster than Crystal's**, because the heavy
dependencies — comrak, oxc, syntect — are compiled once and the leaf crate is
all that recompiles. No fallback is needed and the design stands as written.

Two honest caveats. This is a thirteen-template probe, and codegen scales with
template count, so re-measure at the size of the real doc site. And the release
build is 49 s, which is a deploy cost rather than an authoring one, but it
means `--release` must never be the default for `dev`.

### 4.2 How does a document call a typed component?

The Crystal one read a call only at the start of a line, so `<Tab>` indented
inside `<Tabs>` rendered as literal markup. That broke exactly the three
containers a Mintlify-shaped documentation site is built from, and it was
named "the main thing standing between this and a faithful rebuild".

prerender already scans by position rather than at line start and has a test
for the nested case, so the parsing half is done. The open half is calling a
**real typed component** rather than rewriting the tag into a `<div>`.
Markdown is read at runtime and rsx components are compiled, so attributes
arrive as strings and cannot be compile-checked. That means a registry mapping
tag name to an adapter that parses attributes and calls the component — which
is what the Crystal one generated, and the shape to copy.

Inline calls inside a paragraph were also unsupported there, deliberately.
Revisit: Mintlify's `<Tooltip>` and `<Icon>` are inline by nature.

### 4.3 Search

The Crystal one had none, and both real sites wanted it. Decide between a
build-time index queried by a small hand-written client and an existing tool.
Pending the dependency survey.

### 4.4 Rendering order for the API reference

Mintlify generates ~235 pages from an OpenAPI document. The Crystal port
rendered their prose but not their parameter tables. Decide whether this is in
scope at all; it may be a separate crate that emits markdown.

## 5. Component library

Measured over a real 489-page Mintlify corpus, usage is extremely top-heavy.
Seven components carry almost all of it, and everything else is a long tail.

| Tier | Components | Why |
|---|---|---|
| **Ship first** | `Frame`, `Tabs`/`Tab`, callouts (`Note` `Info` `Tip` `Warning`), `CodeGroup`, `Card`/`CardGroup`, `Accordion`/`AccordionGroup`, `Steps`/`Step` | 348, 235, 435, 170, 182 and 119 uses respectively |
| **Ship next** | `Tooltip`, `Update`, `ParamField`, `ResponseField`, `Expandable`, `Badge` | Real but concentrated: `Update` is 50 uses on one changelog page |
| **Skip** | `Icon`, `Check`, `Danger`, `Panel`, `Visibility`, `products`, `menu`, `dropdowns` | Zero uses in the corpus; `Visibility` is pseudo-security by its own admission |

### 5.1 The one hard problem

`Tabs` and `CodeGroup` build their tab strip by **reading their children's
titles**. Most template engines cannot introspect a block — rsx hands a
component `children: Element`, which is opaque markup by the time it arrives.
The Astro port solved it with HTML-comment sentinels and a regex split. Decide
this deliberately before writing the container components; a real slot
mechanism in rsx would make the problem disappear, and that is a change to
rsx, not a workaround here.

### 5.2 Navigation is data, declared once

One tree, outside the content, yielding the sidebar, the tab bar, breadcrumbs,
prev/next, search ordering and the API page set. Mintlify's rule is worth
copying exactly: **one child type per level** — a tab may contain anchors which
contain groups, but never both at once. That rule is why a 489-page site is a
readable 1,100-line file and why the navigation can be validated rather than
merely parsed.

Ship three concepts and stop: **tabs, groups (recursive), and pages**, plus
global anchors for the handful of external links. The 489-page corpus uses
exactly `tabs -> groups -> nested groups -> page strings`, four global anchors
and one external-href tab. Versions and languages are a partition over file
paths plus a dropdown; add them when someone asks.

**A build-order constraint falls out of this.** Navigation lists slugs only —
every title comes from the page's own frontmatter, so a title is never
restated in two places. That means the sidebar cannot be built until all
content is loaded. Order the build: discover content, load frontmatter,
resolve navigation, then render.

Frontmatter obeys the same long tail as the components. Across 489 production
pages: `title` 487, `description` 260, `openapi` 184, `authMethod` 184,
`openapi-schema` 39, `keywords` 8, `sidebarTitle` 6, `mode` 1. Seven keys
carry a real site.

Four callouts, not six: `Note`, `Info`, `Tip`, `Warning`. `Check`, `Danger`
and the generic `Callout` have zero uses across the corpus.

### 5.3 The highest-leverage feature, and it is cheap

Serve every page's Markdown source at `<url>.md`, honour
`Accept: text/markdown`, generate `llms.txt` from the navigation tree, and
emit a `Link:` discovery header. "Copy page", "Open in ChatGPT" and "Open in
Claude" are then three URLs built on that one route. Roughly a week for the
whole bundle. Neither Hugo nor Astro gives you any of it.

### 5.4 Not building

- **An 18-language code-sample generator.** Eighteen codegen backends is a
  permanent tax where every bug is "the Kotlin sample for my multipart endpoint
  is wrong". Borrow `httpsnippet` through its HAR seam, honour `x-codeSamples`,
  move on.
- **The interactive playground.** It needs a server to proxy requests, and we
  do not have one. Degrade to a copyable endpoint.
- **Hosted search.** See §4.3.
- **Browser-evaluated interpolation.** Mintlify's MDX expressions are
  evaluated in the browser, so `{myVar}` never appears in the initial HTML, in
  a markdown export, or to a crawler. Interpolate at build time or not at all.
- **Mintlify's slugging and fence meta.** Anchor ids that keep `'`, `/` and
  `&`, and fences whose title is bare words before the options, break every
  standard Markdown tool. Use GitHub-compatible slugs and `title="..."`, with a
  compatibility flag for migrations.

### 5.5 Scope, measured

A complete Mintlify-compatible layer over unmodified MDX, for a real 489-page
site, came to **~4,500 lines**: 1,100 for 24 components, 1,200 for the layout
shell, 600 for a static OpenAPI reference, 240 for navigation resolution, 200
for compatibility plugins, 333 for styling. That is the honest size of the
target, and the components are the easy third of it.

## 6. Dependencies

Settled so far. The rest of the survey -- search, heading extraction, link
checking, images, OpenAPI, frontmatter validation, HTML minification, routing
-- is still outstanding.

| Need | Take | Cost | Why |
|---|---|---|---|
| Parallel build | `rayon` | +7 | Justified by syntect, not comrak. Measured over 500 pages: comrak alone is 0.12 ms/page, so 60 ms serial and not worth threading. Syntect is 1.39 ms/page -- over 90% of render time -- and rayon takes it from 695 ms to 131 ms. Pages are not uniform (a page with eight code blocks costs ~20x one with none), so work-stealing beats static chunking by roughly 5.3x against 3-4x. |
| File watching | `notify-debouncer-full` | +2 over notify | `full` correlates rename From/To into one event, which is the cure for the write-rename-replace dance vim and JetBrains do. Depend on the debouncer only and use its `notify` re-export, so version skew is impossible. |
| Dev server | hand-written on `tiny_http` + SSE | +4 | About 100 lines, behind a feature flag. SSE over WebSocket because the browser reconnects on its own, which is what you want when the dev server restarts. `axum` + `tower-http` + `tower-livereload` is the better long-term answer at +35 crates -- still only a fifth of what oxc and lightningcss already cost -- so this is reversible, not a one-way door. |
| Progress | `indicatif` | small | Hides itself when piped, so CI needs no flag. Its `rayon` feature composes with the build above. |
| Colour | `anstyle` + `anstream` | 0 new | clap already pulls `anstyle`. Avoid `colored`: MPL-2.0 would be the only copyleft licence in the tree. |
| Diagnostics | `annotate-snippets` | +4 | A span in a file with a message is the only error shape we have. rust-lang owns it and rustc is migrating onto it, so the output is already familiar. `miette` is +60 with `fancy` and earns it only once we want error codes and chained diagnostics. |
| Test assertions | `scraper` | +17, **dev only** | See section 8. |
| Route matching | `wayfind` | +1 | **Not `matchit`, and not axum.** Verified below. |
| Frontmatter errors | `serde_path_to_error` + `annotate-snippets` | +5 | A dotted field path plus a caret, with no trait refactor. |
| Search | `pagefind` (as a library) | +166 | Its `pagefind::api` module is real Rust, so no npm and no subprocess. Per-search cost is ~35 KB and flat in corpus size, where `elasticlunr` emits 18.7 MB of JSON for 500 pages and parses all of it before the first result. The 166 crates are the price; revisit if they hurt. |
| Image dimensions | `imagesize` | +0 | Zero runtime dependencies, reads 16 bytes, prevents layout shift. It has no SVG support, which is ~20 lines of `regex` we already have. |
| ASCII diagrams | `svgbob` | small | Pure Rust, no layout engine. Mermaid stays client-side by default. |

### 6.1 Routing: `matchit` is wrong for us, verified

I recommended `matchit` earlier on the grounds that axum uses it. That was
wrong, and the failure is exactly our case:

```
matchit 0.9.2:  insert("/docs/{slug}")  ok
                insert("/docs/{*rest}") -> Insertion failed due to conflict
```

`[slug].md` beside `[...rest].md` is the sibling pair the file convention
produces, and Next.js resolves it by precedence. matchit refuses it outright.
It also cannot enumerate its routes or report which pattern matched, both of
which a generator wants for sitemaps.

Measured, same pair, correct syntax for each crate:

| | syntax | accepts the pair | `/docs/intro` | `/docs/a/b/c` |
|---|---|---|---|---|
| `matchit` 0.9.2 | `{slug}` `{*rest}` | **no** | — | — |
| `wayfind` 1.1.2 | `<slug>` `<*rest>` | **yes** | `slug` | `rest` |
| `path-tree` 0.8.3 | `:slug` `:rest*` | yes | `slug` | `rest` |

Both survivors give Next.js precedence. Take **`wayfind`**: it reports the
matched template and detects conflicts, where `path-tree`'s `insert` returns
an id and silently overwrites. The risk is that wayfind is small and young, so
keep the matcher behind our own thin interface.

Speed is irrelevant here and should carry no weight: all three are 20-40 ns
per lookup, which across a whole build is about a millisecond.

Three traps recorded from the survey, each of which bites silently:

1. **`tiny_http` buffers SSE into oblivion.** Its chunked encoder uses an 8 KiB
   buffer with no flush, so 14-byte reload frames never leave the process.
   `Request::into_writer()` and write the response by hand.
2. **Never watch the output directory.** Watch an explicit input set. Better
   still, follow Zola and render into memory in dev, so there is no output to
   loop on.
3. **comrak's AST is `!Send`.** Not merely `!Sync` -- it can never cross a
   thread boundary at all. Keep parse and render inside one closure and return
   a `String`. `Options` and the syntect adapter are both `Sync`; build one and
   share it. Do not use `thread_local!` for `SyntaxSet`: it defeats the
   `OnceLock` sharing that makes it cheap.

## 7. The documentation site

The framework's own documentation is the example site and the end-to-end test.
The Crystal project's rule holds: each subsystem adds a page about the feature
it just shipped, written using that feature, and the site's build is part of CI.

Design targets: the landing page after docs.stripe.com, the documentation
pages after tailwindcss.com/docs, with Astro's optically-centred content pane
and three-state theme control, and Hugo's card-grid section index as a third
navigation layer. Tailwind v4, wired through the generic pre-build command
hook so the framework stays unaware of it.

Colour is settled and lives in `site/assets/css/theme.css`. The nine supplied
swatches are pastels at one lightness, so none can carry text; each anchors an
OKLCH ramp instead, keeping its exact hex at the step that already matched its
lightness. Three tiers per hue — `-low` fill, `-base` stroke, `-high` text —
every one checked against WCAG. One hue is the accent and appears wherever
interaction lives; every other hue is bound to exactly one meaning. Hugo's
docs are the counter-example: one blue doing six jobs separates nothing.

## 8. Testing

Playwright, from the first page rather than after the last. Every bug a human
found in the Crystal sites was **between** pages or **at a breakpoint** — dead
links, a doubled navbar, three stacked nav strips, a page whose own JavaScript
printed itself. None of them were visible in a single loaded desktop page,
which is all a naive test asserts about.

So the harness covers: a link crawl over every built page; duplicate-chrome
assertions; the no-JavaScript pass; visual baselines at several widths; and
the partial-navigation claims, asserted by marking live elements and checking
which marks survived a swap.
