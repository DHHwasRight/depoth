---
title: Installing depoth
description: A site is a Rust crate, so there is nothing to install globally — you add three dependencies and build.
sidebar_title: Install
---

A depoth site is a Rust crate. There is no `depoth` binary to install on your
machine and no theme to download. You create a crate, add three dependencies,
and the crate builds your site.

## Requirements

A Rust toolchain. The workspace declares a minimum of 1.97, which is what the
dependency tree requires: `oxc_parser` 0.153 asks for 1.97 and edition 2024,
and `comrak` 0.55 asks for 1.85.

Pin the toolchain for the project. `rsx` compiles templates in a procedural
macro, and its macro artifacts are not compatible across patch releases of
rustc. Two different rustc on `PATH` compile the host half and the target half
differently, which fails in a way that does not name the cause.

```toml
# mise.toml
[tools]
rust = "1.98"
```

## A new site

```sh
cargo new --bin my-site
cd my-site
```

Add the three dependencies. All three are git dependencies today; none is
published to crates.io yet.

```toml
# Cargo.toml
[dependencies]
depoth = { git = "https://github.com/DHHwasRight/depoth.git", branch = "main" }
prerender = { git = "https://github.com/DHHwasRight/prerender.git", branch = "main" }
rsx = { git = "https://github.com/DHHwasRight/rsx.git", branch = "main" }
```

`depoth` reads the configuration, the navigation and the content. `rsx`
compiles your `.rsx` templates into the binary. `prerender` renders each route
as a document and as its partial trees, and writes them to disk.

## The layout of a site

```text
my-site/
  Cargo.toml
  depoth.yaml      the site's identity, directories and navigation
  content/         markdown documents
  src/
    main.rs
    views/         .rsx templates, compiled into the binary
  assets/          processed and fingerprinted
  public/          copied verbatim
```

`content`, `dist`, `public` and `assets` are the defaults. Every one of them is
a configuration key, so you can move them. See
[Configuration](/docs/configuration).

## Building

```sh
cargo run -- build
```

The first build compiles every dependency and takes about 21 seconds on a cold
`target/`. After that a template edit rebuilds in about half a second, and a
content edit does not rebuild at all — it re-runs the binary you already have.
[Design notes](/docs/design-notes) has the measurements.

The build command is not implemented in this repository yet; the crate today
carries the configuration parser, the navigation model and the frontmatter
splitter.
