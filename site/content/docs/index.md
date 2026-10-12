---
title: What depoth is
description: A static content framework in Rust, built on compiled templates and prerendered partial navigation.
sidebar_title: Overview
---

Two sentences carry the whole design.

**Templates compile; content does not.** A template is Rust. The compiler reads
it, type-checks it and puts it in the binary. A markdown document is data. The
built binary reads it at run time. The two sit on different clocks: a template
edit pays for a compile, a content edit does not.

**There is no backend.** The output is files. Nothing runs at request time, so
any static file server can publish the result.

## What it is for

A documentation site, or any site whose content is markdown and whose shell is
a navigation tree. depoth gives you one configuration file, one navigation
declaration, compiled templates, and a build that writes HTML to a directory.
Navigation is declared once and yields the sidebar, the tab bar, the
breadcrumbs and the prev/next links, so they cannot disagree with each other.

It also prerenders partial navigation. Each route is written as a complete
document and again as a set of smaller trees, so a client with JavaScript can
replace only the part of the page that changed. Every page still works with
JavaScript off, because the document was never derived from the fragment.

## What it is not for

Anything that needs compute when a visitor asks for a page. There is no request
handler, no session, no form endpoint and no database. An interactive API
playground needs a server to proxy requests, so depoth does not have one.

It also does not choose your CSS framework, your bundler or your JavaScript
toolchain. The framework proposes a pre-build command hook and stays out of
what you run in it. This site uses Tailwind; depoth does not know Tailwind
exists.

## Maturity

Early. The repository is at version 0.1.0 and contains the configuration
parser, the navigation model, the frontmatter splitter and the test harness.
The render pipeline, the command-line interface, the component library and
search are not written yet. Where a page below describes something that is not
built, it says so.

The predecessor is real, which is why the decisions are not guesses. The
Crystal depoth reached 0.2.0 with 315 specs and one 489-page production site.
Most of what you read here is inherited from it rather than invented here.
