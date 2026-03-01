use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use base64::Engine;
use lightningcss::printer::PrinterOptions;
use lightningcss::rules::CssRule;
use lightningcss::stylesheet::{ParserOptions, StyleSheet};
use lightningcss::traits::ToCss;
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

fn fixture_data_url(file_name: &str) -> String {
    let bytes = fixture_font_bytes(file_name);
    let payload = base64::engine::general_purpose::STANDARD.encode(bytes);
    format!("data:font/ttf;base64,{payload}")
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

#[test]
fn idempotent_second_run_is_byte_equivalent() {
    let svg = svg_with_single_text("'FontA'");
    let font_path = fixture_font_path("font-a.ttf");

    let out1 = embed_svg_fonts(&svg, move |_query| Ok(font_path.clone()))
        .expect("first embedding should succeed");

    let out2 = embed_svg_fonts(&out1, move |_query| Ok(fixture_font_path("font-a.ttf")))
        .expect("second embedding should succeed and remain stable");

    assert_eq!(out1, out2, "second run should be byte-equivalent");
}

#[test]
fn second_run_does_not_call_resolver_when_fonts_are_already_inlined() {
    let svg = svg_with_single_text("'FontA'");
    let font_path = fixture_font_path("font-a.ttf");
    let out1 = embed_svg_fonts(&svg, move |_query| Ok(font_path.clone()))
        .expect("first embedding should succeed");

    let calls = Arc::new(Mutex::new(0usize));
    let calls_for_resolver = Arc::clone(&calls);

    let _out2 = embed_svg_fonts(&out1, move |_query| {
        let mut guard = calls_for_resolver
            .lock()
            .expect("call counter mutex poisoned");
        *guard += 1;
        Ok(fixture_font_path("font-a.ttf"))
    })
    .expect("second embedding should succeed");

    assert_eq!(
        *calls.lock().expect("call counter mutex poisoned"),
        0,
        "resolver should not be called when all fonts are already available"
    );
}

#[test]
fn existing_font_face_with_mixed_src_uses_data_url() {
    let data_url = fixture_data_url("font-a.ttf");
    let expected_bytes = fixture_font_bytes("font-a.ttf");
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="220" height="80">
  <defs>
    <style><![CDATA[
      @font-face {{
        font-family: 'MixedSans';
        font-style: normal;
        font-weight: 400;
        src: local('Definitely Missing Font'), url('https://example.com/ignored.ttf') format('truetype'), url({data_url}) format('truetype');
      }}
    ]]></style>
  </defs>
  <text x="10" y="40" font-family="'MixedSans'">A</text>
</svg>"#
    );

    let output = embed_svg_fonts(&svg, |_query| {
        Err("resolver should not be called when embedded data source is present".to_string())
    })
    .expect("embedding should succeed when one src data URL is available");

    let decoded = extract_data_urls(&output)
        .into_iter()
        .map(|url| decode_data_url(&url).1)
        .collect::<Vec<_>>();

    assert!(
        decoded.iter().any(|bytes| bytes == &expected_bytes),
        "expected to reuse bytes from data URL source"
    );
}

#[test]
fn existing_font_face_with_multiple_data_urls_uses_first_valid_data_url() {
    let first_data_url = fixture_data_url("font-a.ttf");
    let second_data_url = fixture_data_url("font-b.ttf");
    let first_bytes = fixture_font_bytes("font-a.ttf");
    let second_bytes = fixture_font_bytes("font-b.ttf");
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="220" height="80">
  <defs>
    <style><![CDATA[
      @font-face {{
        font-family: 'DualData';
        font-style: normal;
        font-weight: 400;
        src: url({first_data_url}) format('truetype'), url({second_data_url}) format('truetype');
      }}
    ]]></style>
  </defs>
  <text x="10" y="40" font-family="'DualData'">A</text>
</svg>"#
    );

    let output = embed_svg_fonts(&svg, |_query| {
        Err("resolver should not be called when embedded data source is present".to_string())
    })
    .expect("embedding should succeed");

    let decoded = extract_data_urls(&output)
        .into_iter()
        .map(|url| decode_data_url(&url).1)
        .collect::<Vec<_>>();
    assert!(
        decoded.iter().any(|bytes| bytes == &first_bytes),
        "expected first data URL payload to be used"
    );
    assert!(
        !decoded.iter().any(|bytes| bytes == &second_bytes),
        "did not expect later data URL payload to be selected"
    );
}

#[test]
fn existing_font_face_without_any_data_src_errors() {
    let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="220" height="80">
  <defs>
    <style><![CDATA[
      @font-face {
        font-family: 'NoData';
        font-style: normal;
        font-weight: 400;
        src: local('Definitely Missing Font'), url('https://example.com/external.ttf') format('truetype');
      }
    ]]></style>
  </defs>
  <text x="10" y="40" font-family="'NoData'">A</text>
</svg>"#;

    let err = embed_svg_fonts(svg, |_query| Ok(fixture_font_path("font-a.ttf")))
        .expect_err("font faces without data src should fail");
    assert!(
        err.contains("data"),
        "expected data-source validation error, got: {err}"
    );
}

