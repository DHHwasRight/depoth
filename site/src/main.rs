//! depoth's own documentation, built with depoth.
//!
//! The site is the end-to-end test: it is rebuilt on every change and its
//! output is checked by the same harness a user's site would use.

mod content;
mod data;

use std::path::Path;
use std::process::ExitCode;

use depoth::config::Config;

fn main() -> ExitCode {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let config = match Config::load(&root.join("depoth.yaml")) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    let docs = match content::load(&root.join(&config.content_dir)) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    let entries = config.navigation.flatten();
    if let Err(problems) = content::check_navigation(&docs, &entries) {
        for p in problems {
            eprintln!("error: {p}");
        }
        return ExitCode::FAILURE;
    }

    println!(
        "loaded {} pages, {} in the navigation",
        docs.len(),
        entries.len()
    );
    ExitCode::SUCCESS
}
