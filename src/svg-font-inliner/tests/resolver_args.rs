use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use base64::Engine;
use svg_font_inliner::{embed_svg_fonts, FontQuery};

fn fixture_font_path(file_name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(file_name)
}

fn fixture_font_bytes(file_name: &str) -> Vec<u8> {
    std::fs::read(fixture_font_path(file_name)).expect("fixture font should be readable")
}

fn extract_data_urls(svg: &str) -> Vec<String> {
    let mut urls = Vec::new();
    let mut cursor = 0;

    while let Some(rel_start) = svg[cursor..].find("url(data:") {
        let data_start = cursor + rel_start + "url(".len();
        let rel_end = svg[data_start..]
            .find(')')
            .expect("url(data:...) should have closing ')' in output CSS");
        urls.push(svg[data_start..data_start + rel_end].to_string());
        cursor = data_start + rel_end + 1;
    }

    urls
}

fn decode_data_url(data_url: &str) -> (String, Vec<u8>) {
    assert!(
        data_url.starts_with("data:"),
        "expected data URL, got: {data_url}"
    );
    let without_prefix = &data_url["data:".len()..];
    let (meta, payload) = without_prefix
        .split_once(',')
        .expect("data URL should contain metadata and payload");
    assert!(
        meta.ends_with(";base64"),
        "expected base64 metadata in data URL: {meta}"
    );
    let mime = meta.trim_end_matches(";base64").to_string();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(payload)
        .expect("base64 payload should decode");
    (mime, bytes)
}

fn svg_with_single_text(family: &str) -> String {
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="60"><text x="10" y="40" font-family="{family}">A</text></svg>"#
    )
}

#[test]
fn resolver_receives_explicit_text_style_arguments() {
    let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="240" height="60">
  <text
    x="10"
    y="40"
    font-family="'FontA','FontB',sans-serif"
    font-style="italic"
    font-weight="700"
    font-stretch="condensed"
    font-size="24"
  >
    A
  </text>
</svg>"#;

    let queries: Arc<Mutex<Vec<FontQuery>>> = Arc::new(Mutex::new(Vec::new()));
    let queries_for_resolver = Arc::clone(&queries);
    let font_path = fixture_font_path("font-a.ttf");

    let output = embed_svg_fonts(svg, move |query| {
        queries_for_resolver
            .lock()
            .expect("query capture mutex poisoned")
            .push(query.clone());
        Ok(font_path.clone())
    })
    .expect("embedding should succeed");

    assert!(output.contains("@font-face"));

    let captured = queries.lock().expect("query capture mutex poisoned");
    assert!(
        !captured.is_empty(),
        "resolver should receive at least one query"
    );

    let explicit = captured
        .iter()
        .find(|query| {
            query
                .families
                .starts_with(&["FontA".to_string(), "FontB".to_string()])
        })
        .expect("expected resolver query for explicit FontA/FontB family list");

    assert_eq!(explicit.style, "Italic");
    assert_eq!(explicit.weight, 700);
    assert_eq!(explicit.stretch, "Condensed");
    assert_eq!(explicit.missing_char, None);
    assert!(
        !explicit.variations.is_empty(),
        "variations should be populated"
    );
}

#[test]
fn resolver_receives_default_style_arguments() {
    let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="50">
  <text x="10" y="30" font-family="'FontA','FontB'">A</text>
</svg>"#;

    let queries: Arc<Mutex<Vec<FontQuery>>> = Arc::new(Mutex::new(Vec::new()));
    let queries_for_resolver = Arc::clone(&queries);
    let font_path = fixture_font_path("font-b.ttf");

    let output = embed_svg_fonts(svg, move |query| {
        queries_for_resolver
            .lock()
            .expect("query capture mutex poisoned")
            .push(query.clone());
        Ok(font_path.clone())
    })
    .expect("embedding should succeed");

    assert!(output.contains("@font-face"));

    let captured = queries.lock().expect("query capture mutex poisoned");
    assert!(
        !captured.is_empty(),
        "resolver should receive at least one query"
    );

    let default_style = captured
        .iter()
        .find(|query| {
            query
                .families
                .starts_with(&["FontA".to_string(), "FontB".to_string()])
        })
        .expect("expected resolver query for FontA/FontB family list");

    assert_eq!(default_style.style, "Normal");
    assert_eq!(default_style.weight, 400);
    assert_eq!(default_style.stretch, "Normal");
    assert_eq!(default_style.missing_char, None);
    assert!(
        !default_style.variations.is_empty(),
        "variations should be populated"
    );
}