#[test]
fn existing_font_face_with_relative_weight_errors() {
    let data_url = fixture_data_url("font-a.ttf");
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="220" height="80">
  <defs>
    <style><![CDATA[
      @font-face {{
        font-family: 'RelativeWeight';
        font-style: normal;
        font-weight: bolder;
        src: url({data_url}) format('truetype');
      }}
    ]]></style>
  </defs>
  <text x="10" y="40" font-family="'RelativeWeight'">A</text>
</svg>"#
    );

    let err = embed_svg_fonts(&svg, |_query| Ok(fixture_font_path("font-a.ttf")))
        .expect_err("@font-face with relative weight should fail");
    assert!(
        err.contains("relative font-weight") || err.contains("bolder") || err.contains("lighter"),
        "expected relative font-weight error, got: {err}"
    );
}

#[test]
fn existing_font_face_with_relative_weight_lighter_errors() {
    let data_url = fixture_data_url("font-a.ttf");
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="220" height="80">
  <defs>
    <style><![CDATA[
      @font-face {{
        font-family: 'RelativeWeightLighter';
        font-style: normal;
        font-weight: lighter;
        src: url({data_url}) format('truetype');
      }}
    ]]></style>
  </defs>
  <text x="10" y="40" font-family="'RelativeWeightLighter'">A</text>
</svg>"#
    );

    let err = embed_svg_fonts(&svg, |_query| Ok(fixture_font_path("font-a.ttf")))
        .expect_err("@font-face with relative weight should fail");
    assert!(
        err.contains("relative font-weight") || err.contains("bolder") || err.contains("lighter"),
        "expected relative font-weight error, got: {err}"
    );
}

#[test]
fn existing_font_face_weight_range_matches_middle_value() {
    let data_url = fixture_data_url("font-a.ttf");
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="220" height="80">
  <defs>
    <style><![CDATA[
      @font-face {{
        font-family: 'RangeMid';
        font-style: normal;
        font-weight: 100 300;
        src: url({data_url}) format('truetype');
      }}
    ]]></style>
  </defs>
  <text x="10" y="40" font-family="'RangeMid'" font-weight="200">A</text>
</svg>"#
    );

    let calls = Arc::new(Mutex::new(0usize));
    let calls_for_resolver = Arc::clone(&calls);
    let _output = embed_svg_fonts(&svg, move |_query| {
        let mut guard = calls_for_resolver
            .lock()
            .expect("call counter mutex poisoned");
        *guard += 1;
        Ok(fixture_font_path("font-b.ttf"))
    })
    .expect("embedding should succeed");

    assert_eq!(
        *calls.lock().expect("call counter mutex poisoned"),
        0,
        "resolver should not be called when range includes requested weight"
    );
}

#[test]
fn existing_font_face_weight_range_matches_lower_bound() {
    let data_url = fixture_data_url("font-a.ttf");
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="220" height="80">
  <defs>
    <style><![CDATA[
      @font-face {{
        font-family: 'RangeLower';
        font-style: normal;
        font-weight: 100 300;
        src: url({data_url}) format('truetype');
      }}
    ]]></style>
  </defs>
  <text x="10" y="40" font-family="'RangeLower'" font-weight="100">A</text>
</svg>"#
    );

    let calls = Arc::new(Mutex::new(0usize));
    let calls_for_resolver = Arc::clone(&calls);
    let _output = embed_svg_fonts(&svg, move |_query| {
        let mut guard = calls_for_resolver
            .lock()
            .expect("call counter mutex poisoned");
        *guard += 1;
        Ok(fixture_font_path("font-b.ttf"))
    })
    .expect("embedding should succeed");

    assert_eq!(
        *calls.lock().expect("call counter mutex poisoned"),
        0,
        "resolver should not be called when query is at lower bound"
    );
}

#[test]
fn existing_font_face_weight_range_matches_upper_bound() {
    let data_url = fixture_data_url("font-a.ttf");
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="220" height="80">
  <defs>
    <style><![CDATA[
      @font-face {{
        font-family: 'RangeUpper';
        font-style: normal;
        font-weight: 100 300;
        src: url({data_url}) format('truetype');
      }}
    ]]></style>
  </defs>
  <text x="10" y="40" font-family="'RangeUpper'" font-weight="300">A</text>
</svg>"#
    );

    let calls = Arc::new(Mutex::new(0usize));
    let calls_for_resolver = Arc::clone(&calls);
    let _output = embed_svg_fonts(&svg, move |_query| {
        let mut guard = calls_for_resolver
            .lock()
            .expect("call counter mutex poisoned");
        *guard += 1;
        Ok(fixture_font_path("font-b.ttf"))
    })
    .expect("embedding should succeed");

    assert_eq!(
        *calls.lock().expect("call counter mutex poisoned"),
        0,
        "resolver should not be called when query is at upper bound"
    );
}

#[test]
fn existing_font_face_weight_range_outside_does_not_match() {
    let data_url = fixture_data_url("font-a.ttf");
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="220" height="80">
  <defs>
    <style><![CDATA[
      @font-face {{
        font-family: 'RangeOutside';
        font-style: normal;
        font-weight: 100 300;
        src: url({data_url}) format('truetype');
      }}
    ]]></style>
  </defs>
  <text x="10" y="40" font-family="'RangeOutside'" font-weight="301">A</text>
</svg>"#
    );

    let calls = Arc::new(Mutex::new(0usize));
    let calls_for_resolver = Arc::clone(&calls);
    let output = embed_svg_fonts(&svg, move |_query| {
        let mut guard = calls_for_resolver
            .lock()
            .expect("call counter mutex poisoned");
        *guard += 1;
        Ok(fixture_font_path("font-b.ttf"))
    })
    .expect("embedding should succeed by using resolver for out-of-range weight");

    assert!(output.contains("@font-face"));
    assert!(
        *calls.lock().expect("call counter mutex poisoned") >= 1,
        "resolver should be called when requested weight is outside existing range"
    );
}

