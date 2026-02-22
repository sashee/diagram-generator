use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

use base64::Engine;

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FontQuery {
    pub families: Vec<String>,
    pub style: String,
    pub weight: u16,
    pub stretch: String,
    pub variations: String,
    pub missing_char: Option<char>,
}

#[derive(Clone, Debug)]
struct EmbeddedFont {
    mime: &'static str,
    format_hint: &'static str,
    base64_data: String,
}

#[derive(Clone, Debug)]
struct ResolveDebug {
    request: FontQuery,
    selected_path: PathBuf,
}

fn contains_font_face_rule(svg: &str) -> bool {
    svg.to_ascii_lowercase().contains("@font-face")
}

fn font_family_to_string(family: &usvg::FontFamily) -> String {
    match family {
        usvg::FontFamily::Serif => "serif".to_string(),
        usvg::FontFamily::SansSerif => "sans-serif".to_string(),
        usvg::FontFamily::Cursive => "cursive".to_string(),
        usvg::FontFamily::Fantasy => "fantasy".to_string(),
        usvg::FontFamily::Monospace => "monospace".to_string(),
        usvg::FontFamily::Named(name) => name.clone(),
    }
}

fn style_to_css(style: &str) -> &'static str {
    match style {
        "Italic" => "italic",
        "Oblique" => "oblique",
        _ => "normal",
    }
}

fn stretch_to_css(stretch: &str) -> &'static str {
    match stretch {
        "UltraCondensed" => "ultra-condensed",
        "ExtraCondensed" => "extra-condensed",
        "Condensed" => "condensed",
        "SemiCondensed" => "semi-condensed",
        "SemiExpanded" => "semi-expanded",
        "Expanded" => "expanded",
        "ExtraExpanded" => "extra-expanded",
        "UltraExpanded" => "ultra-expanded",
        _ => "normal",
    }
}

fn xml_escape_attr(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn font_spec_to_query(font: &usvg::Font, missing_char: Option<char>) -> FontQuery {
    FontQuery {
        families: font
            .families()
            .iter()
            .map(font_family_to_string)
            .collect::<Vec<_>>(),
        style: format!("{:?}", font.style()),
        weight: font.weight(),
        stretch: format!("{:?}", font.stretch()),
        variations: format!("{:?}", font.variations()),
        missing_char,
    }
}

fn build_fc_match_pattern(query: &FontQuery, family: &str) -> String {
    let mut pattern = format!("{family}:weight={}", query.weight);
    match query.style.as_str() {
        "Italic" => pattern.push_str(":slant=italic"),
        "Oblique" => pattern.push_str(":slant=oblique"),
        _ => {}
    }

    if let Some(c) = query.missing_char {
        pattern.push_str(&format!(":charset={:X}", c as u32));
    }

    pattern
}

fn resolve_with_fc_match_pattern(pattern: &str) -> Option<PathBuf> {
    let output = Command::new("fc-match")
        .args(["-f", "%{file}\\n", pattern])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8(output.stdout).ok()?;
    let first_line = stdout.lines().find(|line| !line.trim().is_empty())?.trim();
    Some(PathBuf::from(first_line))
}

pub fn resolve_font_with_fc_match(query: &FontQuery) -> Result<PathBuf, String> {
    for family in &query.families {
        let pattern = build_fc_match_pattern(query, family);
        if let Some(path) = resolve_with_fc_match_pattern(&pattern) {
            return Ok(path);
        }
    }

    Err(format!(
        "failed to resolve font file for request: {:?}",
        query
    ))
}

fn fallback_query_from_used_fonts(
    missing_char: char,
    used_fonts: &[usvg::fontdb::ID],
    db: &Arc<usvg::fontdb::Database>,
) -> Result<FontQuery, String> {
    let base_font_id = used_fonts
        .first()
        .copied()
        .ok_or_else(|| "usvg did not provide a base font for fallback selection".to_string())?;
    let base_face = db
        .face(base_font_id)
        .ok_or_else(|| format!("base face not found in font database: {base_font_id:?}"))?;

    let mut families = base_face
        .families
        .iter()
        .map(|(name, _)| name.clone())
        .collect::<Vec<_>>();
    families.dedup();
    if families.is_empty() {
        families.push("sans-serif".to_string());
    }

    Ok(FontQuery {
        families,
        style: format!("{:?}", base_face.style),
        weight: base_face.weight.0,
        stretch: format!("{:?}", base_face.stretch),
        variations: "[]".to_string(),
        missing_char: Some(missing_char),
    })
}

fn find_face_id_for_path(db: &usvg::fontdb::Database, path: &Path) -> Option<usvg::fontdb::ID> {
    for face in db.faces() {
        match &face.source {
            usvg::fontdb::Source::File(face_path) if face_path == path => return Some(face.id),
            usvg::fontdb::Source::SharedFile(face_path, _) if face_path == path => {
                return Some(face.id);
            }
            _ => {}
        }
    }

    None
}

fn ensure_font_loaded(
    db: &mut Arc<usvg::fontdb::Database>,
    loaded_paths: &Arc<Mutex<BTreeSet<PathBuf>>>,
    path: &Path,
) -> Result<usvg::fontdb::ID, String> {
    let should_load = {
        let mut loaded = loaded_paths
            .lock()
            .map_err(|_| "failed to access loaded font path set".to_string())?;
        loaded.insert(path.to_path_buf())
    };

    if should_load {
        Arc::make_mut(db).load_font_file(path).map_err(|err| {
            format!(
                "failed to load font '{}' into usvg db: {err}",
                path.display()
            )
        })?;
    }

    find_face_id_for_path(db, path).ok_or_else(|| {
        format!(
            "loaded font '{}' but could not find face id in font database",
            path.display()
        )
    })
}

fn detect_font_type(path: &Path) -> (&'static str, &'static str) {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|s| s.to_ascii_lowercase())
    {
        Some(ext) if ext == "ttf" => ("font/ttf", "truetype"),
        Some(ext) if ext == "otf" => ("font/otf", "opentype"),
        Some(ext) if ext == "woff" => ("font/woff", "woff"),
        Some(ext) if ext == "woff2" => ("font/woff2", "woff2"),
        Some(ext) if ext == "ttc" => ("font/collection", "truetype"),
        Some(ext) if ext == "otc" => ("font/collection", "opentype"),
        _ => ("application/octet-stream", "unknown"),
    }
}

