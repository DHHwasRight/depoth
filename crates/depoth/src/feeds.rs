//! Sitemap, RSS, robots.txt, canonical URLs and social metadata.
//!
//! The cheapest rows in the feature gap, and two of them were already promised
//! in print: the configuration documentation says `base_url` is what makes a
//! sitemap and a canonical URL possible, and neither was emitted anywhere.
//!
//! One rule governs the whole module. **`base_url` is the origin.** A
//! deployment sub-path is already part of every page's URL, so joining the two
//! naively writes it twice — a bug this project has already shipped once, in
//! `llms.txt`.

use crate::nav::Entry;

/// One page, as much of it as any of these outputs needs.
#[derive(Debug, Clone)]
pub struct Page {
    /// Absolute path, already carrying any deployment sub-path.
    pub url: String,
    pub title: String,
    pub description: Option<String>,
    /// ISO 8601. Omitted rather than invented.
    pub updated: Option<String>,
    /// Kept out of the sitemap and the feed.
    pub hidden: bool,
}

/// Joins an origin to a path that already carries its own prefix.
pub fn absolute(base_url: &str, path: &str) -> String {
    let origin = base_url.trim_end_matches('/');
    if path.starts_with("http://") || path.starts_with("https://") {
        return path.to_string();
    }
    if origin.is_empty() {
        return path.to_string();
    }
    // The path owns its leading slash and any sub-path; the origin owns the
    // host. Neither is allowed to guess at the other.
    if path.starts_with('/') {
        format!("{origin}{path}")
    } else {
        format!("{origin}/{path}")
    }
}

/// Escapes the five characters XML cannot carry raw.
///
/// An unescaped ampersand in a URL is the classic way a sitemap becomes
/// invalid XML that nobody notices for a year, because browsers render the
/// page fine and only the crawler ever reads the file.
pub fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

/// `sitemap.xml`, in navigation order.
///
/// Returns `None` when `base_url` is empty: a sitemap of relative URLs is not
/// a degraded sitemap, it is an invalid one, and emitting it would be worse
/// than emitting nothing.
pub fn sitemap(base_url: &str, pages: &[Page]) -> Option<String> {
    if base_url.trim().is_empty() {
        return None;
    }
    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n",
    );
    for page in pages.iter().filter(|p| !p.hidden) {
        out.push_str("  <url>\n    <loc>");
        out.push_str(&xml_escape(&absolute(base_url, &page.url)));
        out.push_str("</loc>\n");
        if let Some(updated) = &page.updated {
            out.push_str("    <lastmod>");
            out.push_str(&xml_escape(updated));
            out.push_str("</lastmod>\n");
        }
        out.push_str("  </url>\n");
    }
    out.push_str("</urlset>\n");
    Some(out)
}

/// Channel metadata for a feed.
#[derive(Debug, Clone)]
pub struct Channel<'a> {
    pub title: &'a str,
    pub description: &'a str,
    pub base_url: &'a str,
    /// Where the feed itself is published, e.g. `/feed.xml`.
    pub self_path: &'a str,
}

/// RSS 2.0. Items are emitted newest first; `updated` is an RFC 822 date,
/// which is what RSS requires and ISO 8601 is not.
pub fn rss(channel: &Channel<'_>, items: &[Page]) -> Option<String> {
    if channel.base_url.trim().is_empty() {
        return None;
    }
    let self_url = absolute(channel.base_url, channel.self_path);
    let mut out = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <rss version=\"2.0\" xmlns:atom=\"http://www.w3.org/2005/Atom\">\n\
         <channel>\n  <title>{}</title>\n  <description>{}</description>\n  \
         <link>{}</link>\n  \
         <atom:link href=\"{}\" rel=\"self\" type=\"application/rss+xml\" />\n",
        xml_escape(channel.title),
        xml_escape(channel.description),
        xml_escape(&absolute(channel.base_url, "/")),
        xml_escape(&self_url),
    );
    for item in items.iter().filter(|p| !p.hidden) {
        let link = absolute(channel.base_url, &item.url);
        out.push_str("  <item>\n    <title>");
        out.push_str(&xml_escape(&item.title));
        out.push_str("</title>\n    <link>");
        out.push_str(&xml_escape(&link));
        out.push_str("</link>\n    <guid isPermaLink=\"true\">");
        out.push_str(&xml_escape(&link));
        out.push_str("</guid>\n");
        if let Some(description) = &item.description {
            out.push_str("    <description>");
            out.push_str(&xml_escape(description));
            out.push_str("</description>\n");
        }
        if let Some(updated) = &item.updated {
            out.push_str("    <pubDate>");
            out.push_str(&xml_escape(&rfc822(updated)));
            out.push_str("</pubDate>\n");
        }
        out.push_str("  </item>\n");
    }
    out.push_str("</channel>\n</rss>\n");
    Some(out)
}