#[test]
fn resolver_error_is_propagated() {
    let svg = svg_with_single_text("'FontA'");
    let err = embed_svg_fonts(&svg, |_query| Err("resolver boom".to_string()))
        .expect_err("embedding should fail when resolver fails");
    assert!(
        err.contains("resolver boom"),
        "expected resolver error message, got: {err}"
    );
}

#[test]
fn resolver_invalid_path_is_rejected() {
    let svg = svg_with_single_text("'FontA'");
    let err = embed_svg_fonts(&svg, |_query| {
        Ok(PathBuf::from(
            "/definitely/not/a/real/font/path-for-test.ttf",
        ))
    })
    .expect_err("embedding should fail for non-existent font file");

    assert!(
        err.contains("font resolver returned non-existent file")
            || err.contains("failed to load font"),
        "expected invalid-path font error, got: {err}"
    );
}

#[test]
fn inlines_font_a_when_resolver_returns_font_a() {
    let svg = svg_with_single_text("'FontA'");
    let expected = fixture_font_bytes("font-a.ttf");
    let not_expected = fixture_font_bytes("font-b.ttf");
    let font_path = fixture_font_path("font-a.ttf");

    let output = embed_svg_fonts(&svg, move |_query| Ok(font_path.clone()))
        .expect("embedding should succeed");
    let urls = extract_data_urls(&output);
    assert!(
        !urls.is_empty(),
        "expected at least one embedded font data URL"
    );

    let (_, bytes) = decode_data_url(&urls[0]);
    assert_eq!(bytes, expected, "expected font-a payload");
    assert_ne!(bytes, not_expected, "did not expect font-b payload");
}

#[test]
fn inlines_font_b_when_resolver_returns_font_b() {
    let svg = svg_with_single_text("'FontB'");
    let expected = fixture_font_bytes("font-b.ttf");
    let not_expected = fixture_font_bytes("font-a.ttf");
    let font_path = fixture_font_path("font-b.ttf");

    let output = embed_svg_fonts(&svg, move |_query| Ok(font_path.clone()))
        .expect("embedding should succeed");
    let urls = extract_data_urls(&output);
    assert!(
        !urls.is_empty(),
        "expected at least one embedded font data URL"
    );

    let (_, bytes) = decode_data_url(&urls[0]);
    assert_eq!(bytes, expected, "expected font-b payload");
    assert_ne!(bytes, not_expected, "did not expect font-a payload");
}

#[test]
fn inlines_both_fonts_when_resolver_returns_both() {
    let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="220" height="90">
  <text x="10" y="30" font-family="'FontA'">A</text>
  <text x="10" y="70" font-family="'FontB'">A</text>
</svg>"#;
    let font_a_path = fixture_font_path("font-a.ttf");
    let font_b_path = fixture_font_path("font-b.ttf");
    let font_a_bytes = fixture_font_bytes("font-a.ttf");
    let font_b_bytes = fixture_font_bytes("font-b.ttf");

    let output = embed_svg_fonts(svg, move |query| {
        if query.families.iter().any(|f| f == "FontA") {
            return Ok(font_a_path.clone());
        }
        if query.families.iter().any(|f| f == "FontB") {
            return Ok(font_b_path.clone());
        }
        Err(format!("unexpected families: {:?}", query.families))
    })
    .expect("embedding should succeed");

    let decoded = extract_data_urls(&output)
        .into_iter()
        .map(|url| decode_data_url(&url).1)
        .collect::<Vec<_>>();
    assert!(
        decoded.iter().any(|bytes| bytes == &font_a_bytes),
        "expected font-a bytes among embedded payloads"
    );
    assert!(
        decoded.iter().any(|bytes| bytes == &font_b_bytes),
        "expected font-b bytes among embedded payloads"
    );
}

