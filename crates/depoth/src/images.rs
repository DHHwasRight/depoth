//! Images that live beside the document that references them.
//!
//! The content loader walks the tree for `.md` files and skips everything
//! else, so a screenshot sitting next to the page that uses it is silently
//! dropped. This module finds those files, resolves the references to them,
//! and stamps intrinsic dimensions onto the markup so the page stops jumping
//! as images load.
//!
//! **Out of scope, deliberately**: resizing, format conversion and responsive
//! variants. The `image` crate with default features is 121 crates and 37
//! seconds of build time, and the only pure-Rust route to "smaller than JPEG"
//! is AVIF at 100-230 ms per variant. That is a later, feature-gated decision.
//! What is here is the part that is free: `imagesize` has no runtime
//! dependencies and reads about sixteen bytes per file.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A file found beside the content, and where it should be published.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asset {
    pub source: PathBuf,
    /// Absolute URL path, mirroring the file's position under the content root.
    pub url: String,
}

/// Intrinsic size in CSS pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Size {
    pub width: u32,
    pub height: u32,
}

/// Every non-markdown file under `root`, with the URL it should take.
///
/// The caller copies them; this module does not, because the output directory
/// is not its business.
pub fn colocated(root: &Path, base_path: &str) -> std::io::Result<Vec<Asset>> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        if !dir.exists() {
            continue;
        }
        for entry in std::fs::read_dir(&dir)? {
            let path = entry?.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().is_some_and(|e| e == "md") {
                continue;
            }
            let rel = path.strip_prefix(root).expect("under root");
            let url = format!("{base_path}/{}", rel.to_string_lossy().replace('\\', "/"));
            out.push(Asset { source: path, url });
        }
    }
    out.sort_by(|a, b| a.url.cmp(&b.url));
    Ok(out)
}

/// The intrinsic size of an image file.
///
/// SVG is handled here rather than by `imagesize`, which does not read it —
/// and a documentation site is mostly SVG.
pub fn size(path: &Path) -> Option<Size> {
    if path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("svg"))
    {
        return svg_size(&std::fs::read_to_string(path).ok()?);
    }
    let dim = imagesize::size(path).ok()?;
    Some(Size {
        width: dim.width as u32,
        height: dim.height as u32,
    })
}

/// Reads width and height off an `<svg>` root.
///
/// Three real shapes, in order of preference:
/// explicit `width`/`height`; a `viewBox` with neither; and a percentage
/// width, which carries no intrinsic size at all — reported as `None` rather
/// than guessed, because a wrong number is worse than no number.
pub fn svg_size(source: &str) -> Option<Size> {
    let open = source.find("<svg")?;
    let end = source[open..].find('>')? + open;
    let tag = &source[open..end];

    let w = attr(tag, "width").and_then(length);
    let h = attr(tag, "height").and_then(length);
    if let (Some(width), Some(height)) = (w, h) {
        return Some(Size { width, height });
    }
    // A percentage width means the size comes from the parent; there is no
    // intrinsic size to report.
    if attr(tag, "width").is_some_and(|v| v.trim_end().ends_with('%')) {
        return None;
    }
    let view_box = attr(tag, "viewBox")?;
    let nums: Vec<f64> = view_box
        .split([' ', ','])
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse().ok())
        .collect();
    if nums.len() != 4 || nums[2] <= 0.0 || nums[3] <= 0.0 {
        return None;
    }
    Some(Size {
        width: nums[2].round() as u32,
        height: nums[3].round() as u32,
    })
}

fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let mut from = 0;
    while let Some(rel) = tag[from..].find(name) {
        let at = from + rel;
        let before_ok = at == 0 || tag.as_bytes()[at - 1].is_ascii_whitespace();
        let rest = &tag[at + name.len()..];
        let rest_trimmed = rest.trim_start();
        if before_ok && rest_trimmed.starts_with('=') {
            let after_eq = rest_trimmed[1..].trim_start();
            let quote = after_eq.chars().next()?;
            if quote == '"' || quote == '\'' {
                let close = after_eq[1..].find(quote)? + 1;
                return Some(&after_eq[1..close]);
            }
        }
        from = at + name.len();
    }
    None
}

