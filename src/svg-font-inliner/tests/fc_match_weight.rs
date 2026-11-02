use std::path::Path;

use svg_font_inliner::{resolve_font_with_fc_match, FontQuery};

// fc-match only sees the fixtures, where "Fixture Static" has separate Regular (weight class 400) and Bold (700) files.
// This test binary has a single test, as it sets FONTCONFIG_FILE for the whole process.
#[test]
fn css_weights_select_the_matching_static_face() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures");
    let config_dir = tempfile::tempdir().expect("should create fontconfig dir");
    let config = config_dir.path().join("fonts.conf");
    std::fs::write(
        &config,
        format!(
            r#"<?xml version="1.0"?><!DOCTYPE fontconfig SYSTEM "urn:fontconfig:fonts.dtd"><fontconfig><dir>{}</dir><cachedir>{}</cachedir></fontconfig>"#,
            fixtures.display(),
            config_dir.path().join("cache").display()
        ),
    )
    .expect("should write fontconfig file");
    std::env::set_var("FONTCONFIG_FILE", &config);

    let resolve = |weight| {
        let query = FontQuery {
            families: vec!["Fixture Static".to_string()],
            style: "Normal".to_string(),
            weight,
            stretch: "Normal".to_string(),
            variations: String::new(),
            missing_char: None,
        };
        let path = resolve_font_with_fc_match(&query).expect("fc-match should resolve the family");
        path.file_name()
            .and_then(|name| name.to_str())
            .map(str::to_string)
            .expect("resolved path should have a file name")
    };

    assert_eq!(resolve(400), "static-regular.ttf");
    assert_eq!(resolve(700), "static-bold.ttf");
}