#[test]
fn existing_font_face_weight_descending_range_errors() {
    let data_url = fixture_data_url("font-a.ttf");
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="220" height="80">
  <defs>
    <style><![CDATA[
      @font-face {{
        font-family: 'RangeDescending';
        font-style: normal;
        font-weight: 300 100;
        src: url({data_url}) format('truetype');
      }}
    ]]></style>
  </defs>
  <text x="10" y="40" font-family="'RangeDescending'" font-weight="200">A</text>
</svg>"#
    );

    let err = embed_svg_fonts(&svg, |_query| Ok(fixture_font_path("font-b.ttf")))
        .expect_err("descending @font-face weight range should fail");
    assert!(
        err.contains("range") || err.contains("descending") || err.contains("font-weight"),
        "expected range-order validation error, got: {err}"
    );
}

#[test]
fn existing_font_face_weight_overlapping_ranges_match_without_resolver() {
    let data_url_a = fixture_data_url("font-a.ttf");
    let data_url_b = fixture_data_url("font-b.ttf");
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="240" height="90">
  <defs>
    <style><![CDATA[
      @font-face {{
        font-family: 'RangeOverlap';
        font-style: normal;
        font-weight: 100 500;
        src: url({data_url_a}) format('truetype');
      }}
      @font-face {{
        font-family: 'RangeOverlap';
        font-style: normal;
        font-weight: 400 700;
        src: url({data_url_b}) format('truetype');
      }}
    ]]></style>
  </defs>
  <text x="10" y="40" font-family="'RangeOverlap'" font-weight="450">A</text>
</svg>"#
    );

    let calls = Arc::new(Mutex::new(0usize));
    let calls_for_resolver = Arc::clone(&calls);
    let _output = embed_svg_fonts(&svg, move |_query| {
        let mut guard = calls_for_resolver
            .lock()
            .expect("call counter mutex poisoned");
        *guard += 1;
        Ok(fixture_font_path("font-c.ttf"))
    })
    .expect("embedding should succeed");

    assert_eq!(
        *calls.lock().expect("call counter mutex poisoned"),
        0,
        "resolver should not be called when at least one overlap range includes requested weight"
    );
}

#[test]
fn existing_font_face_single_weight_stays_exact_match() {
    let data_url = fixture_data_url("font-a.ttf");
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="220" height="80">
  <defs>
    <style><![CDATA[
      @font-face {{
        font-family: 'SingleExact';
        font-style: normal;
        font-weight: 300;
        src: url({data_url}) format('truetype');
      }}
    ]]></style>
  </defs>
  <text x="10" y="40" font-family="'SingleExact'" font-weight="301">A</text>
</svg>"#
    );

    let calls = Arc::new(Mutex::new(0usize));
    let calls_for_resolver = Arc::clone(&calls);
    let output = embed_svg_fonts(&svg, move |_query| {
        let mut guard = calls_for_resolver
            .lock()
            .expect("call counter mutex poisoned");
        *guard += 1;
        Ok(fixture_font_path("font-b.ttf"))
    })
    .expect("embedding should succeed by using resolver for non-matching single weight");

    assert!(output.contains("@font-face"));
    assert!(
        *calls.lock().expect("call counter mutex poisoned") >= 1,
        "resolver should be called when query does not exactly match single descriptor weight"
    );
}

#[test]
fn existing_font_face_missing_weight_defaults_to_400() {
    let data_url = fixture_data_url("font-a.ttf");
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="220" height="80">
  <defs>
    <style><![CDATA[
      @font-face {{
        font-family: 'MissingWeight';
        font-style: normal;
        src: url({data_url}) format('truetype');
      }}
    ]]></style>
  </defs>
  <text x="10" y="40" font-family="'MissingWeight'">A</text>
</svg>"#
    );

    let calls = Arc::new(Mutex::new(0usize));
    let calls_for_resolver = Arc::clone(&calls);
    let _output = embed_svg_fonts(&svg, move |_query| {
        let mut guard = calls_for_resolver
            .lock()
            .expect("call counter mutex poisoned");
        *guard += 1;
        Ok(fixture_font_path("font-b.ttf"))
    })
    .expect("embedding should succeed");

    assert_eq!(
        *calls.lock().expect("call counter mutex poisoned"),
        0,
        "resolver should not be called when missing descriptor defaults to 400 and query is default 400"
    );
}

#[test]
fn existing_font_face_stretch_range_matches_middle_value() {
    let data_url = fixture_data_url("font-a.ttf");
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="220" height="80">
  <defs>
    <style><![CDATA[
      @font-face {{
        font-family: 'StretchRangeMid';
        font-style: normal;
        font-stretch: condensed expanded;
        src: url({data_url}) format('truetype');
      }}
    ]]></style>
  </defs>
  <text x="10" y="40" font-family="'StretchRangeMid'" font-stretch="normal">A</text>
</svg>"#
    );

    let calls = Arc::new(Mutex::new(0usize));
    let calls_for_resolver = Arc::clone(&calls);
    let _output = embed_svg_fonts(&svg, move |_query| {
        let mut guard = calls_for_resolver
            .lock()
            .expect("call counter mutex poisoned");
        *guard += 1;
        Ok(fixture_font_path("font-b.ttf"))
    })
    .expect("embedding should succeed");

    assert_eq!(
        *calls.lock().expect("call counter mutex poisoned"),
        0,
        "resolver should not be called when stretch range includes requested stretch"
    );
}

