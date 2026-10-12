//! Makes the deployment base path available to the templates.
//!
//! GitHub Pages serves a project site under `/<repo>/`, not at the root, so
//! every absolute URL in the output needs that prefix -- and needs to not have
//! it when the same site is served locally. `env!` refuses to compile when the
//! variable is unset, so it is always set here, to the empty string by default.
fn main() {
    let base = std::env::var("DEPOTH_BASE").unwrap_or_default();
    let base = base.trim_end_matches('/');
    println!("cargo:rustc-env=DEPOTH_BASE={base}");
    println!("cargo:rerun-if-env-changed=DEPOTH_BASE");
}
