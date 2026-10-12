---
title: Deploying a site
description: Building for a static host, publishing to GitHub Pages, and serving the partial trees through nginx.
sidebar_title: Deploying
---

The output is files. Any static file server can publish it, and no server
software is required for the site to work. What a server buys you is partial
navigation, which needs a host that can route on a request header.

## Building for release

```sh
cargo run --release -- build
```

Build in release mode for a deploy. On the thirteen-template probe a release
build took 49 s, which is a deploy cost rather than an authoring one — and the
reason `--release` must never be the default during development.

Empty the output directory before writing to it. The predecessor's doc site
shipped two copies of its stylesheet for weeks, an old unfingerprinted one left
behind from when it lived in `public/`.

## A host with no header routing

```sh
cargo run --release -- build --no-partials
```

One tree, an ordinary static site. Navigation degrades to full page loads and
nothing else changes, because every page was already a complete document. A
test asserts the documents are byte-identical either way, so the site you
publish is the site that was tested.

## GitHub Pages

GitHub Pages cannot route on a header, so build with `--no-partials` and
publish the single tree.

```yaml
# .github/workflows/pages.yml
- uses: dtolnay/rust-toolchain@master
  with:
    toolchain: "1.98"
- run: cargo run --release -- build --no-partials
- uses: actions/upload-pages-artifact@v3
  with:
    path: dist
```

Pin the toolchain version. `rsx` compiles templates in a procedural macro and
its artifacts are not compatible across patch releases of rustc, so a runner
that silently moves to a newer toolchain fails in a way that does not name the
cause.

Set `base_url` in `depoth.yaml` to the published origin. Without it there is no
sitemap and no canonical URL.

## nginx

`prerender nginx` emits a complete config, not a snippet, so it runs as
printed.

```sh
cargo run --release -- build
cargo run --release -- nginx --prefix .run --port 8099 > .run/nginx.conf
nginx -p "$PWD/.run" -c "$PWD/.run/nginx.conf"
```

```nginx
map $http_dp_partial $prerender_root {
    default   …/dist;
    ~*^true$  …/dist-partial/1;
    "1"       …/dist-partial/1;
    "2"       …/dist-partial/2;
    "3"       …/dist-partial/3;
}
server {
    root $prerender_root;
    add_header Vary DP-Partial always;
    location / { try_files $uri $uri/index.html @full; }
    location @full { root …/dist; try_files $uri $uri/index.html =404; }
}
```

Four details there are load-bearing rather than decorative.

- The match is `~*` because header values are not the client's to normalise.
- `Vary` is mandatory. Several bodies share one URL, so a cache that saw a
  fragment will otherwise hand it to the next visitor asking for a document.
- A partial request that misses falls through to `@full` rather than to a 404,
  because a route that exists only at a shallower depth should still answer.
- The 404 page is deliberately not a `try_files` candidate. When it was, a
  partial request for a missing page got a whole document back and the client
  spliced a second navbar into the page.

Redirects become exact-match `location` blocks, which nginx resolves before any
prefix location, so they cost nothing on the pages that are not redirects.

The `build` and `nginx` commands are not implemented in this repository yet;
they are prerender's, and depoth passes through to them.
