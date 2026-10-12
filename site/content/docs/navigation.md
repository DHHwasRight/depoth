---
title: Navigation
description: One tree, declared outside the content, yielding the sidebar, the tab bar, breadcrumbs and prev/next.
sidebar_title: Navigation
---

Navigation is one tree, declared in `depoth.yaml`, outside the content. From it
come the sidebar, the tab bar, the breadcrumbs and the prev/next links. They
are the same data read four ways, so they cannot disagree.

The tree names slugs only. Every title comes from the page's own frontmatter,
so a title is never written in two places. That has a consequence for the
build: the sidebar cannot be built until the content is loaded. The build order
is discover content, read frontmatter, resolve navigation, render.

## The one-child-kind rule

**A node carries exactly one kind of child.** A tab holds groups or pages,
never both. The root holds tabs, or groups, or pages, never two of them.

The rule is Mintlify's and it is worth copying exactly. It is why a 489-page
site stays a readable 1,100-line file: at every level you are reading a list of
one thing, not a mixture you have to sort by eye. It is also why the tree can
be validated rather than merely parsed — a node with two kinds of child has no
single correct rendering, and guessing one produces a sidebar that is wrong in
a way nobody notices.

Three concepts exist and no more: tabs, groups (which nest), and pages. Global
anchors handle the handful of external links. A 489-page production corpus used
exactly that — `tabs` to `groups` to nested groups to page strings, four global
anchors and one external-href tab.

## The root

Exactly one of `tabs`, `groups` or `pages`. `anchors` may accompany any of
them, because an anchor is a link rather than a container.

```yaml
navigation:
  groups:
    - group: Guide
      pages: [docs/index, docs/install]
  anchors:
    - anchor: GitHub
      href: https://github.com/DHHwasRight/depoth
```

Declaring none is an error. Declaring two is an error that names both.

## Tabs

| Key | Type | Meaning |
|---|---|---|
| `tab` | string | Required. The label. |
| `icon` | string | Optional icon name. |
| `href` | string | Makes the tab a plain external link rather than a container. |
| `groups` | list of groups | Optional. |
| `pages` | list of nodes | Optional. |

A tab with neither pages nor groups nor an `href` is an error: it is a control
that cannot go anywhere. A tab that is only an `href` is allowed and is the
normal way to put a link to a source repository in the tab bar.

## Groups

| Key | Type | Default | Meaning |
|---|---|---|---|
| `group` | string | required | The label. |
| `icon` | string | none | Optional icon name. |
| `tag` | string | none | A short badge beside the group name. |
| `root` | slug | none | Clicking the group title opens this page. |
| `expanded` | boolean | `false` | Whether a nested group starts open. |
| `pages` | list of nodes | empty | The children. |

A nested group is an accordion and is closed unless `expanded` is set. A
top-level group is always open. That is Mintlify's rule and a good one: the
outermost level is a section heading, not a control.

A group's `root` page is placed before its children, which is the order a
reader meets them.

## Pages

An entry under `pages` is either a slug or a nested group. Nothing else. In
particular there is no external link here, because a link belongs to a tab or
to an anchor.

```yaml
pages:
  - guide/welcome
  - group: Self-hosted
    pages: [guide/self-hosted/overview, guide/self-hosted/docker]
  - guide/cloud
```

## Anchors

| Key | Type | Meaning |
|---|---|---|
| `anchor` | string | Required. The label. |
| `href` | string | Required. External URL. |
| `icon` | string | Optional icon name. |

Anchors are shown on every page regardless of section.

## What flattening produces

The tree is flattened once into a list of entries in reading order. Each entry
carries its slug, the tab it sits under, the trail of enclosing group names
outermost first, and its index in the whole order.

The trail is the breadcrumb. The index is the prev/next order. The sidebar is
the same walk, stopped at the current tab. `llms.txt` is the same list again.
One flatten, four readers.

## A page listed twice is rejected

Validation fails if any slug appears more than once in the flattened order.

Two sidebar entries for one page makes prev/next ambiguous — the reader can
arrive at the page from one position and leave from the other — and it makes
the active-state highlight flicker between the two entries. There is no answer
that is right, so the tree is refused instead.

## Prev/next stops at a tab boundary

The page before and after a slug are its neighbours **within its own tab**. The
last page of a tab has no next page, even though another page follows it in the
flat order.

A tab is a section a reader chose to be in. Walking them out of it with a
"Next" button is a navigation they did not ask for, and it lands them somewhere
with a different sidebar and no way back along the trail they were following.
If the site has no tabs, every page is in the same `None` tab and prev/next
runs end to end.
