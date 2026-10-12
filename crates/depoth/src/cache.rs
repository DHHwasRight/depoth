//! A content-hash keyed cache of rendered markdown.
//!
//! Rendering is where a build's time goes. Measured over 500 pages: comrak
//! alone is 0.12 ms per page, and syntect highlighting is 1.39 ms — over 90%
//! of render time. A cache keyed on the document body turns an unchanged page
//! into a hash and a map lookup.
//!
//! **The trap, which the Crystal predecessor hit:** the cache served
//! pre-highlighting HTML after the renderer changed and the bodies did not.
//! A body hash alone is not a correct key, because the output depends on the
//! renderer as much as on the input. So every entry also carries a
//! *signature* — a hash of everything about how rendering happens — and the
//! whole cache is discarded when it changes. Forgetting to bump a version
//! constant by hand is exactly the mistake that caused the bug, so the
//! signature is computed rather than written down.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::markdown::{Heading, Rendered};

/// The format of the cache file itself. Bump when the on-disk shape changes;
/// the renderer's behaviour is covered by the signature instead.
const FORMAT: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry {
    html: String,
    headings: Vec<Heading>,
}

/// Rendered output, keyed by the hash of the source that produced it.
#[derive(Debug)]
pub struct RenderCache {
    signature: String,
    entries: HashMap<String, Entry>,
    /// Keys read during this build. Anything untouched is dropped on save, so
    /// the file does not grow for every page that ever existed.
    used: std::collections::HashSet<String>,
    hits: usize,
    misses: usize,
}

impl RenderCache {
    /// A cache valid only for the given signature.
    ///
    /// Build the signature from everything that can change the output: the
    /// set of registered component names, the highlighting theme, the markdown
    /// options. Anything left out is a correctness bug waiting for the day
    /// someone changes it.
    pub fn new(signature: impl AsRef<[u8]>) -> Self {
        Self {
            signature: hash(signature.as_ref()),
            entries: HashMap::new(),
            used: std::collections::HashSet::new(),
            hits: 0,
            misses: 0,
        }
    }

    /// Loads a cache from disk, discarding it when the signature or the format
    /// does not match. A stale cache is not an error; it is a cold build.
    pub fn load(path: &Path, signature: impl AsRef<[u8]>) -> Self {
        let mut cache = Self::new(signature);
        let Ok(text) = std::fs::read_to_string(path) else {
            return cache;
        };
        let mut lines = text.lines();
        let header = lines.next().unwrap_or_default();
        let Some((format, sig)) = header.split_once(' ') else {
            return cache;
        };
        if format != FORMAT.to_string() || sig != cache.signature {
            return cache;
        }
        let mut current: Option<(String, String, Vec<Heading>)> = None;
        for line in lines {
            if let Some(key) = line.strip_prefix("\u{1}key ") {
                if let Some((k, html, headings)) = current.take() {
                    cache.entries.insert(k, Entry { html, headings });
                }
                current = Some((key.to_string(), String::new(), Vec::new()));
            } else if let Some(h) = line.strip_prefix("\u{1}h ") {
                if let Some((_, _, headings)) = current.as_mut() {
                    if let Some(heading) = parse_heading(h) {
                        headings.push(heading);
                    }
                }
            } else if let Some((_, html, _)) = current.as_mut() {
                if !html.is_empty() {
                    html.push('\n');
                }
                html.push_str(line);
            }
        }
        if let Some((k, html, headings)) = current {
            cache.entries.insert(k, Entry { html, headings });
        }
        cache
    }

    /// A hit, or `None`. Use this when rendering can fail: a cache must never
    /// store a failure, and a closure that cannot return an error forces the
    /// render to happen before the lookup, which makes the cache pure
    /// overhead.
    pub fn get(&mut self, source: &str) -> Option<Rendered> {
        let key = hash(source.as_bytes());
        self.used.insert(key.clone());
        match self.entries.get(&key) {
            Some(entry) => {
                self.hits += 1;
                Some(Rendered {
                    html: entry.html.clone(),
                    headings: entry.headings.clone(),
                })
            }
            None => {
                self.misses += 1;
                None
            }
        }
    }

    /// Records a successful render.
    pub fn insert(&mut self, source: &str, out: &Rendered) {
        let key = hash(source.as_bytes());
        self.used.insert(key.clone());
        self.entries.insert(
            key,
            Entry {
                html: out.html.clone(),
                headings: out.headings.clone(),
            },
        );
    }

    /// The rendered form of `source`, rendering it only on a miss.
    pub fn render<F>(&mut self, source: &str, render: F) -> Rendered
    where
        F: FnOnce(&str) -> Rendered,
    {
        let key = hash(source.as_bytes());
        self.used.insert(key.clone());
        if let Some(entry) = self.entries.get(&key) {
            self.hits += 1;
            return Rendered {
                html: entry.html.clone(),
                headings: entry.headings.clone(),
            };
        }
        self.misses += 1;
        let out = render(source);
        self.entries.insert(
            key,
            Entry {
                html: out.html.clone(),
                headings: out.headings.clone(),
            },
        );
        out
    }

