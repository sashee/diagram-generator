use std::path::Path;

use base64::Engine;
use svg_font_inliner::embed_svg_fonts;

// A font hb-subset fails on is embedded whole.
// This test binary has a single test, as it sets HB_SUBSET_BIN for the whole process.
#[test]
fn font_is_embedded_whole_when_subsetting_fails() {
    std::env::set_var("HB_SUBSET_BIN", "false");
    let font_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("font-c.ttf");
    let font_bytes = std::fs::read(&font_path).expect("fixture font should be readable");
    let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="60"><text x="10" y="40" font-family="'FontC'">A</text></svg>"#;

    let output = embed_svg_fonts(svg, move |_query| Ok(font_path.clone()))
        .expect("embedding should succeed");
    let payload = output
        .split_once("base64,")
        .and_then(|(_, rest)| rest.split_once(')'))
        .map(|(payload, _)| payload)
        .expect("output should embed a font");
    let embedded = base64::engine::general_purpose::STANDARD
        .decode(payload)
        .expect("base64 payload should decode");
    assert_eq!(embedded, font_bytes);
}