fn build_css(
    request_to_font: &[(FontQuery, PathBuf)],
    embedded: &BTreeMap<PathBuf, EmbeddedFont>,
) -> String {
    let mut css = String::new();
    for (request, path) in request_to_font {
        let family = match request.families.first() {
            Some(f) => f,
            None => continue,
        };
        let font = match embedded.get(path) {
            Some(v) => v,
            None => continue,
        };
        let family_escaped = xml_escape_attr(family);
        css.push_str("@font-face {\n");
        css.push_str(&format!("  font-family: \"{family_escaped}\";\n"));
        css.push_str(&format!(
            "  font-style: {};\n",
            style_to_css(&request.style)
        ));
        css.push_str(&format!("  font-weight: {};\n", request.weight));
        css.push_str(&format!(
            "  font-stretch: {};\n",
            stretch_to_css(&request.stretch)
        ));
        css.push_str(&format!(
            "  src: url(data:{};base64,{}) format(\"{}\");\n",
            font.mime, font.base64_data, font.format_hint
        ));
        css.push_str("}\n");
    }
    css
}

fn find_root_svg_open_tag_end(svg: &str) -> Option<usize> {
    let start = svg.find("<svg")?;
    let bytes = svg.as_bytes();
    let mut i = start;
    let mut quote: Option<u8> = None;
    while i < bytes.len() {
        let b = bytes[i];
        if let Some(q) = quote {
            if b == q {
                quote = None;
            }
        } else if b == b'\'' || b == b'"' {
            quote = Some(b);
        } else if b == b'>' {
            return Some(i + 1);
        }
        i += 1;
    }
    None
}

fn build_debug_comments(debug_entries: &[ResolveDebug]) -> Result<String, String> {
    let mut out = String::new();
    for debug in debug_entries {
        let json = serde_json::json!({
            "request": {
                "families": debug.request.families,
                "style": debug.request.style,
                "weight": debug.request.weight,
                "stretch": debug.request.stretch,
                "variations": debug.request.variations,
                "missing_char": debug.request.missing_char,
            },
            "selected": {
                "result_path": debug.selected_path.display().to_string(),
            }
        });
        let mut serialized = serde_json::to_string(&json)
            .map_err(|e| format!("failed to serialize debug comment: {e}"))?;
        serialized = serialized.replace("--", "- -");
        out.push_str("<!-- font-embed: ");
        out.push_str(&serialized);
        out.push_str(" -->\n");
    }
    Ok(out)
}