/// `"120"`, `"120px"` and `"90pt"` all carry an intrinsic size; a percentage
/// does not.
fn length(value: &str) -> Option<u32> {
    let v = value.trim();
    if v.ends_with('%') {
        return None;
    }
    let (number, unit) = v.split_at(v.find(|c: char| c.is_alphabetic()).unwrap_or(v.len()));
    let n: f64 = number.trim().parse().ok()?;
    let px = match unit.trim() {
        "" | "px" => n,
        // The CSS absolute units, which real exports do use.
        "pt" => n * 96.0 / 72.0,
        "pc" => n * 16.0,
        "in" => n * 96.0,
        "cm" => n * 96.0 / 2.54,
        "mm" => n * 96.0 / 25.4,
        _ => return None,
    };
    if px <= 0.0 {
        return None;
    }
    Some(px.round() as u32)
}

/// An image reference the markup made that could not be resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unresolved {
    pub page_url: String,
    pub src: String,
}

/// Stamps `width` and `height` onto every `<img>` whose source resolves and
/// that does not already declare them.
///
/// A browser that knows the intrinsic size reserves the space, and the page
/// stops reflowing as images arrive. Remote sources and images that already
/// carry dimensions are left exactly as they are.
pub fn annotate(
    html: &str,
    page_url: &str,
    sizes: &HashMap<String, Size>,
    unresolved: &mut Vec<Unresolved>,
) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(at) = rest.find("<img") {
        let (before, from) = rest.split_at(at);
        out.push_str(before);
        let Some(close) = from.find('>') else {
            out.push_str(from);
            return out;
        };
        let tag = &from[..=close];
        out.push_str(&annotate_tag(tag, page_url, sizes, unresolved));
        rest = &from[close + 1..];
    }
    out.push_str(rest);
    out
}

fn annotate_tag(
    tag: &str,
    page_url: &str,
    sizes: &HashMap<String, Size>,
    unresolved: &mut Vec<Unresolved>,
) -> String {
    if attr(tag, "width").is_some() || attr(tag, "height").is_some() {
        return tag.to_string();
    }
    let Some(src) = attr(tag, "src") else {
        return tag.to_string();
    };
    if src.contains("://") || src.starts_with("//") || src.starts_with("data:") {
        return tag.to_string();
    }
    let resolved = resolve(page_url, src);
    match sizes.get(&resolved) {
        Some(size) => {
            let insert = tag.len() - if tag.ends_with("/>") { 2 } else { 1 };
            format!(
                "{} width=\"{}\" height=\"{}\"{}",
                tag[..insert].trim_end(),
                size.width,
                size.height,
                &tag[insert..]
            )
        }
        None => {
            unresolved.push(Unresolved {
                page_url: page_url.to_string(),
                src: src.to_string(),
            });
            tag.to_string()
        }
    }
}

