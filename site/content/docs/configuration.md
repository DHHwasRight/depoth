---
title: Configuration
description: depoth.yaml, key by key — identity, directories, navigation, redirects, commands and bundles.
sidebar_title: Configuration
---

One file, `depoth.yaml`, owns the whole shell: identity, directories,
navigation, redirects. It sits at the crate root.

## Why YAML and not TOML

Two reasons, and both are practical.

The navigation tree is recursive. Groups nest inside groups, and real sites
nest five deep. TOML can express that, but only through repeated table-array
headers that are unreadable at that depth and give no visual cue about where
you are in the tree. YAML's indentation is the tree.

Frontmatter is already YAML. Using YAML for configuration too means a project
carries one parser and one set of quoting rules rather than two.

## A complete example

```yaml
name: Depoth
description: A static content framework in Rust.
base_url: https://depoth.dev

content_dir: content
output_dir: dist
public_dir: public
assets_dir: assets

navigation:
  tabs:
    - tab: Documentation
      groups:
        - group: Getting started
          root: docs/index
          pages:
            - docs/install
            - docs/configuration
        - group: Reference
          icon: book
          pages:
            - docs/navigation
            - docs/content
            - group: Advanced
              expanded: true
              pages:
                - docs/partial-navigation
                - docs/layouts
    - tab: Changelog
      pages:
        - changelog/index
    - tab: Source
      href: https://github.com/DHHwasRight/depoth
  anchors:
    - anchor: GitHub
      href: https://github.com/DHHwasRight/depoth
      icon: github

redirects:
  - from: /getting-started
    to: /docs/install

commands:
  - run: npx @tailwindcss/cli -i assets/css/theme.css -o assets/css/site.css
  - run: npx @tailwindcss/cli -i assets/css/theme.css -o assets/css/site.css --minify
    mode: production

bundles:
  - output: css/site.css
    sources:
      - css/theme.css
      - css/code.css
    mode: production
```

## Identity

| Key | Type | Default | Meaning |
|---|---|---|---|
| `name` | string | required | Shown in the navbar and appended to every page title. |
| `description` | string | none | The site's own description. |
| `base_url` | string | empty | Absolute origin. Without it there is no sitemap and no canonical URL. |

## Directories

Each is a path relative to the crate root.

| Key | Default | Meaning |
|---|---|---|
| `content_dir` | `content` | Markdown documents. |
| `output_dir` | `dist` | Where the build writes. |
| `public_dir` | `public` | Copied verbatim, unprocessed. |
| `assets_dir` | `assets` | Read, but not yet processed. See below. |

<Warning title="assets_dir is read, not processed">
There is no asset pipeline yet. The key is parsed and a site may name a
directory, but nothing fingerprints, minifies or bundles its contents. This
site builds its one stylesheet with the Tailwind CLI and publishes it by hand.
Fingerprinting, bundling and subresource integrity are planned, not present.
</Warning>

A file whose name starts with `_` is an input by convention. It is bundled into
whatever imports it and is never published on its own. Today that convention is
enforced by the site's own build script, not by depoth.

## navigation

Required. The tree is large enough to have its own page — see
[Navigation](/docs/navigation).

## redirects

A list of `from` and `to` pairs. Both keys are required.

```yaml
redirects:
  - from: /old/path
    to: /docs/new-path
```

## commands

Shell commands run before assets are collected. The framework proposes the hook
and stays out of what it runs. This is where a site wires up Tailwind, or
anything else, without depoth knowing it exists.

| Key | Type | Meaning |
|---|---|---|
| `run` | string | The command line. |
| `mode` | `development` or `production` | When to run it. Absent means both. |

## bundles

Published assets concatenated into one file. A bundle consumes its sources, so
nothing leaves a stray copy behind in the output.

| Key | Type | Meaning |
|---|---|---|
| `output` | string | Path of the bundled file. |
| `sources` | list of strings | The files to concatenate, in order. |
| `mode` | `development` or `production` | When to bundle. Absent means both. |

## Unknown keys are errors

Every table in the configuration rejects keys it does not know. `base_ur1`
fails the build and names itself in the message. A typo that is quietly ignored
costs more time than any error message does.

The navigation tree is validated as soon as the file parses, before any content
is read. A malformed tree is therefore one error naming one node, not a cascade
of missing-page errors.
