use std::fs;
use std::path::PathBuf;

use base64::Engine;

fn render(svg: &str, zoom: f32) -> Result<Vec<u8>, String> {
    svg_to_png::render_svg_to_png(svg, zoom, None)
}

fn render_with_background(svg: &str, zoom: f32, background: &str) -> Result<Vec<u8>, String> {
    let background = svg_to_png::BackgroundColor::parse(background)?;
    svg_to_png::render_svg_to_png(svg, zoom, Some(background))
}

fn fixture_font_path(file_name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(file_name)
}

fn fixture_font_data_url(file_name: &str) -> String {
    let bytes = fs::read(fixture_font_path(file_name)).expect("fixture font should be readable");
    let payload = base64::engine::general_purpose::STANDARD.encode(bytes);
    format!("data:font/ttf;base64,{payload}")
}

fn font_face_css(family: &str, file_name: &str) -> String {
    format!(
        "@font-face {{ font-family: '{family}'; font-style: normal; font-weight: 400; src: url({}) format('truetype'); }}",
        fixture_font_data_url(file_name)
    )
}

fn png_size(bytes: &[u8]) -> (u32, u32) {
    assert!(bytes.len() >= 24, "expected at least PNG header + IHDR");
    assert_eq!(&bytes[0..8], b"\x89PNG\r\n\x1a\n");
    assert_eq!(&bytes[12..16], b"IHDR");

    let w = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
    let h = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
    (w, h)
}

#[test]
fn zoom_validation_rejects_non_positive_or_non_finite() {
    let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"/>"#;

    for zoom in [0.0, -1.0, f32::NAN, f32::INFINITY] {
        let err = render(svg, zoom).expect_err("invalid zoom should error");
        assert!(err.contains("--zoom"));
    }
}

#[test]
fn zoom_scales_output_dimensions() {
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10"><rect width="20" height="10" fill="#000"/></svg>"##;

    let out1 = render(svg, 1.0).expect("zoom=1 should render");
    let out2 = render(svg, 2.0).expect("zoom=2 should render");
    let (w1, h1) = png_size(&out1);
    let (w2, h2) = png_size(&out2);

    assert!(w2 > w1, "expected width to increase with zoom");
    assert!(h2 > h1, "expected height to increase with zoom");
}

#[test]
fn rejects_non_svg_or_parse_failure_input() {
    let err = render("not svg", 1.0).expect_err("non-svg should fail");
    assert!(!err.is_empty());

    let malformed = r#"<svg xmlns="http://www.w3.org/2000/svg"><text>hello</svg"#;
    let err = render(malformed, 1.0).expect_err("malformed svg should fail");
    assert!(!err.is_empty());
}

#[test]
fn returns_png_signature_on_valid_svg() {
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"><rect width="8" height="8" fill="#fff"/></svg>"##;
    let out = render(svg, 1.0).expect("valid svg should render");
    assert!(out.len() >= 8);
    assert_eq!(&out[0..8], b"\x89PNG\r\n\x1a\n");
}

#[test]
fn deterministic_output_for_same_svg_and_zoom() {
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"><circle cx="4" cy="4" r="3" fill="#0f0"/></svg>"##;
    let out1 = render(svg, 1.0).expect("first render should succeed");
    let out2 = render(svg, 1.0).expect("second render should succeed");
    assert_eq!(out1, out2, "render output should be deterministic");
}

#[test]
fn rejects_external_image_href() {
    let svgs = [
        r#"<svg xmlns="http://www.w3.org/2000/svg"><image href="https://example.com/a.png" width="10" height="10"/></svg>"#,
        r#"<svg xmlns="http://www.w3.org/2000/svg"><image href="file:///tmp/a.png" width="10" height="10"/></svg>"#,
        r#"<svg xmlns="http://www.w3.org/2000/svg"><image href="a.png" width="10" height="10"/></svg>"#,
    ];

    for svg in svgs {
        let err = render(svg, 1.0).expect_err("external image refs should fail");
        assert!(!err.is_empty());
    }
}