/// Converts an ISO 8601 date to the RFC 822 form RSS requires.
///
/// Handles a plain `YYYY-MM-DD` and a full timestamp. Anything else is passed
/// through unchanged, on the grounds that a date we do not understand is more
/// useful to a human than a date we invented.
fn rfc822(iso: &str) -> String {
    const DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let date = iso.split(['T', ' ']).next().unwrap_or(iso);
    let mut parts = date.split('-');
    let (Some(y), Some(m), Some(d)) = (parts.next(), parts.next(), parts.next()) else {
        return iso.to_string();
    };
    let (Ok(y), Ok(m), Ok(d)) = (y.parse::<i64>(), m.parse::<usize>(), d.parse::<i64>()) else {
        return iso.to_string();
    };
    if !(1..=12).contains(&m) {
        return iso.to_string();
    }
    let time = iso.split(['T', ' ']).nth(1).unwrap_or("00:00:00");
    let time = time.trim_end_matches('Z');
    // Sakamoto's method: the weekday without a calendar dependency.
    let t = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let yy = if m < 3 { y - 1 } else { y };
    let weekday = ((yy + yy / 4 - yy / 100 + yy / 400 + t[m - 1] + d) % 7).rem_euclid(7) as usize;
    format!(
        "{}, {:02} {} {} {} +0000",
        DAYS[weekday],
        d,
        MONTHS[m - 1],
        y,
        time
    )
}

/// `robots.txt` naming the sitemap, when there is one to name.
pub fn robots(base_url: &str, sitemap_path: &str) -> String {
    let mut out = String::from("User-agent: *\nAllow: /\n");
    if !base_url.trim().is_empty() {
        out.push_str(&format!(
            "\nSitemap: {}\n",
            absolute(base_url, sitemap_path)
        ));
    }
    out
}

/// `<link rel="canonical">`, the one line every page should carry.
pub fn canonical_tag(base_url: &str, url: &str) -> Option<String> {
    if base_url.trim().is_empty() {
        return None;
    }
    Some(format!(
        "<link rel=\"canonical\" href=\"{}\">",
        xml_escape(&absolute(base_url, url))
    ))
}

/// Open Graph and Twitter tags, so a page shared into a chat is not a bare URL.
pub fn social_tags(base_url: &str, page: &Page, image: Option<&str>, site_name: &str) -> String {
    let mut out = String::new();
    let mut tag = |property: &str, content: &str| {
        out.push_str(&format!(
            "<meta property=\"{property}\" content=\"{}\">\n",
            xml_escape(content)
        ));
    };
    tag("og:type", "article");
    tag("og:site_name", site_name);
    tag("og:title", &page.title);
    if let Some(description) = &page.description {
        tag("og:description", description);
    }
    if !base_url.trim().is_empty() {
        tag("og:url", &absolute(base_url, &page.url));
    }
    if let Some(image) = image {
        tag("og:image", &absolute(base_url, image));
    }
    // Twitter inherits every og: value it has no opinion about, so only the
    // card type is worth stating.
    out.push_str(&format!(
        "<meta name=\"twitter:card\" content=\"{}\">\n",
        if image.is_some() {
            "summary_large_image"
        } else {
            "summary"
        }
    ));
    out
}

