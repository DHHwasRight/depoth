---
title: Testing a built site
description: Six rules asserted over the output HTML in process, and the three claims that still need a browser.
sidebar_title: Testing
---

The output of a static site generator is HTML on disk, so almost every claim
worth making about it can be checked by reading that HTML. `depoth-test` does
that, in process, with no browser.

```rust
use depoth_test::{assert_clean, Policy, Site};

#[test]
fn the_site_is_clean() {
    let site = Site::open("dist").expect("built");
    assert_clean(&site, &Policy::default());
}
```

`Site::open` reads every `.html` file under the root and indexes every other
file so asset links can be resolved. `check` returns findings rather than
panicking at the first one, because a harness that stops at the first finding
turns one build into ten runs.

## The six rules

The rules are not invented. Each one is a bug that reached a human in an
earlier generation of this project, and every one of them was **between** pages
rather than inside one.

| Rule | What it asserts | Why it exists |
|---|---|---|
| `dead-link` | Every internal `href` resolves to a file that was built. | A page was renamed and the links to it were not. The commonest failure by a distance. |
| `dead-asset` | Every `img`, `script`, `link` and `source` target exists. | The same rename, applied to a stylesheet or an image instead of a page. |
| `duplicate-chrome` | Selectors that must match exactly once, match exactly once. | A doubled navbar, and three stacked nav strips. A count that should be one and is two. |
| `incomplete-document` | Each page has a doctype, a `lang`, a non-empty `<title>` and exactly one `h1`. | A fragment written to a route a browser can navigate to — which is what produced the doubled navbar upstream of it. |
| `links-into-a-partial` | No `href` points at a fragment. | A fragment is fetched, never navigated to. A link to one hands the reader a page with no shell around it. |
| `headings` | No heading level is skipped. | The outline is the document's structure for anything that cannot see the page. |

`Policy` is what "correct" means for one site: which selectors must be unique,
which routes are allowed to be partial, and which link prefixes to leave alone.
The default requires exactly one `body > header` and one `main` on every page.

External links, `mailto:`, `tel:`, `data:` and bare fragments are left alone.
Relative links resolve against the page's own directory.

`Site::titles` groups pages by their `<title>`, which is how a template that
stamped one title onto everything shows up.

<Warning title="A check that cannot fail is not coverage">
The base-path rule shipped once with its `Policy` field wired up and the code
that used it missing, so it stored a value and did nothing. Every rule here has
a test that plants the defect it looks for.
</Warning>

## Why it runs in milliseconds

There is no browser process, no HTTP server, no page load and no screenshot
comparison. The harness parses HTML in process and walks it with a CSS selector
engine. A browser suite pays a process launch and a real page load per
navigation, which is why it is measured in seconds, and it proves nothing about
these six rules that reading the files does not already prove.

The one dependency is `scraper` — html5ever's parser with a complete selector
engine, seventeen crates that a site's own binary never links. It is bought
deliberately: a harness that mis-parses HTML gives false confidence rather than
no confidence.

Each rule has a test with a fixture carrying exactly the bug it looks for. A
check that cannot fail is worse than no check, because it reads as coverage.

## What is left to Playwright

Three kinds of claim need a real browser, and they are run from the first page
rather than after the last:

- Layout at a breakpoint. Nothing about the DOM tells you two elements overlap
  at 390 pixels.
- A swap actually happening. The partial-navigation claims are asserted by
  marking live elements and checking which marks survived the swap, which
  requires a live DOM across a navigation.
- A visual baseline, at several widths.

The no-JavaScript pass also belongs there, because turning JavaScript off is a
browser setting. That every page works with JavaScript off is a test, not a
guideline.
