mod views { rsx::load_dir!("src/views"); }
use rsx::Template;

fn main() {
    let items: Vec<String> = (0..50).map(|n| format!("row {n}")).collect();
    let mut total = 0usize;
    for _ in 0..200 {
        let body = views::page0::Page0 { heading: "Probe", items: &items }
            .render_to_string()
            .expect("render");
        total += views::layouts::Shell { title: "Probe", children: rsx::raw(body) }
            .render_to_string()
            .expect("render")
            .len();
    }
    println!("{total}");
}
