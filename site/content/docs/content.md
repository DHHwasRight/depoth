---
title: Writing content
description: Frontmatter keys, the markdown body, and how a mistake in a header is reported with a file, a line and a column.
sidebar_title: Content
---

A document is a YAML header between `---` fences, followed by markdown. It
lives under `content_dir` and its path, minus the extension, is the slug the
navigation names.

```markdown
---
title: Installing depoth
description: One sentence, used as the page subtitle and the meta description.
sidebar_title: Install
---

Prose starts here. The title supplies the `h1`, so the body starts at `##`.
```

## Frontmatter keys

| Key | Required | Used for |
|---|---|---|
| `title` | yes | The page heading, the `<title>`, and the navigation label. |
| `description` | no | The page subtitle and the meta description. |
| `sidebar_title` | no | A shorter label for the sidebar, when the title is too long for it. |

The navigation label is `sidebar_title` when it is present and `title`
otherwise. Nothing in the navigation declaration repeats either of them.

Three keys is not an accident. Across 489 production pages, `title` appeared
487 times, `description` 260 and a sidebar title 6. A small number of keys
carries a real site.

## The body

The body is markdown, rendered at build time. Headings get GitHub-compatible
anchor ids and are collected into the on-this-page rail with their level, their
id and their text.

Code fences are highlighted at build time, so no highlighter is sent to the
browser. A site with no code on it ships no CSS for code.

## A document with no header

Not an error. It is a document with no header, and the body starts at line 1.

An opening `---` with no closing `---` is also left as a body in full. Guessing
that the author meant a header would hide the real mistake, which is the
missing fence.

A `---` thematic break inside the body is not a closing fence, because the
closing fence is only the first line that is exactly `---`. A byte-order mark
at the start of the file does not hide the opening fence either.

## How errors are reported

A frontmatter mistake is reported with the file, the line, the column and the
field:

```text
guide/install.md:3:9  weight: invalid type: string "3", expected u64
```

The line is counted from the start of the file, not from the start of the
header. The field is the dotted path serde walked to reach the failure, which
still names it when the failure is nested inside a sequence or a map. An
unknown key names the alternatives, so `titel` suggests `title`.

## Why depoth splits the fences itself

Splitting `---` fences is about ten lines. Every frontmatter crate does it, and
every one of them hands back the YAML and throws away where the YAML started.

That byte offset is the whole difference between

```text
line 2 column 9: invalid type: string "3", expected u64
```

and the message above. The first one is relative to a block the author cannot
see — they are looking at a file, and the header has an opening fence the
parser already removed. Keeping the offset costs ten lines and one dependency
saved, and it turns every frontmatter error from a hunt into a jump.