#[test]
fn existing_font_face_stretch_range_matches_lower_bound() {
    let data_url = fixture_data_url("font-a.ttf");
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="220" height="80">
  <defs>
    <style><![CDATA[
      @font-face {{
        font-family: 'StretchRangeLower';
        font-style: normal;
        font-stretch: condensed expanded;
        src: url({data_url}) format('truetype');
      }}
    ]]></style>
  </defs>
  <text x="10" y="40" font-family="'StretchRangeLower'" font-stretch="condensed">A</text>
</svg>"#
    );

    let calls = Arc::new(Mutex::new(0usize));
    let calls_for_resolver = Arc::clone(&calls);
    let _output = embed_svg_fonts(&svg, move |_query| {
        let mut guard = calls_for_resolver
            .lock()
            .expect("call counter mutex poisoned");
        *guard += 1;
        Ok(fixture_font_path("font-b.ttf"))
    })
    .expect("embedding should succeed");

    assert_eq!(
        *calls.lock().expect("call counter mutex poisoned"),
        0,
        "resolver should not be called when requested stretch is at lower bound"
    );
}

#[test]
fn existing_font_face_stretch_range_matches_upper_bound() {
    let data_url = fixture_data_url("font-a.ttf");
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="220" height="80">
  <defs>
    <style><![CDATA[
      @font-face {{
        font-family: 'StretchRangeUpper';
        font-style: normal;
        font-stretch: condensed expanded;
        src: url({data_url}) format('truetype');
      }}
    ]]></style>
  </defs>
  <text x="10" y="40" font-family="'StretchRangeUpper'" font-stretch="expanded">A</text>
</svg>"#
    );

    let calls = Arc::new(Mutex::new(0usize));
    let calls_for_resolver = Arc::clone(&calls);
    let _output = embed_svg_fonts(&svg, move |_query| {
        let mut guard = calls_for_resolver
            .lock()
            .expect("call counter mutex poisoned");
        *guard += 1;
        Ok(fixture_font_path("font-b.ttf"))
    })
    .expect("embedding should succeed");

    assert_eq!(
        *calls.lock().expect("call counter mutex poisoned"),
        0,
        "resolver should not be called when requested stretch is at upper bound"
    );
}

#[test]
fn existing_font_face_stretch_range_outside_does_not_match() {
    let data_url = fixture_data_url("font-a.ttf");
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="220" height="80">
  <defs>
    <style><![CDATA[
      @font-face {{
        font-family: 'StretchRangeOutside';
        font-style: normal;
        font-stretch: condensed normal;
        src: url({data_url}) format('truetype');
      }}
    ]]></style>
  </defs>
  <text x="10" y="40" font-family="'StretchRangeOutside'" font-stretch="expanded">A</text>
</svg>"#
    );

    let calls = Arc::new(Mutex::new(0usize));
    let calls_for_resolver = Arc::clone(&calls);
    let output = embed_svg_fonts(&svg, move |_query| {
        let mut guard = calls_for_resolver
            .lock()
            .expect("call counter mutex poisoned");
        *guard += 1;
        Ok(fixture_font_path("font-b.ttf"))
    })
    .expect("embedding should succeed by using resolver for out-of-range stretch");

    assert!(output.contains("@font-face"));
    assert!(
        *calls.lock().expect("call counter mutex poisoned") >= 1,
        "resolver should be called when requested stretch is outside existing range"
    );
}

#[test]
fn existing_font_face_stretch_descending_range_errors() {
    let data_url = fixture_data_url("font-a.ttf");
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="220" height="80">
  <defs>
    <style><![CDATA[
      @font-face {{
        font-family: 'StretchRangeDescending';
        font-style: normal;
        font-stretch: expanded condensed;
        src: url({data_url}) format('truetype');
      }}
    ]]></style>
  </defs>
  <text x="10" y="40" font-family="'StretchRangeDescending'" font-stretch="normal">A</text>
</svg>"#
    );

    let err = embed_svg_fonts(&svg, |_query| Ok(fixture_font_path("font-b.ttf")))
        .expect_err("descending @font-face stretch range should fail");
    assert!(
        err.contains("stretch") || err.contains("range") || err.contains("descending"),
        "expected stretch range-order validation error, got: {err}"
    );
}