#[test]
fn allows_data_url_image_href() {
    let one_px = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+/a0cAAAAASUVORK5CYII=";
    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1\" height=\"1\"><image href=\"data:image/png;base64,{one_px}\" width=\"1\" height=\"1\"/></svg>"
    );

    let out = render(&svg, 1.0).expect("data URL image should be allowed");
    assert_eq!(&out[0..8], b"\x89PNG\r\n\x1a\n");
}

#[test]
fn allows_internal_fragment_refs() {
    let svg = r##"
<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10">
  <defs>
    <linearGradient id="g"><stop offset="0" stop-color="#f00"/></linearGradient>
  </defs>
  <rect width="10" height="10" fill="url(#g)"/>
</svg>
"##;
    let out = render(svg, 1.0).expect("internal # refs should be allowed");
    assert_eq!(&out[0..8], b"\x89PNG\r\n\x1a\n");
}

#[test]
fn rejects_css_external_url_and_import() {
    let with_external_url = r#"
<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10">
  <style><![CDATA[
    rect { fill: url(https://example.com/pattern.svg); }
  ]]></style>
  <rect width="10" height="10"/>
</svg>
"#;
    let with_import = r#"
<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10">
  <style><![CDATA[
    @import url('https://example.com/style.css');
  ]]></style>
  <rect width="10" height="10"/>
</svg>
"#;

    let err = render(with_external_url, 1.0).expect_err("external CSS URL should fail");
    assert!(!err.is_empty());
    let err = render(with_import, 1.0).expect_err("@import should fail");
    assert!(!err.is_empty());
}

#[test]
fn rejects_xml_stylesheet_processing_instruction() {
    let svg = r#"<?xml version="1.0"?>
<?xml-stylesheet type="text/css" href="https://example.com/style.css"?>
<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"></svg>"#;

    let err = render(svg, 1.0).expect_err("xml stylesheet PI should fail");
    assert!(!err.is_empty());
}

#[test]
fn rejects_text_when_no_inline_font_faces_present() {
    let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="30"><text x="5" y="20" font-family="FontA">A</text></svg>"#;
    let err = render(svg, 1.0).expect_err("text without inline font-face should fail");
    assert!(!err.is_empty());
}

#[test]
fn rejects_inline_font_face_without_data_src() {
    let svg = r#"
<svg xmlns="http://www.w3.org/2000/svg" width="100" height="30">
  <defs>
    <style><![CDATA[
      @font-face { font-family: 'FontA'; src: url('/tmp/font-a.ttf') format('truetype'); }
    ]]></style>
  </defs>
  <text x="5" y="20" font-family="FontA">A</text>
</svg>
"#;

    let err = render(svg, 1.0).expect_err("font-face without data src should fail");
    assert!(!err.is_empty());
}

#[test]
fn accepts_inline_font_face_with_data_src() {
    let css = font_face_css("FontA", "font-a.ttf");
    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"100\" height=\"30\"><defs><style><![CDATA[{css}]]></style></defs><text x=\"5\" y=\"20\" font-family=\"FontA\">A</text></svg>"
    );

    let out = render(&svg, 1.0).expect("inline data font-face should succeed");
    assert_eq!(&out[0..8], b"\x89PNG\r\n\x1a\n");
}

#[test]
fn errors_when_text_font_cannot_be_resolved_from_inline_faces() {
    let css = font_face_css("FontA", "font-a.ttf");
    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"100\" height=\"30\"><defs><style><![CDATA[{css}]]></style></defs><text x=\"5\" y=\"20\" font-family=\"MissingFont\">A</text></svg>"
    );

    let err = render(&svg, 1.0).expect_err("missing font family should fail");
    assert!(!err.is_empty());
}

#[test]
fn ok_when_all_text_fonts_resolve_from_inline_faces() {
    let css = format!(
        "{} {}",
        font_face_css("FontA", "font-a.ttf"),
        font_face_css("FontB", "font-b.ttf")
    );
    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"100\" height=\"50\"><defs><style><![CDATA[{css}]]></style></defs><text x=\"5\" y=\"20\" font-family=\"FontA\">A</text><text x=\"5\" y=\"40\" font-family=\"FontB\">B</text></svg>"
    );

    let out = render(&svg, 1.0).expect("all referenced inline fonts should resolve");
    assert_eq!(&out[0..8], b"\x89PNG\r\n\x1a\n");
}

