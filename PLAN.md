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

Ship five node shapes and stop: groups, tabs, anchors, global anchors, and an
external `href`. The real corpus uses exactly those.

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

*Pending the crate survey.*

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
