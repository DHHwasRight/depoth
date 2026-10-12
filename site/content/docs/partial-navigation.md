---
title: Partial navigation
description: Every route is prerendered as a document and as smaller trees, so a navigation replaces only what changed.
sidebar_title: Partial navigation
---

Every route is built several times: once as a full document, and once for each
number of layout layers a visitor might already be holding. With JavaScript, a
click fetches the smallest form that still covers what changed and swaps it in.
Without JavaScript — a crawler, a reader who turned it off, the first load of
any page — the browser gets the document.

Neither form is derived from the other at run time. The server stays a static
file server. This is [prerender](https://github.com/DHHwasRight/prerender) on
the build side and [dp-swap](https://github.com/DHHwasRight/htmx) on the client
side.

```text
dist/delta/card-6/activity/index.html             2932 bytes   <!DOCTYPE> down
dist-partial/1/delta/card-6/activity/index.html   2468 bytes   below the shell
dist-partial/2/…                                  1791 bytes   below the section
dist-partial/3/…                                   425 bytes   one panel
```

## dp-layout markers

A layout must mark the element that wraps **exactly** its children with the
prefix it was registered under.

```rust
pub fn Docs(layout: &str, children: Element) -> Element {
    <div class="docs">
        <aside>…sidebar…</aside>
        <div dp-layout={layout}>{children}</div>
    </div>
}
```

That marker is the whole trick. A partial contains every layout below the one
it starts at, because the server does not know where the visitor came from. The
client does — the old DOM is right there. dp-swap walks both trees down their
`dp-layout` markers, finds the last layer whose prefix matches on both sides,
and replaces only what is below it.

| navigation | deepest shared layer | what is replaced |
|---|---|---|
| `/docs/guides/install` → `/docs/guides/deploy` | `/docs/guides` | the guide body |
| `/docs/reference` → `/docs/guides/install` | `/docs` | the whole guides section |
| `/about` → `/docs/guides/install` | none | the entire outlet |

Put the marker on the element wrapping exactly the children and nothing else.
Anything a layout renders inside the marked element is replaced along with
them, so a sidebar inside the marker loses its scroll position on every
navigation — which looks like a working site that flickers rather than one that
breaks. `prerender check` fails the build when a layout forgets its marker or
drops its children. It cannot catch a marker in the wrong place.

## Asking for less

The client counts the layers it already holds and sends the number:

```text
GET /delta/card-6/activity      DP-Partial: 3
```

The server answers from `dist-partial/3`. Nothing is configured for this and
there is no manifest: the markers in the live document name the prefixes the
current page sits under, and the build writes one tree per depth up to the
deepest layout chain in the site. A server that does not understand the number
can treat any value as "give me the fragment" and serve depth 1.

All the forms live at the same URL. A request carrying `DP-Partial` is served
from a partial tree; everything else gets the document.

## Chrome that is not swapped

The deeper the swap, the more of the page keeps its old state — including the
sidebar link still highlighted for the section you left. A layout can be
registered with a second renderer whose output is applied out of band. It emits
elements carrying `dp-swap-oob` and an `id`, and the client applies them by
`id` before the main swap. Only the deepest skipped layout contributes
out-of-band markup to a given partial, so a response never carries replacements
for chrome the client is about to discard.

## --no-partials

Partial navigation needs a server that can route on a request header. Some
hosts have none — GitHub Pages among them — and there the partial trees are
bytes nothing can ever ask for.

```sh
cargo run -- build --no-partials
```

One tree, an ordinary static site. Navigation degrades to full page loads and
nothing else changes, because every page was already a complete document —
which is the same property that makes the site work with JavaScript off.

The switch is not a second rendering path. A test asserts the documents are
byte-identical either way, so the site you publish is the site that was tested.
