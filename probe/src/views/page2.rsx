
pub fn Page2(heading: &str, items: &[String]) -> Element {
    <section class="page">
        <h1>{heading}</h1>
        for item in items {
            <p class="row">{item}</p>
        }
        if items.is_empty() {
            <p class="empty">Nothing here yet.</p>
        }
    </section>
}