#[test]
fn ok_with_inline_fallback_font_chain() {
    let css = format!(
        "{} {} {}",
        font_face_css("FontA", "font-a.ttf"),
        font_face_css("FontB", "font-b.ttf"),
        font_face_css("FontC", "font-c.ttf")
    );
    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"180\" height=\"30\"><defs><style><![CDATA[{css}]]></style></defs><text x=\"5\" y=\"20\" font-family=\"'FontA','FontB','FontC'\">ABC</text></svg>"
    );

    let out = render(&svg, 1.0).expect("fallback chain with inline fonts should resolve");
    assert_eq!(&out[0..8], b"\x89PNG\r\n\x1a\n");
}

#[test]
fn ok_when_font_family_is_inherited_from_group() {
    let css = font_face_css("FontA", "font-a.ttf");
    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"120\" height=\"40\"><defs><style><![CDATA[{css}]]></style></defs><g font-family=\"FontA\"><text x=\"5\" y=\"20\" font-size=\"16\">A</text></g></svg>"
    );

    let out = render(&svg, 1.0).expect("inherited font-family should resolve from inline font");
    assert_eq!(&out[0..8], b"\x89PNG\r\n\x1a\n");
}

#[test]
fn errors_when_fallback_needed_but_unavailable_inline() {
    let css = font_face_css("FontA", "font-a.ttf");
    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"180\" height=\"30\"><defs><style><![CDATA[{css}]]></style></defs><text x=\"5\" y=\"20\" font-family=\"'FontA','MissingFallback'\">ABC</text></svg>"
    );

    let err = render(&svg, 1.0).expect_err("unavailable fallback should fail");
    assert!(!err.is_empty());
}

#[test]
fn inline_text_changes_rendered_pixels() {
    let css = font_face_css("FontA", "font-a.ttf");
    let with_text = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"140\" height=\"40\"><defs><style><![CDATA[{css}]]></style></defs><text x=\"8\" y=\"28\" font-family=\"FontA\" font-size=\"24\" fill=\"#000\">A</text></svg>"
    );
    let without_text =
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"140\" height=\"40\"></svg>";

    let with_text_png = render(&with_text, 1.0).expect("inline text should render");
    let without_text_png = render(without_text, 1.0).expect("empty svg should render");

    assert_ne!(
        with_text_png, without_text_png,
        "text render should differ from empty canvas"
    );
}

#[test]
fn identical_svg_with_different_inline_fonts_produces_different_png() {
    let css_a = font_face_css("FontUnderTest", "font-a.ttf");
    let css_b = font_face_css("FontUnderTest", "font-b.ttf");

    let svg_a = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"180\" height=\"50\"><defs><style><![CDATA[{css_a}]]></style></defs><text x=\"8\" y=\"36\" font-family=\"FontUnderTest\" font-size=\"32\" fill=\"#000\">AA</text></svg>"
    );
    let svg_b = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"180\" height=\"50\"><defs><style><![CDATA[{css_b}]]></style></defs><text x=\"8\" y=\"36\" font-family=\"FontUnderTest\" font-size=\"32\" fill=\"#000\">AA</text></svg>"
    );

    let png_a = render(&svg_a, 1.0).expect("font-a svg should render");
    let png_b = render(&svg_b, 1.0).expect("font-b svg should render");

    assert_ne!(
        png_a, png_b,
        "same SVG text with different inline fonts should produce different PNG bytes"
    );
}

#[test]
fn background_color_changes_output_bytes() {
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"><circle cx="8" cy="8" r="4" fill="#000"/></svg>"##;
    let transparent = render(svg, 1.0).expect("transparent render should succeed");
    let white =
        render_with_background(svg, 1.0, "#ffffff").expect("background render should succeed");

    assert_ne!(
        transparent, white,
        "transparent and solid background renders should differ"
    );
}

#[test]
fn background_color_parser_rejects_invalid_values() {
    let err = svg_to_png::BackgroundColor::parse("not-a-color")
        .expect_err("invalid background color should fail");
    assert!(err.contains("--background"));
}