#[test]
fn dedupes_same_resolved_font() {
    let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="220" height="90">
  <text x="10" y="30" font-family="'FontA'">A</text>
  <text x="10" y="70" font-family="'FontA'">A</text>
</svg>"#;
    let font_path = fixture_font_path("font-a.ttf");

    let output = embed_svg_fonts(svg, move |_query| Ok(font_path.clone()))
        .expect("embedding should succeed");
    let font_face_count = output.matches("@font-face").count();
    assert_eq!(
        font_face_count, 1,
        "expected exactly one @font-face block for deduped request"
    );
    let urls = extract_data_urls(&output);
    assert_eq!(urls.len(), 1, "expected exactly one embedded data URL");
}

#[test]
fn family_order_preference_is_usable_by_resolver() {
    let svg = svg_with_single_text("'MissingFont','FontB'");
    let observed_queries: Arc<Mutex<Vec<FontQuery>>> = Arc::new(Mutex::new(Vec::new()));
    let observed_for_resolver = Arc::clone(&observed_queries);
    let font_b_path = fixture_font_path("font-b.ttf");
    let font_b_bytes = fixture_font_bytes("font-b.ttf");

    let output = embed_svg_fonts(&svg, move |query| {
        observed_for_resolver
            .lock()
            .expect("query capture mutex poisoned")
            .push(query.clone());

        if query.families.iter().any(|f| f == "MissingFont")
            && query.families.iter().any(|f| f == "FontB")
        {
            return Ok(font_b_path.clone());
        }

        Err(format!("unexpected families: {:?}", query.families))
    })
    .expect("embedding should succeed");

    let captured = observed_queries
        .lock()
        .expect("query capture mutex poisoned");
    let query = captured
        .iter()
        .find(|q| q.families.iter().any(|f| f == "MissingFont"))
        .expect("expected query containing MissingFont");
    assert!(
        query
            .families
            .starts_with(&["MissingFont".to_string(), "FontB".to_string()]),
        "family order should be preserved"
    );

    let decoded = extract_data_urls(&output)
        .into_iter()
        .map(|url| decode_data_url(&url).1)
        .collect::<Vec<_>>();
    assert!(
        decoded.iter().any(|bytes| bytes == &font_b_bytes),
        "expected FontB payload in output"
    );
}

#[test]
fn mixed_success_and_failure_returns_error() {
    let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="220" height="90">
  <text x="10" y="30" font-family="'FontA'">A</text>
  <text x="10" y="70" font-family="'MissingOnly'">A</text>
</svg>"#;
    let font_a_path = fixture_font_path("font-a.ttf");

    let err = embed_svg_fonts(svg, move |query| {
        if query.families.iter().any(|f| f == "MissingOnly") {
            return Err("missing-only resolver failure".to_string());
        }
        Ok(font_a_path.clone())
    })
    .expect_err("mixed resolver success/failure should fail overall");

    assert!(
        err.contains("missing-only resolver failure"),
        "expected propagated mixed-path error, got: {err}"
    );
}

#[test]
fn malformed_svg_returns_parse_error() {
    let malformed = "<svg><text>A</svg";
    let font_path = fixture_font_path("font-a.ttf");

    let err = embed_svg_fonts(malformed, move |_query| Ok(font_path.clone()))
        .expect_err("malformed SVG should fail parsing");
    assert!(
        err.contains("failed to parse SVG"),
        "expected parse error message, got: {err}"
    );
}

#[test]
fn data_url_format_is_correct_and_decodable() {
    let svg = svg_with_single_text("'FontA'");
    let font_path = fixture_font_path("font-a.ttf");

    let output = embed_svg_fonts(&svg, move |_query| Ok(font_path.clone()))
        .expect("embedding should succeed");
    let urls = extract_data_urls(&output);
    assert!(!urls.is_empty(), "expected at least one data URL");

    for data_url in urls {
        let (mime, bytes) = decode_data_url(&data_url);
        assert_eq!(mime, "font/ttf", "expected TTF mime for .ttf fixture");
        assert!(
            !bytes.is_empty(),
            "decoded font payload should not be empty"
        );
    }
}

