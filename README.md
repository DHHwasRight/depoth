<p align="center">
  <img src="https://raw.githubusercontent.com/DHHwasRight/.github/main/profile/logo.png" width="96" alt="">
</p>

<h1 align="center">depoth</h1>

<p align="center">A static content framework in Rust. Templates compile; content does not.</p>

> **Status:** The DHHwasRight org is named after RailsConf 2026. It is an
> attempt to implement DHH's claims (HTML over the wire, no build step, one
> database) in Rust, to see how far that gets. This project is mostly vibe
> coded: an LLM wrote most of it. It is a thought exercise. We use it for
> internal products where mission criticality is not needed.

## What it is

A depoth site is a Rust crate. Its templates are [rsx](https://github.com/DHHwasRight/rsx)
files that compile into the site's binary. Its content is Markdown with YAML
frontmatter, which that binary reads at run time. The output is a directory of
static files. There is no backend.

## Why

The design keeps two build clocks apart:

- A template edit pays a `cargo` compile. Measured on a 13-template probe:
  0.5 s on a warm `target/`.
- A content edit only re-runs the binary that is already built.

So a content author never waits for the compiler. `PLAN.md` §4.1 has the
numbers.

Other decisions, each with its reason in `PLAN.md` §2:

- Every page works with JavaScript off. A partial is never the only form of a
  page.
- The navigation is one YAML tree in `depoth.yaml`. It names slugs only. Each
  title comes from the page's own frontmatter.
- `assets/` is processed and fingerprinted. `public/` is copied verbatim.
- Syntax highlighting happens at build time (syntect).
- The build also writes a sitemap, a search index, `llms.txt`, and the
  Markdown source of each page at `<url>.md`.

## Quick start

You need Rust 1.97 or later. Pin one toolchain for the site (the repo pins
1.98 in `mise.toml`): rsx compiles templates in a procedural macro, and its
artifacts are not compatible across rustc patch releases.

```sh
cargo install --git https://github.com/DHHwasRight/depoth depoth-cli

depoth new my-site --layout docs --no-git
cd my-site
depoth routes   # print the navigation tree
depoth build    # compile the templates, render the content into dist/
depoth check    # check the content and the built output
```

`depoth new` asks for anything you do not pass as a flag. The generated
`Cargo.toml` takes `depoth`, `prerender` and `rsx` as git dependencies; none
is on crates.io yet. The first `depoth build` compiles the whole dependency
tree, which takes a while.

`depoth dev` exists but does not serve yet. It prints what the dev server will
do.

### Work on depoth itself

```sh
mise run ci      # cargo build, clippy -D warnings, cargo test
```

The repository has five workspace members:

| Path | What it is |
|---|---|
| `crates/depoth` | The library: config, frontmatter, Markdown, navigation, routes, assets, images, feeds, search, render cache. |
| `crates/depoth-cli` | The `depoth` binary. |
| `crates/depoth-test` | Assertions over a built site: dead links, dead assets, duplicated chrome. `depoth check` runs them. Test support only. |
| `site` | The documentation site. It is also the end-to-end test. |
| `probe` | The template compile-time measurement behind `PLAN.md` §4.1. |

To build and check the documentation site the way CI does:

```sh
(cd site && npm ci && npm run css)   # Tailwind stylesheet
cargo run -p depoth-site -- build --no-partials --out site/dist
cargo test -p depoth-test --test site
```

If you build with `DEPOTH_BASE=/depoth`, set the same variable for the test.
The check resolves links against that base path.

## How it fits with the sibling projects

- **[rsx](https://github.com/DHHwasRight/rsx)**: the template compiler. A
  dependency of `depoth` and of every generated site.
- **[prerender](https://github.com/DHHwasRight/prerender)**: layouts and the
  per-depth partial trees. A dependency of `depoth` and of every generated
  site.
- **[htmx](https://github.com/DHHwasRight/htmx)** (dp-swap): the client for
  partial navigation. depoth writes the partial trees; dp-swap fetches and
  swaps them in the browser. depoth does not ship dp-swap. A site adds it.
  See [Partial navigation](https://dhhwasright.github.io/depoth/docs/partial-navigation).
- **[brocanth](https://github.com/DHHwasRight/brocanth)**: the server
  framework. It does not depend on depoth. It reuses two of depoth's
  conventions: the `assets/` and `public/` split, and the `_` prefix for an
  input file that is never published.

depoth does not use jt, zod or strata.

## Docs

- Documentation site: <https://dhhwasright.github.io/depoth/>. Its source is
  `site/content/docs/`.
- [`PLAN.md`](PLAN.md): the design, the decisions carried over from the
  Crystal depoth, the open questions, and the dependency survey.

## License

No license yet.