/// Pages in navigation order, so every output agrees with the sidebar.
pub fn pages_in_nav_order<F>(entries: &[Entry], lookup: F) -> Vec<Page>
where
    F: Fn(&str) -> Option<Page>,
{
    entries.iter().filter_map(|e| lookup(&e.slug)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(url: &str, title: &str) -> Page {
        Page {
            url: url.into(),
            title: title.into(),
            description: Some(format!("About {title}")),
            updated: Some("2026-10-12".into()),
            hidden: false,
        }
    }

    #[test]
    fn a_sub_path_already_in_the_url_is_not_doubled() {
        // The bug this project already shipped once, in llms.txt.
        assert_eq!(
            absolute("https://example.github.io", "/depoth/docs/install"),
            "https://example.github.io/depoth/docs/install"
        );
        assert_eq!(
            absolute("https://example.com/", "/a"),
            "https://example.com/a"
        );
        assert_eq!(absolute("", "/a"), "/a");
        assert_eq!(
            absolute("https://example.com", "https://other.test/x"),
            "https://other.test/x"
        );
    }

    #[test]
    fn an_ampersand_in_a_url_is_escaped() {
        let out = sitemap("https://e.com", &[page("/a?x=1&y=2", "A")]).expect("sitemap");
        assert!(out.contains("x=1&amp;y=2"), "{out}");
        assert!(!out.contains("x=1&y=2"), "raw ampersand survived: {out}");
    }

    #[test]
    fn an_empty_base_url_yields_no_sitemap_at_all() {
        // A sitemap of relative URLs is invalid, not degraded.
        assert!(sitemap("", &[page("/a", "A")]).is_none());
        assert!(sitemap("   ", &[page("/a", "A")]).is_none());
    }

    #[test]
    fn a_hidden_page_is_in_neither_the_sitemap_nor_the_feed() {
        let mut hidden = page("/secret", "Secret");
        hidden.hidden = true;
        let pages = vec![page("/a", "A"), hidden];
        let map = sitemap("https://e.com", &pages).expect("sitemap");
        assert!(!map.contains("/secret"), "{map}");
        let channel = Channel {
            title: "T",
            description: "D",
            base_url: "https://e.com",
            self_path: "/feed.xml",
        };
        let feed = rss(&channel, &pages).expect("rss");
        assert!(!feed.contains("/secret"), "{feed}");
    }

    #[test]
    fn the_feed_carries_the_self_link_validators_require() {
        let channel = Channel {
            title: "T",
            description: "D",
            base_url: "https://e.com",
            self_path: "/feed.xml",
        };
        let feed = rss(&channel, &[page("/a", "A")]).expect("rss");
        assert!(
            feed.contains(r#"<atom:link href="https://e.com/feed.xml" rel="self""#),
            "{feed}"
        );
        assert!(feed.contains("xmlns:atom="), "{feed}");
    }

    #[test]
    fn rss_dates_are_rfc_822_not_iso() {
        // 2026-10-12 is a Monday.
        assert_eq!(rfc822("2026-10-12"), "Mon, 12 Oct 2026 00:00:00 +0000");
        assert_eq!(
            rfc822("2026-01-01T13:45:00Z"),
            "Thu, 01 Jan 2026 13:45:00 +0000"
        );
        // A date we do not understand is passed through rather than invented.
        assert_eq!(rfc822("sometime"), "sometime");
    }

    #[test]
    fn the_sitemap_follows_the_order_it_was_given() {
        let pages = vec![page("/c", "C"), page("/a", "A"), page("/b", "B")];
        let out = sitemap("https://e.com", &pages).expect("sitemap");
        let at = |s: &str| out.find(s).expect("present");
        assert!(
            at("/c") < at("/a") && at("/a") < at("/b"),
            "reordered: {out}"
        );
    }

    #[test]
    fn a_canonical_tag_needs_an_origin() {
        assert!(canonical_tag("", "/a").is_none());
        assert_eq!(
            canonical_tag("https://e.com", "/depoth/a").expect("tag"),
            "<link rel=\"canonical\" href=\"https://e.com/depoth/a\">"
        );
    }

    #[test]
    fn robots_names_the_sitemap_only_when_there_is_one() {
        assert!(!robots("", "/sitemap.xml").contains("Sitemap:"));
        assert!(
            robots("https://e.com", "/sitemap.xml").contains("Sitemap: https://e.com/sitemap.xml")
        );
    }

    #[test]
    fn social_tags_describe_the_page() {
        let out = social_tags("https://e.com", &page("/a", "A"), Some("/og.png"), "Site");
        assert!(
            out.contains(r#"<meta property="og:title" content="A">"#),
            "{out}"
        );
        assert!(out.contains(r#"content="https://e.com/og.png""#), "{out}");
        assert!(out.contains("summary_large_image"), "{out}");
        let plain = social_tags("https://e.com", &page("/a", "A"), None, "Site");
        assert!(plain.contains(r#"content="summary""#), "{plain}");
    }

    #[test]
    fn the_sitemap_is_well_formed() {
        let out = sitemap("https://e.com", &[page("/a", "A"), page("/b", "B")]).expect("map");
        assert_eq!(out.matches("<url>").count(), 2);
        assert_eq!(out.matches("</url>").count(), 2);
        assert!(out.starts_with("<?xml"));
        assert!(out.trim_end().ends_with("</urlset>"));
        // Every opened tag closes, counted crudely but checkably.
        for tag in ["loc", "lastmod"] {
            assert_eq!(
                out.matches(&format!("<{tag}>")).count(),
                out.matches(&format!("</{tag}>")).count(),
                "unbalanced {tag}: {out}"
            );
        }
    }
}