#[test]
fn existing_font_face_stretch_overlapping_ranges_match_without_resolver() {
    let data_url_a = fixture_data_url("font-a.ttf");
    let data_url_b = fixture_data_url("font-b.ttf");
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="240" height="90">
  <defs>
    <style><![CDATA[
      @font-face {{
        font-family: 'StretchOverlap';
        font-style: normal;
        font-stretch: condensed normal;
        src: url({data_url_a}) format('truetype');
      }}
      @font-face {{
        font-family: 'StretchOverlap';
        font-style: normal;
        font-stretch: normal expanded;
        src: url({data_url_b}) format('truetype');
      }}
    ]]></style>
  </defs>
  <text x="10" y="40" font-family="'StretchOverlap'" font-stretch="normal">A</text>
</svg>"#
    );

    let calls = Arc::new(Mutex::new(0usize));
    let calls_for_resolver = Arc::clone(&calls);
    let _output = embed_svg_fonts(&svg, move |_query| {
        let mut guard = calls_for_resolver
            .lock()
            .expect("call counter mutex poisoned");
        *guard += 1;
        Ok(fixture_font_path("font-c.ttf"))
    })
    .expect("embedding should succeed");

    assert_eq!(
        *calls.lock().expect("call counter mutex poisoned"),
        0,
        "resolver should not be called when at least one overlap stretch range includes requested stretch"
    );
}

#[test]
fn existing_font_face_stretch_single_value_stays_exact_match() {
    let data_url = fixture_data_url("font-a.ttf");
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="220" height="80">
  <defs>
    <style><![CDATA[
      @font-face {{
        font-family: 'StretchSingleExact';
        font-style: normal;
        font-stretch: condensed;
        src: url({data_url}) format('truetype');
      }}
    ]]></style>
  </defs>
  <text x="10" y="40" font-family="'StretchSingleExact'" font-stretch="normal">A</text>
</svg>"#
    );

    let calls = Arc::new(Mutex::new(0usize));
    let calls_for_resolver = Arc::clone(&calls);
    let output = embed_svg_fonts(&svg, move |_query| {
        let mut guard = calls_for_resolver
            .lock()
            .expect("call counter mutex poisoned");
        *guard += 1;
        Ok(fixture_font_path("font-b.ttf"))
    })
    .expect("embedding should succeed by using resolver for non-matching single stretch");

    assert!(output.contains("@font-face"));
    assert!(
        *calls.lock().expect("call counter mutex poisoned") >= 1,
        "resolver should be called when query does not exactly match single descriptor stretch"
    );
}

#[test]
fn existing_font_face_missing_stretch_defaults_to_normal() {
    let data_url = fixture_data_url("font-a.ttf");
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="220" height="80">
  <defs>
    <style><![CDATA[
      @font-face {{
        font-family: 'MissingStretch';
        font-style: normal;
        src: url({data_url}) format('truetype');
      }}
    ]]></style>
  </defs>
  <text x="10" y="40" font-family="'MissingStretch'">A</text>
</svg>"#
    );

    let calls = Arc::new(Mutex::new(0usize));
    let calls_for_resolver = Arc::clone(&calls);
    let _output = embed_svg_fonts(&svg, move |_query| {
        let mut guard = calls_for_resolver
            .lock()
            .expect("call counter mutex poisoned");
        *guard += 1;
        Ok(fixture_font_path("font-b.ttf"))
    })
    .expect("embedding should succeed");

    assert_eq!(
        *calls.lock().expect("call counter mutex poisoned"),
        0,
        "resolver should not be called when missing descriptor defaults to normal and query is normal"
    );
}

#[test]
fn existing_font_face_style_oblique_range_matches_default_angle() {
    let data_url = fixture_data_url("font-a.ttf");
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="220" height="80">
  <defs>
    <style><![CDATA[
      @font-face {{
        font-family: 'StyleObliqueRangeMatch';
        font-style: oblique 10deg 20deg;
        src: url({data_url}) format('truetype');
      }}
    ]]></style>
  </defs>
  <text x="10" y="40" font-family="'StyleObliqueRangeMatch'" font-style="oblique">A</text>
</svg>"#
    );

    let calls = Arc::new(Mutex::new(0usize));
    let calls_for_resolver = Arc::clone(&calls);
    let _output = embed_svg_fonts(&svg, move |_query| {
        let mut guard = calls_for_resolver
            .lock()
            .expect("call counter mutex poisoned");
        *guard += 1;
        Ok(fixture_font_path("font-b.ttf"))
    })
    .expect("embedding should succeed");

    assert_eq!(
        *calls.lock().expect("call counter mutex poisoned"),
        0,
        "resolver should not be called when oblique range includes default 14deg"
    );
}

#[test]
fn existing_font_face_style_oblique_range_outside_default_angle_does_not_match() {
    let data_url = fixture_data_url("font-a.ttf");
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="220" height="80">
  <defs>
    <style><![CDATA[
      @font-face {{
        font-family: 'StyleObliqueRangeMiss';
        font-style: oblique 20deg 30deg;
        src: url({data_url}) format('truetype');
      }}
    ]]></style>
  </defs>
  <text x="10" y="40" font-family="'StyleObliqueRangeMiss'" font-style="oblique">A</text>
</svg>"#
    );

    let calls = Arc::new(Mutex::new(0usize));
    let calls_for_resolver = Arc::clone(&calls);
    let output = embed_svg_fonts(&svg, move |_query| {
        let mut guard = calls_for_resolver
            .lock()
            .expect("call counter mutex poisoned");
        *guard += 1;
        Ok(fixture_font_path("font-b.ttf"))
    })
    .expect("embedding should succeed by using resolver for unmatched oblique angle");

    assert!(output.contains("@font-face"));
    assert!(
        *calls.lock().expect("call counter mutex poisoned") >= 1,
        "resolver should be called when oblique range does not include default 14deg"
    );
}