    /// Writes the cache, keeping only what this build used.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut out = format!("{FORMAT} {}\n", self.signature);
        for (key, entry) in &self.entries {
            if !self.used.contains(key) {
                continue;
            }
            out.push_str(&format!("\u{1}key {key}\n"));
            for h in &entry.headings {
                out.push_str(&format!("\u{1}h {} {} {}\n", h.level, h.id, h.text));
            }
            out.push_str(&entry.html);
            out.push('\n');
        }
        std::fs::write(path, out)
    }

    pub fn stats(&self) -> (usize, usize) {
        (self.hits, self.misses)
    }

    pub fn default_path(root: &Path) -> PathBuf {
        root.join(".depoth/cache/render")
    }
}

fn parse_heading(line: &str) -> Option<Heading> {
    let (level, rest) = line.split_once(' ')?;
    let (id, text) = rest.split_once(' ')?;
    Some(Heading {
        level: level.parse().ok()?,
        id: id.to_string(),
        text: text.to_string(),
    })
}

fn hash(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    h.finalize()
        .iter()
        .take(16)
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rendered(html: &str) -> Rendered {
        Rendered {
            html: html.to_string(),
            headings: vec![Heading {
                level: 2,
                id: "x".into(),
                text: "X".into(),
            }],
        }
    }

    #[test]
    fn get_and_insert_let_a_fallible_render_use_the_cache() {
        // The shape a caller needs when rendering returns a Result: look
        // first, render only on a miss, store only a success.
        let mut cache = RenderCache::new("sig");
        assert!(cache.get("# T").is_none());
        cache.insert("# T", &rendered("<h1>T</h1>"));
        assert_eq!(cache.get("# T").expect("hit").html, "<h1>T</h1>");
        assert_eq!(cache.stats(), (1, 1));
    }

    #[test]
    fn a_repeat_is_a_hit_and_is_not_rendered_again() {
        let mut cache = RenderCache::new("sig");
        let mut calls = 0;
        for _ in 0..3 {
            cache.render("# Title", |_| {
                calls += 1;
                rendered("<h1>Title</h1>")
            });
        }
        assert_eq!(calls, 1, "rendered more than once");
        assert_eq!(cache.stats(), (2, 1));
    }

    #[test]
    fn a_different_body_is_a_miss() {
        let mut cache = RenderCache::new("sig");
        cache.render("a", |_| rendered("A"));
        let out = cache.render("b", |_| rendered("B"));
        assert_eq!(out.html, "B");
        assert_eq!(cache.stats(), (0, 2));
    }

    #[test]
    fn a_changed_signature_discards_everything() {
        // The bug this whole module is shaped around: bodies unchanged,
        // renderer changed, stale HTML served.
        let dir = std::env::temp_dir().join("depoth-cache-sig");
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("render");

        let mut old = RenderCache::new("theme=light");
        old.render("# T", |_| rendered("<h1 class=\"light\">T</h1>"));
        old.save(&path).expect("save");

        let mut new = RenderCache::load(&path, "theme=dark");
        let mut rendered_again = false;
        let out = new.render("# T", |_| {
            rendered_again = true;
            rendered("<h1 class=\"dark\">T</h1>")
        });
        assert!(
            rendered_again,
            "a changed signature must invalidate the cache"
        );
        assert!(out.html.contains("dark"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn it_round_trips_through_disk() {
        let dir = std::env::temp_dir().join("depoth-cache-rt");
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("render");

        let mut a = RenderCache::new("sig");
        a.render("# T\n\nbody with\nseveral lines", |_| {
            rendered("<h1>T</h1>\n<p>body with\nseveral lines</p>")
        });
        a.save(&path).expect("save");

        let mut b = RenderCache::load(&path, "sig");
        let out = b.render("# T\n\nbody with\nseveral lines", |_| {
            panic!("should have been a hit")
        });
        assert_eq!(out.html, "<h1>T</h1>\n<p>body with\nseveral lines</p>");
        assert_eq!(out.headings.len(), 1);
        assert_eq!(out.headings[0].id, "x");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_entry_nothing_used_is_dropped_on_save() {
        // Otherwise the file grows for every page that ever existed.
        let dir = std::env::temp_dir().join("depoth-cache-prune");
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("render");

        let mut a = RenderCache::new("sig");
        a.render("kept", |_| rendered("K"));
        a.render("dropped", |_| rendered("D"));
        a.save(&path).expect("save");

        let mut b = RenderCache::load(&path, "sig");
        b.render("kept", |_| panic!("should be a hit"));
        b.save(&path).expect("save");

        let mut c = RenderCache::load(&path, "sig");
        let mut missed = false;
        c.render("dropped", |_| {
            missed = true;
            rendered("D")
        });
        assert!(missed, "an unused entry should not survive a save");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_corrupt_file_is_a_cold_build_rather_than_an_error() {
        let dir = std::env::temp_dir().join("depoth-cache-corrupt");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        let path = dir.join("render");
        std::fs::write(&path, "nonsense that is not a cache").expect("write");
        let cache = RenderCache::load(&path, "sig");
        assert_eq!(cache.stats(), (0, 0));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
