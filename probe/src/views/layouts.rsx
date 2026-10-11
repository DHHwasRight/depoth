
pub fn Shell(title: &str, children: Element) -> Element {
    <html lang="en">
        <head><title>{title}</title></head>
        <body><main dp-layout="/">{children}</main></body>
    </html>
}