#[test]
fn existing_font_face_style_descending_oblique_range_errors() {
    let data_url = fixture_data_url("font-a.ttf");
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="220" height="80">
  <defs>
    <style><![CDATA[
      @font-face {{
        font-family: 'StyleObliqueDescending';
        font-style: oblique 20deg 10deg;
        src: url({data_url}) format('truetype');
      }}
    ]]></style>
  </defs>
  <text x="10" y="40" font-family="'StyleObliqueDescending'" font-style="oblique">A</text>
</svg>"#
    );

    let err = embed_svg_fonts(&svg, |_query| Ok(fixture_font_path("font-b.ttf")))
        .expect_err("descending @font-face oblique range should fail");
    assert!(
        err.contains("font-style") || err.contains("oblique") || err.contains("descending"),
        "expected oblique style range-order validation error, got: {err}"
    );
}

#[test]
fn existing_font_face_style_italic_does_not_match_oblique() {
    let data_url = fixture_data_url("font-a.ttf");
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="220" height="80">
  <defs>
    <style><![CDATA[
      @font-face {{
        font-family: 'StyleStrictItalic';
        font-style: italic;
        src: url({data_url}) format('truetype');
      }}
    ]]></style>
  </defs>
  <text x="10" y="40" font-family="'StyleStrictItalic'" font-style="oblique">A</text>
</svg>"#
    );

    let calls = Arc::new(Mutex::new(0usize));
    let calls_for_resolver = Arc::clone(&calls);
    let output = embed_svg_fonts(&svg, move |_query| {
        let mut guard = calls_for_resolver
            .lock()
            .expect("call counter mutex poisoned");
        *guard += 1;
        Ok(fixture_font_path("font-b.ttf"))
    })
    .expect("embedding should succeed by using resolver for strict style mismatch");

    assert!(output.contains("@font-face"));
    assert!(
        *calls.lock().expect("call counter mutex poisoned") >= 1,
        "resolver should be called when italic face does not match oblique request"
    );
}

#[test]
fn existing_font_face_style_oblique_does_not_match_normal() {
    let data_url = fixture_data_url("font-a.ttf");
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="220" height="80">
  <defs>
    <style><![CDATA[
      @font-face {{
        font-family: 'StyleStrictOblique';
        font-style: oblique 10deg 20deg;
        src: url({data_url}) format('truetype');
      }}
    ]]></style>
  </defs>
  <text x="10" y="40" font-family="'StyleStrictOblique'">A</text>
</svg>"#
    );

    let calls = Arc::new(Mutex::new(0usize));
    let calls_for_resolver = Arc::clone(&calls);
    let output = embed_svg_fonts(&svg, move |_query| {
        let mut guard = calls_for_resolver
            .lock()
            .expect("call counter mutex poisoned");
        *guard += 1;
        Ok(fixture_font_path("font-b.ttf"))
    })
    .expect("embedding should succeed by using resolver for strict style mismatch");

    assert!(output.contains("@font-face"));
    assert!(
        *calls.lock().expect("call counter mutex poisoned") >= 1,
        "resolver should be called when oblique face does not match normal request"
    );
}

#[test]
fn style_with_font_face_and_other_rules_preserves_non_font_rules() {
    let data_url = fixture_data_url("font-a.ttf");
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="220" height="80">
  <defs>
    <style><![CDATA[
      @font-face {{
        font-family: 'MixedSans';
        font-style: normal;
        font-weight: 400;
        src: url({data_url}) format('truetype');
      }}

      .label {{
        fill: #cc0000;
      }}
    ]]></style>
  </defs>
  <text class="label" x="10" y="40" font-family="'MixedSans'">A</text>
</svg>"#
    );

    let calls: Arc<Mutex<usize>> = Arc::new(Mutex::new(0));
    let calls_for_resolver = Arc::clone(&calls);
    let output = embed_svg_fonts(&svg, move |_query| {
        let mut guard = calls_for_resolver
            .lock()
            .expect("call counter mutex poisoned");
        *guard += 1;
        Err("resolver should not be called when embedded data source is present".to_string())
    })
    .expect("embedding should succeed and preserve non-font style rules");

    assert_eq!(
        *calls.lock().expect("call counter mutex poisoned"),
        0,
        "resolver should not be called when existing data URL font-face is available"
    );

    let doc = roxmltree::Document::parse(&output).expect("output should remain valid SVG XML");
    let mut found_label_style_rule = false;
    let mut found_font_face_rule = false;

    for style_node in doc
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "style")
    {
        let css = style_node.text().unwrap_or("");
        let stylesheet = StyleSheet::parse(css, ParserOptions::default())
            .expect("output style content should be valid CSS");

        for rule in &stylesheet.rules.0 {
            match rule {
                CssRule::FontFace(_) => {
                    found_font_face_rule = true;
                }
                CssRule::Style(style_rule) => {
                    let selector_css = style_rule
                        .selectors
                        .to_css_string(PrinterOptions::default())
                        .expect("selector should serialize");
                    if selector_css.contains(".label") {
                        found_label_style_rule = true;
                    }
                }
                _ => {}
            }
        }
    }

    assert!(
        found_label_style_rule,
        "expected non-font .label rule to be preserved"
    );
    assert!(
        found_font_face_rule,
        "expected output to include at least one @font-face rule"
    );
}