fn inject_style_block(svg: &str, debug_comments: &str, css: &str) -> Result<String, String> {
    let insert_at = find_root_svg_open_tag_end(svg)
        .ok_or_else(|| "failed to find root <svg> opening tag".to_string())?;
    let style_block = format!("{debug_comments}<defs><style><![CDATA[\n{css}]]></style></defs>");
    let mut out = String::with_capacity(svg.len() + style_block.len());
    out.push_str(&svg[..insert_at]);
    out.push_str(&style_block);
    out.push_str(&svg[insert_at..]);
    Ok(out)
}

pub fn embed_svg_fonts<F>(input_svg: &str, resolver: F) -> Result<String, String>
where
    F: Fn(&FontQuery) -> Result<PathBuf, String> + Send + Sync + 'static,
{
    if contains_font_face_rule(input_svg) {
        return Err("refusing to process SVG: SVG already contains @font-face rules".to_string());
    }

    let resolver = Arc::new(resolver);
    let queries: Arc<Mutex<Vec<FontQuery>>> = Arc::new(Mutex::new(Vec::new()));
    let queries_for_select_font = Arc::clone(&queries);
    let queries_for_select_fallback = Arc::clone(&queries);
    let resolver_for_select_font = Arc::clone(&resolver);
    let resolver_for_select_fallback = Arc::clone(&resolver);
    let resolved_paths: Arc<Mutex<BTreeMap<FontQuery, PathBuf>>> =
        Arc::new(Mutex::new(BTreeMap::new()));
    let resolved_paths_for_select_font = Arc::clone(&resolved_paths);
    let resolved_paths_for_select_fallback = Arc::clone(&resolved_paths);
    let loaded_paths: Arc<Mutex<BTreeSet<PathBuf>>> = Arc::new(Mutex::new(BTreeSet::new()));
    let loaded_paths_for_select_font = Arc::clone(&loaded_paths);
    let loaded_paths_for_select_fallback = Arc::clone(&loaded_paths);
    let resolver_error: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let resolver_error_for_select_font = Arc::clone(&resolver_error);
    let resolver_error_for_select_fallback = Arc::clone(&resolver_error);

    let mut options = usvg::Options::default();
    options.font_resolver = usvg::FontResolver {
        select_font: Box::new(move |font, db| {
            let query = font_spec_to_query(font, None);

            if let Ok(mut guard) = queries_for_select_font.lock() {
                guard.push(query.clone());
            }

            match resolver_for_select_font(&query) {
                Ok(path) => {
                    if let Ok(mut map) = resolved_paths_for_select_font.lock() {
                        map.insert(query.clone(), path.clone());
                    }

                    match ensure_font_loaded(db, &loaded_paths_for_select_font, &path) {
                        Ok(id) => return Some(id),
                        Err(err) => {
                            if let Ok(mut slot) = resolver_error_for_select_font.lock() {
                                if slot.is_none() {
                                    *slot = Some(err);
                                }
                            }
                        }
                    }
                }
                Err(err) => {
                    if let Ok(mut slot) = resolver_error_for_select_font.lock() {
                        if slot.is_none() {
                            *slot = Some(err);
                        }
                    }
                }
            }

            None
        }),
        select_fallback: Box::new(move |missing_char, used_fonts, db| {
            let query = match fallback_query_from_used_fonts(missing_char, used_fonts, db) {
                Ok(query) => query,
                Err(err) => {
                    if let Ok(mut slot) = resolver_error_for_select_fallback.lock() {
                        if slot.is_none() {
                            *slot = Some(err);
                        }
                    }
                    return None;
                }
            };

            if let Ok(mut guard) = queries_for_select_fallback.lock() {
                guard.push(query.clone());
            }

            match resolver_for_select_fallback(&query) {
                Ok(path) => {
                    if let Ok(mut map) = resolved_paths_for_select_fallback.lock() {
                        map.insert(query.clone(), path.clone());
                    }

                    match ensure_font_loaded(db, &loaded_paths_for_select_fallback, &path) {
                        Ok(id) => Some(id),
                        Err(err) => {
                            if let Ok(mut slot) = resolver_error_for_select_fallback.lock() {
                                if slot.is_none() {
                                    *slot = Some(err);
                                }
                            }
                            None
                        }
                    }
                }
                Err(err) => {
                    if let Ok(mut slot) = resolver_error_for_select_fallback.lock() {
                        if slot.is_none() {
                            *slot = Some(err);
                        }
                    }
                    None
                }
            }
        }),
    };

    usvg::Tree::from_data(input_svg.as_bytes(), &options)
        .map_err(|e| format!("failed to parse SVG: {e}"))?;

    if let Ok(slot) = resolver_error.lock() {
        if let Some(err) = slot.as_ref() {
            return Err(err.clone());
        }
    }

    let mut deduped_queries = {
        let guard = queries
            .lock()
            .map_err(|_| "failed to collect font queries".to_string())?;
        guard
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
    };
    deduped_queries.sort();

    let resolved_paths_snapshot = resolved_paths
        .lock()
        .map_err(|_| "failed to collect resolved font paths".to_string())?
        .clone();

    let mut request_to_font = Vec::<(FontQuery, PathBuf)>::new();
    let mut resolve_debug = Vec::<ResolveDebug>::new();
    let mut needed_paths = BTreeSet::<PathBuf>::new();
    for query in &deduped_queries {
        let chosen = resolved_paths_snapshot
            .get(query)
            .cloned()
            .or_else(|| resolver(query).ok())
            .ok_or_else(|| format!("failed to resolve font file for request: {query:?}"))?;
        if !chosen.exists() {
            return Err(format!(
                "font resolver returned non-existent file for request {query:?}: {}",
                chosen.display()
            ));
        }

        needed_paths.insert(chosen.clone());
        request_to_font.push((query.clone(), chosen));
        resolve_debug.push(ResolveDebug {
            request: query.clone(),
            selected_path: request_to_font
                .last()
                .map(|(_, p)| p.clone())
                .ok_or_else(|| "internal error: missing selected path".to_string())?,
        });
    }

    let mut embedded = BTreeMap::<PathBuf, EmbeddedFont>::new();
    for path in needed_paths {
        let bytes = std::fs::read(&path)
            .map_err(|e| format!("failed to read font file '{}': {e}", path.display()))?;
        let (mime, format_hint) = detect_font_type(&path);
        let base64_data = base64::engine::general_purpose::STANDARD.encode(bytes);
        embedded.insert(
            path.clone(),
            EmbeddedFont {
                mime,
                format_hint,
                base64_data,
            },
        );
    }

    let css = build_css(&request_to_font, &embedded);
    if css.trim().is_empty() {
        return Err("no font requests were found; refusing to emit unchanged SVG".to_string());
    }

    let debug_comments = build_debug_comments(&resolve_debug)?;
    let output_svg = inject_style_block(input_svg, &debug_comments, &css)?;

    Ok(output_svg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn inject_style_block_preserves_xml_declaration_and_text() {
        let input = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\"><text>hello</text></svg>";
        let out = inject_style_block(
            input,
            "<!-- font-embed: {\"debug\":true} -->\n",
            "@font-face { font-family: \"x\"; }",
        )
        .expect("injection should succeed");

        assert!(out.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
        assert!(out.contains("<text>hello</text>"));
        assert!(out.contains("@font-face"));
    }

    #[test]
    fn plantuml_fixture_has_no_stylesheet_class_dependencies() {
        let fixture = include_str!("../examples/plantuml-like.svg");
        assert!(!fixture.contains("<style>"));
        assert!(!fixture.contains("class=\""));
    }

    #[test]
    fn fallback_fixture_triggers_font_requests() {
        let fixture = include_bytes!("../examples/fallback.svg");
        let seen_families: Arc<Mutex<Vec<Vec<String>>>> = Arc::new(Mutex::new(Vec::new()));
        let seen_families_for_resolver = Arc::clone(&seen_families);

        let mut options = usvg::Options::default();
        options.font_resolver = usvg::FontResolver {
            select_font: Box::new(move |font, _db| {
                let families = font
                    .families()
                    .iter()
                    .map(font_family_to_string)
                    .collect::<Vec<_>>();
                seen_families_for_resolver
                    .lock()
                    .expect("mutex poisoned")
                    .push(families);
                None
            }),
            select_fallback: usvg::FontResolver::default_fallback_selector(),
        };

        usvg::Tree::from_data(fixture, &options).expect("fixture should parse");

        let families = seen_families.lock().expect("mutex poisoned");
        assert!(!families.is_empty());
        assert!(families
            .iter()
            .any(|f| f.contains(&"monospace".to_string())));
        assert!(families
            .iter()
            .any(|f| f.contains(&"sans-serif".to_string())));
    }
}
