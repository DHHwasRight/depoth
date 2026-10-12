//! The built documentation site, checked by the same harness a user's site
//! would use. The site is the end-to-end test.

use std::path::Path;

use depoth_test::{Policy, Site};

#[test]
fn the_documentation_site_is_clean() {
    let dist = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../site/dist");
    if !dist.join("index.html").is_file() {
        eprintln!(
            "skipping: run `cargo run -p depoth-site -- build --no-partials --out site/dist` first"
        );
        return;
    }
    let site = Site::open(&dist).expect("open");
    let policy = Policy {
        unique: vec!["body > header".into(), "main".into()],
        // Matches the DEPOTH_BASE the site was built with, so the artefact
        // that ships is the artefact that is checked.
        base: std::env::var("DEPOTH_BASE").unwrap_or_default(),
        ..Policy::default()
    };
    let findings = site.check(&policy);
    assert!(
        findings.is_empty(),
        "{} finding(s):\n{}",
        findings.len(),
        findings
            .iter()
            .map(|f| format!("  {f}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}