#[test]
fn defs_with_non_style_content_is_preserved_when_font_face_only_style_is_removed() {
    let data_url = fixture_data_url("font-a.ttf");
    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="220" height="80">
  <defs>
    <style><![CDATA[
      @font-face {{
        font-family: 'DefsMixedSans';
        font-style: normal;
        font-weight: 400;
        src: url({data_url}) format('truetype');
      }}
    ]]></style>
    <linearGradient id="kept-gradient">
      <stop offset="0%" stop-color="#ff0000"/>
      <stop offset="100%" stop-color="#0000ff"/>
    </linearGradient>
  </defs>
  <text x="10" y="40" font-family="'DefsMixedSans'">A</text>
</svg>"##
    );

    let output = embed_svg_fonts(&svg, |_query| {
        Err("resolver should not be called when embedded data source is present".to_string())
    })
    .expect("embedding should succeed");

    let doc = roxmltree::Document::parse(&output).expect("output should remain valid SVG XML");

    let defs_count = doc
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "defs")
        .count();
    assert!(defs_count > 0, "expected <defs> to remain present");

    let gradient_exists = doc.descendants().any(|n| {
        n.is_element()
            && n.tag_name().name() == "linearGradient"
            && n.attribute("id") == Some("kept-gradient")
    });
    assert!(
        gradient_exists,
        "expected non-style defs content to be preserved"
    );
}

#[test]
fn style_open_tag_with_gt_in_attribute_value_is_handled_correctly() {
    let data_url = fixture_data_url("font-a.ttf");
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="220" height="80">
  <defs>
    <style data-note="a > b"><![CDATA[
      @font-face {{
        font-family: 'AttrGtSans';
        font-style: normal;
        font-weight: 400;
        src: url({data_url}) format('truetype');
      }}

      .label {{
        fill: red;
      }}
    ]]></style>
  </defs>
  <text class="label" x="10" y="40" font-family="'AttrGtSans'">A</text>
</svg>"#
    );

    let output = embed_svg_fonts(&svg, |_query| {
        Err("resolver should not be called when embedded data source is present".to_string())
    })
    .expect("embedding should succeed when style attribute contains '>'");

    let doc = roxmltree::Document::parse(&output).expect("output should remain valid SVG XML");
    let mut found_label_style_rule = false;

    for style_node in doc
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "style")
    {
        let css = style_node.text().unwrap_or("");
        let stylesheet = StyleSheet::parse(css, ParserOptions::default())
            .expect("output style content should be valid CSS");

        for rule in &stylesheet.rules.0 {
            if let CssRule::Style(style_rule) = rule {
                let selector_css = style_rule
                    .selectors
                    .to_css_string(PrinterOptions::default())
                    .expect("selector should serialize");
                if selector_css.contains(".label") {
                    found_label_style_rule = true;
                }
            }
        }
    }

    assert!(
        found_label_style_rule,
        "expected .label rule to be preserved when style open-tag attribute contains '>'"
    );
}

#[test]
fn unicode_range_is_removed_after_processing() {
    let data_url = fixture_data_url("font-b.ttf");
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="220" height="80">
  <defs>
    <style><![CDATA[
      @font-face {{
        font-family: 'SubsetMe';
        font-style: normal;
        font-weight: 400;
        src: url({data_url}) format('truetype');
        unicode-range: U+0041;
      }}
    ]]></style>
  </defs>
  <text x="10" y="40" font-family="'SubsetMe'">A</text>
</svg>"#
    );

    let output = embed_svg_fonts(&svg, |_query| {
        Err("resolver should not be needed for covered glyphs".to_string())
    })
    .expect("embedding should succeed with ranged pre-inlined face");

    assert!(
        !output.to_ascii_lowercase().contains("unicode-range"),
        "unicode-range should be removed after materializing subset"
    );
}

#[test]
fn unicode_range_gap_triggers_fallback_resolution() {
    let data_url = fixture_data_url("font-b.ttf");
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="220" height="80">
  <defs>
    <style><![CDATA[
      @font-face {{
        font-family: 'SubsetGap';
        font-style: normal;
        font-weight: 400;
        src: url({data_url}) format('truetype');
        unicode-range: U+0041;
      }}
    ]]></style>
  </defs>
  <text x="10" y="40" font-family="'SubsetGap'">AB</text>
</svg>"#
    );

    let seen_queries: Arc<Mutex<Vec<FontQuery>>> = Arc::new(Mutex::new(Vec::new()));
    let seen_for_resolver = Arc::clone(&seen_queries);
    let font_b_path = fixture_font_path("font-b.ttf");

    let _output = embed_svg_fonts(&svg, move |query| {
        seen_for_resolver
            .lock()
            .expect("query capture mutex poisoned")
            .push(query.clone());
        Ok(font_b_path.clone())
    })
    .expect("embedding should succeed by resolving missing range characters");

    let captured = seen_queries.lock().expect("query capture mutex poisoned");
    assert!(
        captured.iter().any(|q| q.missing_char == Some('B')),
        "expected fallback resolution for missing char outside unicode-range"
    );
}

