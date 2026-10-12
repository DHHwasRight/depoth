---
title: Design notes
description: The decisions worth knowing — the two clocks, the type as the schema, propose rather than choose, and no backend.
sidebar_title: Design notes
---

Each decision below was argued and shipped in the Crystal predecessor, which
reached 0.2.0, 315 specs and one 489-page production site. They are carried
forward rather than invented here, which is why the reasoning is specific.

## The two clocks

A template edit pays for a compile. A content edit re-runs the already-built
binary. Keeping those on separate clocks is the point of compiling templates at
all: a content author must never wait for a compiler.

The open risk in moving to Rust was that the compile clock would leave
interactive range. It did not. Measured on a thirteen-template probe — a
layout, control flow, a component call — with a warm `target/`:

| | |
|---|---|
| Cold build, every dependency | 21.3 s |
| **Template edit, rebuild** | **0.50 s** |
| No-op rebuild | 0.36 s |
| **Re-run the built binary** | **0.16 s** |
| Release build | 49.0 s |

A template edit is four times faster than the Crystal predecessor's 2.0 s,
because the heavy dependencies — comrak, oxc, syntect — compile once and only
the leaf crate recompiles. No fallback is needed and the design stands as
written.

Two caveats stay on the record. Codegen scales with template count, so 0.50 s
is a thirteen-template number and must be re-measured at the size of a real
site. And the release build is 49 s, which is a deploy cost — but it is the
reason `--release` is never the default for development.

## The type is the schema

A page's frontmatter is deserialised into a Rust struct. The struct is the
schema; there is no schema file.

Generating structs from a schema file is circular: reading the schema means
compiling it, and its output is what needs compiling. Deriving instead gives
coercion, defaults and error messages for free, and an unknown key is rejected
by the same derive that reads the known ones. A typo that is quietly ignored
costs more time than any error message does.

## Compile templates, do not interpret them

An interpreter is a second, weaker implementation of a compiler that already
exists. See [Layouts](/docs/layouts) for what that buys and what it costs.

## Propose, don't choose

Choosing a CSS framework or a bundler is the site's decision, not the
framework's. depoth offers a pre-build command hook and stays out of what you
run in it. Grep the Crystal source for "tailwind" and you get nothing; this
documentation site uses Tailwind, and the framework does not know it exists.

The same rule applies to markdown extension, ranked by blast radius: a render
hook first, because it takes markup out of every document at once, and a
component call inside a document last.

## No backend, ever

The output is files. No request handling, no compute at request time. That
constraint is what makes partial navigation prerendered rather than served —
every form of every page is written at build time, and the server only chooses
between them.

It is also why there is no interactive API playground: that needs a server to
proxy requests, and we do not have one. It degrades to a copyable endpoint.

## Every page works with JavaScript off

A fragment is never the only representation of a page. This is a test, not a
guideline — and it is the same property that makes `--no-partials` a one-line
change rather than a second rendering path.

## Build order

Navigation lists slugs only; every title comes from the page's own frontmatter.
That means the sidebar cannot be built until all content is loaded, which fixes
the order: discover content, read frontmatter, resolve navigation, render.