#[test]
fn primary_font_a_then_fallback_for_b_inlines_a_and_b() {
    let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="60"><text x="10" y="40" font-family="'FontA'">AB</text></svg>"#;
    let queries: Arc<Mutex<Vec<FontQuery>>> = Arc::new(Mutex::new(Vec::new()));
    let queries_for_resolver = Arc::clone(&queries);
    let font_a_path = fixture_font_path("font-a.ttf");
    let font_b_path = fixture_font_path("font-b.ttf");
    let font_a_bytes = fixture_font_bytes("font-a.ttf");
    let font_b_bytes = fixture_font_bytes("font-b.ttf");

    let output = embed_svg_fonts(svg, move |query| {
        queries_for_resolver
            .lock()
            .expect("query capture mutex poisoned")
            .push(query.clone());

        match query.missing_char {
            None => Ok(font_a_path.clone()),
            Some('B') => Ok(font_b_path.clone()),
            Some(other) => Err(format!("unexpected fallback char: {other}")),
        }
    })
    .expect("embedding should succeed");

    let captured = queries.lock().expect("query capture mutex poisoned");
    assert_eq!(captured[0].missing_char, None);
    assert!(
        captured.iter().any(|q| q.missing_char == Some('B')),
        "expected fallback resolver call for missing 'B'"
    );

    let decoded = extract_data_urls(&output)
        .into_iter()
        .map(|url| decode_data_url(&url).1)
        .collect::<Vec<_>>();
    assert!(decoded.iter().any(|bytes| bytes == &font_a_bytes));
    assert!(decoded.iter().any(|bytes| bytes == &font_b_bytes));
}

#[test]
fn fallback_can_chain_to_c() {
    let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="220" height="60"><text x="10" y="40" font-family="'FontA'">ABC</text></svg>"#;
    let queries: Arc<Mutex<Vec<FontQuery>>> = Arc::new(Mutex::new(Vec::new()));
    let queries_for_resolver = Arc::clone(&queries);
    let font_a_path = fixture_font_path("font-a.ttf");
    let font_b_path = fixture_font_path("font-b.ttf");
    let font_c_path = fixture_font_path("font-c.ttf");
    let font_a_bytes = fixture_font_bytes("font-a.ttf");
    let font_b_bytes = fixture_font_bytes("font-b.ttf");
    let font_c_bytes = fixture_font_bytes("font-c.ttf");

    let output = embed_svg_fonts(svg, move |query| {
        queries_for_resolver
            .lock()
            .expect("query capture mutex poisoned")
            .push(query.clone());

        match query.missing_char {
            None => Ok(font_a_path.clone()),
            Some('B') => Ok(font_b_path.clone()),
            Some('C') => Ok(font_c_path.clone()),
            Some(other) => Err(format!("unexpected fallback char: {other}")),
        }
    })
    .expect("embedding should succeed");

    let captured = queries.lock().expect("query capture mutex poisoned");
    assert!(captured.iter().any(|q| q.missing_char == Some('B')));
    assert!(captured.iter().any(|q| q.missing_char == Some('C')));

    let decoded = extract_data_urls(&output)
        .into_iter()
        .map(|url| decode_data_url(&url).1)
        .collect::<Vec<_>>();
    assert!(decoded.iter().any(|bytes| bytes == &font_a_bytes));
    assert!(decoded.iter().any(|bytes| bytes == &font_b_bytes));
    assert!(decoded.iter().any(|bytes| bytes == &font_c_bytes));
}

#[test]
fn when_primary_font_covers_text_no_fallback_query_is_made() {
    let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="60"><text x="10" y="40" font-family="'FontA'">AB</text></svg>"#;
    let queries: Arc<Mutex<Vec<FontQuery>>> = Arc::new(Mutex::new(Vec::new()));
    let queries_for_resolver = Arc::clone(&queries);
    let font_b_path = fixture_font_path("font-b.ttf");

    let _output = embed_svg_fonts(svg, move |query| {
        queries_for_resolver
            .lock()
            .expect("query capture mutex poisoned")
            .push(query.clone());

        match query.missing_char {
            None => Ok(font_b_path.clone()),
            Some(other) => Err(format!("did not expect fallback query for {other}")),
        }
    })
    .expect("embedding should succeed");

    let captured = queries.lock().expect("query capture mutex poisoned");
    assert!(captured.iter().all(|q| q.missing_char.is_none()));
}