#[test]
fn output_font_face_order_is_stable_regardless_of_input_order() {
    let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="220" height="90">
  <text x="10" y="30" font-family="'FontA'">A</text>
  <text x="10" y="70" font-family="'FontB'">A</text>
</svg>"#;
    let font_a_path = fixture_font_path("font-a.ttf");
    let font_b_path = fixture_font_path("font-b.ttf");

    let render = || {
        embed_svg_fonts(svg, {
            let font_a_path = font_a_path.clone();
            let font_b_path = font_b_path.clone();
            move |query| {
                if query.families.iter().any(|f| f == "FontA") {
                    return Ok(font_a_path.clone());
                }
                if query.families.iter().any(|f| f == "FontB") {
                    return Ok(font_b_path.clone());
                }
                Err(format!("unexpected families: {:?}", query.families))
            }
        })
        .expect("embedding should succeed")
    };

    let out1 = render();
    let out2 = render();

    assert_eq!(
        out1, out2,
        "output should be deterministic for identical input"
    );
}

#[test]
fn dedupe_key_includes_descriptors_not_just_family() {
    let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="260" height="100">
  <text x="10" y="30" font-family="'FontA'" font-weight="400">A</text>
  <text x="10" y="70" font-family="'FontA'" font-weight="700">A</text>
</svg>"#;
    let font_path = fixture_font_path("font-a.ttf");

    let output = embed_svg_fonts(svg, move |_query| Ok(font_path.clone()))
        .expect("embedding should succeed");

    let font_face_count = output.matches("@font-face").count();
    assert_eq!(
        font_face_count, 2,
        "same family with different descriptors should not be deduped into one face"
    );
}

#[test]
fn unicode_range_multiple_faces_same_descriptor_are_merged_deterministically() {
    let font_b_data = fixture_data_url("font-b.ttf");
    let font_c_data = fixture_data_url("font-c.ttf");
    let font_c_bytes = fixture_font_bytes("font-c.ttf");
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="260" height="80">
  <defs>
    <style><![CDATA[
      @font-face {{
        font-family: 'SubsetMerge';
        font-style: normal;
        font-weight: 400;
        src: url({font_b_data}) format('truetype');
        unicode-range: U+0041-0042;
      }}
      @font-face {{
        font-family: 'SubsetMerge';
        font-style: normal;
        font-weight: 400;
        src: url({font_c_data}) format('truetype');
        unicode-range: U+0043;
      }}
    ]]></style>
  </defs>
  <text x="10" y="40" font-family="'SubsetMerge'">ABC</text>
</svg>"#
    );

    let out1 = embed_svg_fonts(&svg, |_query| {
        Err("resolver should not be needed when existing ranged faces cover text".to_string())
    })
    .expect("first run should succeed");
    let out2 = embed_svg_fonts(&out1, |_query| {
        Err("resolver should not be needed on second run".to_string())
    })
    .expect("second run should succeed");

    assert_eq!(out1, out2, "output should be stable across runs");
    let font_face_count = out1.matches("@font-face").count();
    assert_eq!(
        font_face_count, 1,
        "faces that only differ by unicode-range should merge to one deterministic emitted face"
    );

    let decoded = extract_data_urls(&out1)
        .into_iter()
        .map(|url| decode_data_url(&url).1)
        .collect::<Vec<_>>();
    assert!(
        decoded.iter().any(|bytes| bytes == &font_c_bytes),
        "expected merged output to include payload with full ABC coverage"
    );
}

#[test]
fn unicode_range_fallback_chain_is_idempotent() {
    let data_url = fixture_data_url("font-a.ttf");
    let font_b_path = fixture_font_path("font-b.ttf");
    let font_c_path = fixture_font_path("font-c.ttf");
    let resolver_calls_first = Arc::new(Mutex::new(0usize));
    let resolver_calls_first_clone = Arc::clone(&resolver_calls_first);
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="260" height="80">
  <defs>
    <style><![CDATA[
      @font-face {{
        font-family: 'SubsetChain';
        font-style: normal;
        font-weight: 400;
        src: url({data_url}) format('truetype');
        unicode-range: U+0041;
      }}
    ]]></style>
  </defs>
  <text x="10" y="40" font-family="'SubsetChain'">ABC</text>
</svg>"#
    );

    let out1 = embed_svg_fonts(&svg, move |query| {
        let mut guard = resolver_calls_first_clone
            .lock()
            .expect("first call counter mutex poisoned");
        *guard += 1;

        match query.missing_char {
            Some('B') => Ok(font_b_path.clone()),
            Some('C') => Ok(font_c_path.clone()),
            None => Err("did not expect primary resolution for pre-inlined face".to_string()),
            Some(other) => Err(format!("unexpected fallback char: {other}")),
        }
    })
    .expect("first run should succeed");

    let resolver_calls_second = Arc::new(Mutex::new(0usize));
    let resolver_calls_second_clone = Arc::clone(&resolver_calls_second);
    let out2 = embed_svg_fonts(&out1, move |_query| {
        let mut guard = resolver_calls_second_clone
            .lock()
            .expect("second call counter mutex poisoned");
        *guard += 1;
        Ok(fixture_font_path("font-c.ttf"))
    })
    .expect("second run should succeed");

    assert!(
        *resolver_calls_first
            .lock()
            .expect("first call counter mutex poisoned")
            >= 2,
        "first run should resolve at least B and C through fallback"
    );
    assert_eq!(
        *resolver_calls_second
            .lock()
            .expect("second call counter mutex poisoned"),
        0,
        "second run should not need resolver after normalization"
    );
    assert_eq!(out1, out2, "fallback-chain output should be idempotent");
}