/// Resolves a reference against the page that made it.
pub fn resolve(page_url: &str, src: &str) -> String {
    if src.starts_with('/') {
        return src.to_string();
    }
    let dir = page_url.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
    let mut parts: Vec<&str> = dir.split('/').filter(|s| !s.is_empty()).collect();
    for segment in src.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    format!("/{}", parts.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The smallest valid files, as bytes, so the tests need no encoder.
    const PNG_1X1: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x07, 0x00, 0x00, 0x00, 0x05, 0x08, 0x06, 0x00, 0x00, 0x00,
    ];
    const GIF_3X2: &[u8] = b"GIF89a\x03\x00\x02\x00\x00\x00\x00";

    fn fixture(name: &str) -> PathBuf {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-images")
            .join(name);
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("mkdir");
        root
    }

    fn write(root: &Path, rel: &str, bytes: &[u8]) -> PathBuf {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().expect("parent")).expect("mkdir");
        std::fs::write(&p, bytes).expect("write");
        p
    }

    #[test]
    fn raster_dimensions_are_read_from_the_header() {
        let root = fixture("raster");
        assert_eq!(
            size(&write(&root, "a.png", PNG_1X1)),
            Some(Size {
                width: 7,
                height: 5
            })
        );
        assert_eq!(
            size(&write(&root, "b.gif", GIF_3X2)),
            Some(Size {
                width: 3,
                height: 2
            })
        );
    }

    #[test]
    fn svg_width_and_height_win_over_the_view_box() {
        let s = svg_size(r#"<svg width="40" height="20" viewBox="0 0 100 50"></svg>"#);
        assert_eq!(
            s,
            Some(Size {
                width: 40,
                height: 20
            })
        );
    }

    #[test]
    fn a_view_box_alone_is_enough() {
        assert_eq!(
            svg_size(r#"<svg viewBox="0 0 24 24" fill="none"></svg>"#),
            Some(Size {
                width: 24,
                height: 24
            })
        );
        // Commas are legal separators and real exports use them.
        assert_eq!(
            svg_size(r#"<svg viewBox="0,0,16,9"></svg>"#),
            Some(Size {
                width: 16,
                height: 9
            })
        );
    }

    #[test]
    fn css_units_are_converted_to_pixels() {
        assert_eq!(
            svg_size(r#"<svg width="120px" height="60px"></svg>"#),
            Some(Size {
                width: 120,
                height: 60
            })
        );
        // 72pt is an inch, which is 96px.
        assert_eq!(
            svg_size(r#"<svg width="72pt" height="36pt"></svg>"#),
            Some(Size {
                width: 96,
                height: 48
            })
        );
    }

    #[test]
    fn a_percentage_width_has_no_intrinsic_size() {
        // Its size comes from the parent, so reporting a number would be a
        // guess, and a wrong number is worse than none.
        assert_eq!(svg_size(r#"<svg width="100%" height="100%"></svg>"#), None);
    }

    #[test]
    fn colocated_files_keep_their_position() {
        let root = fixture("colocated");
        write(&root, "docs/install.md", b"# x");
        write(&root, "docs/images/retry.png", PNG_1X1);
        write(&root, "docs/deep/nested/a.gif", GIF_3X2);
        let found = colocated(&root, "/base").expect("walk");
        let urls: Vec<_> = found.iter().map(|a| a.url.as_str()).collect();
        assert_eq!(
            urls,
            [
                "/base/docs/deep/nested/a.gif",
                "/base/docs/images/retry.png"
            ]
        );
    }

    #[test]
    fn a_relative_reference_resolves_against_its_page() {
        assert_eq!(
            resolve("/docs/install", "./images/a.png"),
            "/docs/images/a.png"
        );
        assert_eq!(
            resolve("/docs/guide/deep", "../shared/b.png"),
            "/docs/shared/b.png"
        );
        assert_eq!(
            resolve("/docs/install", "/absolute/c.png"),
            "/absolute/c.png"
        );
    }

    #[test]
    fn dimensions_are_stamped_onto_markup() {
        let mut sizes = HashMap::new();
        sizes.insert(
            "/docs/images/a.png".to_string(),
            Size {
                width: 800,
                height: 600,
            },
        );
        let mut missing = Vec::new();
        let out = annotate(
            r#"<p><img src="./images/a.png" alt="x"></p>"#,
            "/docs/install",
            &sizes,
            &mut missing,
        );
        assert!(out.contains(r#"width="800" height="600""#), "{out}");
        assert!(out.contains(r#"alt="x""#), "attributes lost: {out}");
        assert!(missing.is_empty());
    }

    #[test]
    fn an_image_that_declares_its_size_is_untouched() {
        let mut sizes = HashMap::new();
        sizes.insert(
            "/a.png".to_string(),
            Size {
                width: 1,
                height: 1,
            },
        );
        let html = r#"<img src="/a.png" width="20">"#;
        let mut missing = Vec::new();
        assert_eq!(annotate(html, "/p", &sizes, &mut missing), html);
    }

    #[test]
    fn a_remote_source_is_left_alone_and_not_reported() {
        let html = r#"<img src="https://cdn.example.com/a.png">"#;
        let mut missing = Vec::new();
        assert_eq!(annotate(html, "/p", &HashMap::new(), &mut missing), html);
        assert!(missing.is_empty(), "a remote image is not ours to resolve");
    }

    #[test]
    fn an_unresolvable_reference_is_reported_rather_than_silently_passed() {
        let mut missing = Vec::new();
        let html = r#"<img src="./gone.png">"#;
        assert_eq!(
            annotate(html, "/docs/x", &HashMap::new(), &mut missing),
            html
        );
        assert_eq!(
            missing,
            vec![Unresolved {
                page_url: "/docs/x".into(),
                src: "./gone.png".into()
            }]
        );
    }

    #[test]
    fn a_self_closing_tag_keeps_its_slash() {
        let mut sizes = HashMap::new();
        sizes.insert(
            "/a.png".to_string(),
            Size {
                width: 4,
                height: 3,
            },
        );
        let mut missing = Vec::new();
        let out = annotate(r#"<img src="/a.png" />"#, "/p", &sizes, &mut missing);
        assert!(out.ends_with("/>"), "{out}");
        assert!(out.contains(r#"width="4""#), "{out}");
    }
}
