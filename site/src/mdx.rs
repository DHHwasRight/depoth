//! The components a document may call.
//!
//! Four callouts, not six. Across a real 489-page corpus, `Check`, `Danger`
//! and the generic `Callout` were used zero times, while `Info` alone appeared
//! 231 times. Shipping a component nobody writes costs a name in the
//! namespace and a row in the documentation.
//!
//! Each maps onto a variant the stylesheet already defines, so a callout never
//! picks its own colours: one hue is the accent and every other hue means
//! exactly one thing.

use depoth::components::{Attrs, Components};

/// `Info` and `Note` are the same box. Mintlify ships both and real documents
/// use both, so accepting both costs nothing and rejecting one costs an edit
/// on every page that migrates.
const CALLOUTS: [(&str, &str, &str); 5] = [
    ("Note", "note", "Note"),
    ("Info", "note", "Note"),
    ("Tip", "tip", "Tip"),
    ("Warning", "caution", "Warning"),
    ("Check", "success", "Check"),
];

pub fn components() -> Components {
    let mut set = Components::new();
    for (tag, variant, default_title) in CALLOUTS {
        set = set.add(tag, move |attrs: &Attrs, children: &str| {
            let title = attrs.get_or("title", default_title);
            format!(
                "<aside class=\"callout {variant}\">\
                   <p class=\"callout__title\">{}</p>\
                   <div class=\"callout__body\">{children}</div>\
                 </aside>",
                escape(title)
            )
        });
    }
    set.add("Tabs", |_, children| {
        format!("<div class=\"mdx-tabs\">{children}</div>")
    })
    .add("Tab", |attrs: &Attrs, children: &str| {
        // The tab strip is not built here. A container that reads its
        // children's titles needs a slot mechanism rsx does not have, so each
        // panel carries its own label until that lands.
        format!(
            "<section class=\"mdx-tab\"><h4 class=\"mdx-tab__title\">{}</h4>{children}</section>",
            escape(attrs.get_or("title", ""))
        )
    })
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
