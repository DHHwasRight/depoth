---
title: Layouts and templates
description: Templates are rsx. They compile into the binary, which buys type checking and costs a rebuild on every edit.
sidebar_title: Layouts
---

Templates are [rsx](https://github.com/DHHwasRight/rsx). An `.rsx` file is
Rust — `use` declarations, items, functions — except that a function returning
`Element` has a markup body.

```rust
pub fn Card(title: &str, count: usize) -> Element {
    <div class="card">
        <h2>{title}</h2>
        <span>{count}</span>
    </div>
}
```

A component is a function whose name starts with an uppercase letter, because
that is how the markup tells a component from an HTML element. A parameter of
type `Element` receives children. Visibility, generics and `where` clauses are
ordinary Rust.

## Loading them

```rust
rsx::load_dir!("src/views");       // every .rsx under the directory
rsx::load!("src/views/card.rsx");  // one file
```

Paths are relative to the crate root. There is no `build.rs`: the macro
compiles the templates during expansion and emits an `include_bytes!` for every
file it reads, which is what makes cargo rebuild when a template changes.

## What compiling buys

The template is type checked. A component called with the wrong argument, a
missing field, a renamed struct — all of them are compile errors naming the
line, not a blank region in a rendered page or a run-time panic on the one
route nobody opened.

Escaping is in the type system. `{expr}` renders any `Display` value with HTML
escaping applied, and nothing is allocated to do it: formatting goes into the
output buffer through an escaping adapter. `{! expr}` renders without escaping
and is the only way to emit HTML you built elsewhere, so an audit for XSS is a
grep for one token.

There is no template interpreter to maintain. An interpreter is a second,
weaker implementation of a compiler that already exists.

## What compiling costs

A template edit pays for a compile. Measured on a thirteen-template probe with
a warm `target/`, that edit costs **0.50 s** — four times faster than the
Crystal predecessor's 2.0 s, because the heavy dependencies are compiled once
and only the leaf crate recompiles. A content edit pays nothing, because it
re-runs the binary you already have.

Two honest caveats. Codegen scales with template count, and 0.50 s is a
thirteen-template measurement. And a release build of the probe is 49 s, which
is a deploy cost rather than an authoring one — but it means `--release` must
never be the default for development.

The other cost is harder to price: a template is not editable without a Rust
toolchain. You cannot ship a theme as a folder of files that a reader edits in
place, and you cannot reload a template without a rebuild. depoth takes that
trade deliberately. The alternative is an interpreter, and the clock
measurements say the trade is not needed.

## Layouts

A layout is a component registered against a path prefix, and it wraps every
route beneath that prefix. Rendering folds from the innermost layout outward.
Each layout must mark the element that wraps exactly its children — see
[Partial navigation](/docs/partial-navigation), where that marker does its real
work.
