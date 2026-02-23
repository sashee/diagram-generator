use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use tempfile::TempDir;

use base64::Engine;
use regex::Regex;

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
}

#[derive(Clone, Debug)]
struct ExistingFace {
    family: String,
    style_css: String,
    weight: u16,
    stretch_css: String,
    data_url: String,
    unicode_range: Option<String>,
}

#[derive(Clone, Debug)]
struct ExistingLoadedFace {
    family: String,
    style_css: String,
    weight: u16,
    stretch_css: String,
    runtime_path: PathBuf,
}

fn remove_inliner_debug_comments(svg: &str) -> String {
    let mut out = String::with_capacity(svg.len());
    let mut cursor = 0;
    while let Some(rel_start) = svg[cursor..].find("<!-- font-embed:") {
        let start = cursor + rel_start;
        out.push_str(&svg[cursor..start]);
        let tail = &svg[start..];
        if let Some(rel_end) = tail.find("-->") {
            cursor = start + rel_end + 3;
        } else {
            cursor = svg.len();
            break;
        }
    }
    out.push_str(&svg[cursor..]);
    out
}

fn remove_empty_defs_style_blocks(svg: &str) -> String {
    let re = Regex::new(r"(?is)<defs>\s*<style>\s*<!\[CDATA\[\s*\]\]>\s*</style>\s*</defs>")
        .expect("valid empty defs/style regex");
    re.replace_all(svg, "").to_string()
}

fn trim_leading_inner_whitespace(svg: &str) -> String {
    let Some(open_end) = find_root_svg_open_tag_end(svg) else {
        return svg.to_string();
    };
    let rest = &svg[open_end..];
    let trimmed = rest.trim_start_matches(char::is_whitespace);
    if trimmed.len() == rest.len() {
        return svg.to_string();
    }

    let mut out = String::with_capacity(svg.len());
    out.push_str(&svg[..open_end]);
    out.push_str(trimmed);
    out
}

fn split_css_declarations(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut paren_depth = 0usize;

    for c in body.chars() {
        if let Some(q) = quote {
            current.push(c);
            if c == q {
                quote = None;
            }
            continue;
        }

        match c {
            '\'' | '"' => {
                quote = Some(c);
                current.push(c);
            }
            '(' => {
                paren_depth += 1;
                current.push(c);
            }
            ')' => {
                paren_depth = paren_depth.saturating_sub(1);
                current.push(c);
            }
            ';' if paren_depth == 0 => {
                let trimmed = current.trim();
                if !trimmed.is_empty() {
                    out.push(trimmed.to_string());
                }
                current.clear();
            }
            _ => current.push(c),
        }
    }

    let trimmed = current.trim();
    if !trimmed.is_empty() {
        out.push(trimmed.to_string());
    }

    out
}

fn normalize_css_string_value(value: &str) -> String {
    let v = value.trim();
    let unquoted =
        if (v.starts_with('\'') && v.ends_with('\'')) || (v.starts_with('"') && v.ends_with('"')) {
            &v[1..v.len().saturating_sub(1)]
        } else {
            v
        };
    unquoted.trim().to_string()
}

fn parse_weight(value: &str) -> u16 {
    value.trim().parse::<u16>().ok().unwrap_or(400)
}

fn select_first_data_url(src_value: &str) -> Option<String> {
    let lower = src_value.to_ascii_lowercase();
    let mut cursor = 0usize;
    while let Some(rel_idx) = lower[cursor..].find("url(") {
        let start = cursor + rel_idx + 4;
        let bytes = src_value.as_bytes();
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
            } else if b == b')' {
                let raw = src_value[start..i].trim();
                let unquoted = if (raw.starts_with('"') && raw.ends_with('"'))
                    || (raw.starts_with('\'') && raw.ends_with('\''))
                {
                    &raw[1..raw.len().saturating_sub(1)]
                } else {
                    raw
                };
                if unquoted.to_ascii_lowercase().starts_with("data:") {
                    return Some(unquoted.to_string());
                }
                cursor = i + 1;
                break;
            }
            i += 1;
        }

        if i >= bytes.len() {
            break;
        }
    }

    None
}

fn parse_existing_face(block: &str) -> Result<Option<ExistingFace>, String> {
    let open = match block.find('{') {
        Some(v) => v,
        None => return Ok(None),
    };
    let close = match block.rfind('}') {
        Some(v) if v > open => v,
        _ => return Ok(None),
    };
    let body = &block[open + 1..close];
    let decls = split_css_declarations(body);

    let mut family: Option<String> = None;
    let mut style_css = "normal".to_string();
    let mut weight = 400u16;
    let mut stretch_css = "normal".to_string();
    let mut src: Option<String> = None;
    let mut unicode_range: Option<String> = None;

    for decl in decls {
        let Some((name, value)) = decl.split_once(':') else {
            continue;
        };
        let key = name.trim().to_ascii_lowercase();
        let value = value.trim();
        match key.as_str() {
            "font-family" => family = Some(normalize_css_string_value(value)),
            "font-style" => style_css = value.to_ascii_lowercase(),
            "font-weight" => weight = parse_weight(value),
            "font-stretch" => stretch_css = value.to_ascii_lowercase(),
            "src" => src = Some(value.to_string()),
            "unicode-range" => unicode_range = Some(value.to_string()),
            _ => {}
        }
    }

    let Some(family) = family else {
        return Ok(None);
    };
    let Some(src_value) = src else {
        return Err(format!(
            "existing @font-face for family '{}' has no src declaration",
            family
        ));
    };
    let Some(data_url) = select_first_data_url(&src_value) else {
        return Err(format!(
            "existing @font-face for family '{}' has no data URL src",
            family
        ));
    };

    Ok(Some(ExistingFace {
        family,
        style_css,
        weight,
        stretch_css,
        data_url,
        unicode_range,
    }))
}

fn collect_existing_faces_and_strip_font_face_rules(
    svg: &str,
) -> Result<(String, Vec<ExistingFace>), String> {
    let lower = svg.to_ascii_lowercase();
    let mut scan = 0usize;
    let mut remove_ranges = Vec::<(usize, usize)>::new();
    let mut existing_faces = Vec::<ExistingFace>::new();

    while let Some(rel_idx) = lower[scan..].find("@font-face") {
        let start = scan + rel_idx;
        let Some(open_rel) = svg[start..].find('{') else {
            break;
        };
        let open = start + open_rel;
        let bytes = svg.as_bytes();
        let mut i = open;
        let mut depth = 0usize;
        let mut quote: Option<u8> = None;
        let mut end: Option<usize> = None;
        while i < bytes.len() {
            let b = bytes[i];
            if let Some(q) = quote {
                if b == q {
                    quote = None;
                }
            } else if b == b'\'' || b == b'"' {
                quote = Some(b);
            } else if b == b'{' {
                depth += 1;
            } else if b == b'}' {
                if depth == 0 {
                    break;
                }
                depth -= 1;
                if depth == 0 {
                    end = Some(i + 1);
                    break;
                }
            }
            i += 1;
        }

        let Some(end) = end else {
            return Err("failed to parse existing @font-face block".to_string());
        };
        let block = &svg[start..end];
        if let Some(face) = parse_existing_face(block)? {
            existing_faces.push(face);
        }
        remove_ranges.push((start, end));
        scan = end;
    }

    if remove_ranges.is_empty() {
        return Ok((svg.to_string(), existing_faces));
    }

    let mut cleaned = String::with_capacity(svg.len());
    let mut cursor = 0usize;
    for (start, end) in remove_ranges {
        if start > cursor {
            cleaned.push_str(&svg[cursor..start]);
        }
        cursor = end;
    }
    if cursor < svg.len() {
        cleaned.push_str(&svg[cursor..]);
    }

    Ok((cleaned, existing_faces))
}

fn decode_data_url(data_url: &str) -> Result<(String, Vec<u8>), String> {
    let lower = data_url.to_ascii_lowercase();
    if !lower.starts_with("data:") {
        return Err("expected data URL source".to_string());
    }
    let without_prefix = &data_url["data:".len()..];
    let Some((meta, payload)) = without_prefix.split_once(',') else {
        return Err("invalid data URL: missing ',' separator".to_string());
    };
    if !meta.to_ascii_lowercase().ends_with(";base64") {
        return Err("invalid data URL: expected ';base64' payload".to_string());
    }
    let mime = meta[..meta.len() - ";base64".len()].trim().to_string();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(payload)
        .map_err(|e| format!("invalid base64 payload in data URL: {e}"))?;
    Ok((mime, bytes))
}

fn match_existing_face_path(
    query: &FontQuery,
    existing_faces: &[ExistingLoadedFace],
) -> Option<PathBuf> {
    match_existing_face_paths(query, existing_faces)
        .into_iter()
        .next()
}

fn match_existing_face_paths(
    query: &FontQuery,
    existing_faces: &[ExistingLoadedFace],
) -> Vec<PathBuf> {
    let style_css = style_to_css(&query.style);
    let stretch_css = stretch_to_css(&query.stretch);
    let mut paths = Vec::new();

    for family in &query.families {
        for face in existing_faces.iter().filter(|face| {
            face.family == *family
                && face.style_css == style_css
                && face.weight == query.weight
                && face.stretch_css == stretch_css
        }) {
            paths.push(face.runtime_path.clone());
        }
    }

    paths
}

fn normalize_unicode_range_spec(spec: &str) -> String {
    spec.chars().filter(|c| !c.is_whitespace()).collect()
}

fn subset_font_to_unicode_range(
    input_path: &Path,
    output_path: &Path,
    unicode_range: &str,
) -> Result<(), String> {
    let pyftsubset_bin =
        std::env::var("PYFTSUBSET_BIN").unwrap_or_else(|_| "pyftsubset".to_string());
    let unicode_spec = normalize_unicode_range_spec(unicode_range);
    let output_arg = format!("--output-file={}", output_path.display());
    let unicodes_arg = format!("--unicodes={unicode_spec}");

    let output = Command::new(&pyftsubset_bin)
        .arg(input_path)
        .arg(output_arg)
        .arg(unicodes_arg)
        .output()
        .map_err(|e| format!("failed to execute '{}': {e}", pyftsubset_bin))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if stderr.is_empty() {
            Err(format!(
                "pyftsubset failed for '{}' with status {}",
                input_path.display(),
                output.status
            ))
        } else {
            Err(format!(
                "pyftsubset failed for '{}': {}",
                input_path.display(),
                stderr
            ))
        }
    }
}

fn db_face_supports_char(db: &usvg::fontdb::Database, id: usvg::fontdb::ID, c: char) -> bool {
    db.with_face_data(id, |data, face_index| {
        let Ok(face) = ttf_parser::Face::parse(data, face_index) else {
            return false;
        };
        face.glyph_index(c).is_some()
    })
    .unwrap_or(false)
}

fn file_size(path: &Path) -> u64 {
    std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
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
    let cleaned_input = remove_inliner_debug_comments(input_svg);
    let (stripped_svg, existing_faces) =
        collect_existing_faces_and_strip_font_face_rules(&cleaned_input)?;
    let stripped_svg = remove_empty_defs_style_blocks(&stripped_svg);
    let stripped_svg = trim_leading_inner_whitespace(&stripped_svg);

    let existing_font_temp =
        TempDir::new().map_err(|e| format!("failed to create temp dir: {e}"))?;
    let mut loaded_existing_faces = Vec::<ExistingLoadedFace>::new();
    let mut existing_runtime_paths = BTreeSet::<PathBuf>::new();
    let mut runtime_to_emit = BTreeMap::<PathBuf, PathBuf>::new();
    for (idx, face) in existing_faces.iter().enumerate() {
        let (_mime, bytes) = decode_data_url(&face.data_url)?;
        let emit_path = existing_font_temp
            .path()
            .join(format!("existing-face-{idx}-emit.ttf"));
        std::fs::write(&emit_path, bytes).map_err(|e| {
            format!(
                "failed to materialize existing embedded font for family '{}': {e}",
                face.family
            )
        })?;

        let runtime_path = if let Some(unicode_range) = &face.unicode_range {
            let subset_path = existing_font_temp
                .path()
                .join(format!("existing-face-{idx}-runtime-subset.ttf"));
            subset_font_to_unicode_range(&emit_path, &subset_path, unicode_range)?;
            subset_path
        } else {
            emit_path.clone()
        };

        existing_runtime_paths.insert(runtime_path.clone());
        runtime_to_emit.insert(runtime_path.clone(), emit_path.clone());
        loaded_existing_faces.push(ExistingLoadedFace {
            family: face.family.clone(),
            style_css: face.style_css.clone(),
            weight: face.weight,
            stretch_css: face.stretch_css.clone(),
            runtime_path,
        });
    }
    let loaded_existing_faces = Arc::new(loaded_existing_faces);
    let existing_runtime_paths = Arc::new(existing_runtime_paths);
    let runtime_to_emit = Arc::new(runtime_to_emit);

    let resolver = Arc::new(resolver);
    let queries: Arc<Mutex<Vec<FontQuery>>> = Arc::new(Mutex::new(Vec::new()));
    let queries_for_select_font = Arc::clone(&queries);
    let queries_for_select_fallback = Arc::clone(&queries);
    let resolver_for_select_font = Arc::clone(&resolver);
    let resolver_for_select_fallback = Arc::clone(&resolver);
    let existing_faces_for_select_font = Arc::clone(&loaded_existing_faces);
    let existing_faces_for_select_fallback = Arc::clone(&loaded_existing_faces);
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
    let face_query_context: Arc<Mutex<BTreeMap<usvg::fontdb::ID, FontQuery>>> =
        Arc::new(Mutex::new(BTreeMap::new()));
    let face_query_context_for_select_font = Arc::clone(&face_query_context);
    let face_query_context_for_select_fallback = Arc::clone(&face_query_context);

    let mut options = usvg::Options::default();
    options.font_resolver = usvg::FontResolver {
        select_font: Box::new(move |font, db| {
            let query = font_spec_to_query(font, None);

            if let Ok(mut guard) = queries_for_select_font.lock() {
                guard.push(query.clone());
            }

            if let Some(existing_path) =
                match_existing_face_path(&query, &existing_faces_for_select_font)
            {
                if let Ok(mut map) = resolved_paths_for_select_font.lock() {
                    map.insert(query.clone(), existing_path.clone());
                }

                match ensure_font_loaded(db, &loaded_paths_for_select_font, &existing_path) {
                    Ok(id) => {
                        if let Ok(mut map) = face_query_context_for_select_font.lock() {
                            map.insert(id, query.clone());
                        }
                        return Some(id);
                    }
                    Err(err) => {
                        if let Ok(mut slot) = resolver_error_for_select_font.lock() {
                            if slot.is_none() {
                                *slot = Some(err);
                            }
                        }
                        return None;
                    }
                }
            }

            match resolver_for_select_font(&query) {
                Ok(path) => {
                    if let Ok(mut map) = resolved_paths_for_select_font.lock() {
                        map.insert(query.clone(), path.clone());
                    }

                    match ensure_font_loaded(db, &loaded_paths_for_select_font, &path) {
                        Ok(id) => {
                            if let Ok(mut map) = face_query_context_for_select_font.lock() {
                                map.insert(id, query.clone());
                            }
                            return Some(id);
                        }
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
            let query = match face_query_context_for_select_fallback
                .lock()
                .ok()
                .and_then(|map| used_fonts.first().and_then(|id| map.get(id).cloned()))
            {
                Some(mut base) => {
                    base.missing_char = Some(missing_char);
                    base
                }
                None => match fallback_query_from_used_fonts(missing_char, used_fonts, db) {
                    Ok(query) => query,
                    Err(err) => {
                        if let Ok(mut slot) = resolver_error_for_select_fallback.lock() {
                            if slot.is_none() {
                                *slot = Some(err);
                            }
                        }
                        return None;
                    }
                },
            };

            if let Ok(mut guard) = queries_for_select_fallback.lock() {
                guard.push(query.clone());
            }

            {
                let candidate_paths =
                    match_existing_face_paths(&query, &existing_faces_for_select_fallback);
                for existing_path in candidate_paths {
                    let id = match ensure_font_loaded(
                        db,
                        &loaded_paths_for_select_fallback,
                        &existing_path,
                    ) {
                        Ok(id) => id,
                        Err(err) => {
                            if let Ok(mut slot) = resolver_error_for_select_fallback.lock() {
                                if slot.is_none() {
                                    *slot = Some(err);
                                }
                            }
                            return None;
                        }
                    };

                    if used_fonts.contains(&id) {
                        continue;
                    }

                    if !db_face_supports_char(db, id, missing_char) {
                        continue;
                    }

                    if let Ok(mut map) = resolved_paths_for_select_fallback.lock() {
                        map.insert(query.clone(), existing_path.clone());
                    }
                    if let Ok(mut map) = face_query_context_for_select_fallback.lock() {
                        map.insert(id, query.clone());
                    }
                    return Some(id);
                }
            }

            match resolver_for_select_fallback(&query) {
                Ok(path) => {
                    if let Ok(mut map) = resolved_paths_for_select_fallback.lock() {
                        map.insert(query.clone(), path.clone());
                    }

                    match ensure_font_loaded(db, &loaded_paths_for_select_fallback, &path) {
                        Ok(id) => {
                            if let Ok(mut map) = face_query_context_for_select_fallback.lock() {
                                map.insert(id, query.clone());
                            }
                            Some(id)
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

    usvg::Tree::from_data(stripped_svg.as_bytes(), &options)
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

    let mut request_to_font = Vec::<(FontQuery, PathBuf, bool)>::new();
    let mut needed_paths = BTreeSet::<PathBuf>::new();
    for query in &deduped_queries {
        let chosen_runtime = resolved_paths_snapshot
            .get(query)
            .cloned()
            .ok_or_else(|| format!("failed to resolve font file for request: {query:?}"))?;
        if !chosen_runtime.exists() {
            return Err(format!(
                "font resolver returned non-existent file for request {query:?}: {}",
                chosen_runtime.display()
            ));
        }

        let chosen_emit = runtime_to_emit
            .get(&chosen_runtime)
            .cloned()
            .unwrap_or_else(|| chosen_runtime.clone());
        let from_existing = existing_runtime_paths.contains(&chosen_runtime);

        needed_paths.insert(chosen_emit.clone());
        request_to_font.push((query.clone(), chosen_emit, from_existing));
    }

    let mut grouped =
        BTreeMap::<(String, String, u16, String), Vec<(FontQuery, PathBuf, bool)>>::new();
    for (query, path, from_existing) in request_to_font {
        let key = (
            query.families.first().cloned().unwrap_or_default(),
            query.style.clone(),
            query.weight,
            query.stretch.clone(),
        );
        grouped
            .entry(key)
            .or_default()
            .push((query, path, from_existing));
    }

    let mut merged_request_to_font = Vec::<(FontQuery, PathBuf)>::new();
    for entries in grouped.into_values() {
        let has_existing = entries.iter().any(|(_, _, from_existing)| *from_existing);
        if has_existing && entries.len() > 1 {
            let chosen_path = entries
                .iter()
                .map(|(_, path, _)| path.clone())
                .max_by(|a, b| file_size(a).cmp(&file_size(b)).then_with(|| a.cmp(b)))
                .unwrap_or_else(|| entries[0].1.clone());
            let chosen_query = entries
                .iter()
                .find(|(q, _, _)| q.missing_char.is_none())
                .map(|(q, _, _)| q.clone())
                .unwrap_or_else(|| entries[0].0.clone());
            merged_request_to_font.push((chosen_query, chosen_path));
        } else {
            for (query, path, _) in entries {
                merged_request_to_font.push((query, path));
            }
        }
    }
    merged_request_to_font.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));

    let resolve_debug = merged_request_to_font
        .iter()
        .map(|(request, _)| ResolveDebug {
            request: request.clone(),
        })
        .collect::<Vec<_>>();

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

    let css = build_css(&merged_request_to_font, &embedded);
    if css.trim().is_empty() {
        return Err("no font requests were found; refusing to emit unchanged SVG".to_string());
    }

    let debug_comments = build_debug_comments(&resolve_debug)?;
    let output_svg = inject_style_block(&stripped_svg, &debug_comments, &css)?;

    Ok(output_svg)
}

pub fn ensure_text_fonts_inline(input_svg: &str) -> Result<(), String> {
    embed_svg_fonts(input_svg, |_query| {
        Err("font is not inlined in SVG @font-face data source".to_string())
    })
    .map(|_| ())
}

pub fn parse_svg_tree_inline_fonts_only(input_svg: &str) -> Result<usvg::Tree, String> {
    let cleaned_input = remove_inliner_debug_comments(input_svg);
    let (stripped_svg, existing_faces) =
        collect_existing_faces_and_strip_font_face_rules(&cleaned_input)?;
    let stripped_svg = remove_empty_defs_style_blocks(&stripped_svg);
    let stripped_svg = trim_leading_inner_whitespace(&stripped_svg);

    let existing_font_temp =
        TempDir::new().map_err(|e| format!("failed to create temp dir: {e}"))?;
    let mut loaded_existing_faces = Vec::<ExistingLoadedFace>::new();
    for (idx, face) in existing_faces.iter().enumerate() {
        let (_mime, bytes) = decode_data_url(&face.data_url)?;
        let emit_path = existing_font_temp
            .path()
            .join(format!("existing-face-{idx}-emit.ttf"));
        std::fs::write(&emit_path, bytes).map_err(|e| {
            format!(
                "failed to materialize existing embedded font for family '{}': {e}",
                face.family
            )
        })?;

        let runtime_path = if let Some(unicode_range) = &face.unicode_range {
            let subset_path = existing_font_temp
                .path()
                .join(format!("existing-face-{idx}-runtime-subset.ttf"));
            subset_font_to_unicode_range(&emit_path, &subset_path, unicode_range)?;
            subset_path
        } else {
            emit_path
        };

        loaded_existing_faces.push(ExistingLoadedFace {
            family: face.family.clone(),
            style_css: face.style_css.clone(),
            weight: face.weight,
            stretch_css: face.stretch_css.clone(),
            runtime_path,
        });
    }

    let loaded_existing_faces = Arc::new(loaded_existing_faces);
    let loaded_paths: Arc<Mutex<BTreeSet<PathBuf>>> = Arc::new(Mutex::new(BTreeSet::new()));
    let loaded_paths_for_select_font = Arc::clone(&loaded_paths);
    let loaded_paths_for_select_fallback = Arc::clone(&loaded_paths);
    let existing_faces_for_select_font = Arc::clone(&loaded_existing_faces);
    let existing_faces_for_select_fallback = Arc::clone(&loaded_existing_faces);
    let resolver_error: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let resolver_error_for_select_font = Arc::clone(&resolver_error);
    let resolver_error_for_select_fallback = Arc::clone(&resolver_error);
    let face_query_context: Arc<Mutex<BTreeMap<usvg::fontdb::ID, FontQuery>>> =
        Arc::new(Mutex::new(BTreeMap::new()));
    let face_query_context_for_select_font = Arc::clone(&face_query_context);
    let face_query_context_for_select_fallback = Arc::clone(&face_query_context);

    let mut options = usvg::Options::default();
    options.font_resolver = usvg::FontResolver {
        select_font: Box::new(move |font, db| {
            let query = font_spec_to_query(font, None);

            let Some(existing_path) =
                match_existing_face_path(&query, &existing_faces_for_select_font)
            else {
                if let Ok(mut slot) = resolver_error_for_select_font.lock() {
                    if slot.is_none() {
                        *slot =
                            Some("font is not inlined in SVG @font-face data source".to_string());
                    }
                }
                return None;
            };

            match ensure_font_loaded(db, &loaded_paths_for_select_font, &existing_path) {
                Ok(id) => {
                    if let Ok(mut map) = face_query_context_for_select_font.lock() {
                        map.insert(id, query);
                    }
                    Some(id)
                }
                Err(err) => {
                    if let Ok(mut slot) = resolver_error_for_select_font.lock() {
                        if slot.is_none() {
                            *slot = Some(err);
                        }
                    }
                    None
                }
            }
        }),
        select_fallback: Box::new(move |missing_char, used_fonts, db| {
            let query = match face_query_context_for_select_fallback
                .lock()
                .ok()
                .and_then(|map| used_fonts.first().and_then(|id| map.get(id).cloned()))
            {
                Some(mut base) => {
                    base.missing_char = Some(missing_char);
                    base
                }
                None => match fallback_query_from_used_fonts(missing_char, used_fonts, db) {
                    Ok(query) => query,
                    Err(err) => {
                        if let Ok(mut slot) = resolver_error_for_select_fallback.lock() {
                            if slot.is_none() {
                                *slot = Some(err);
                            }
                        }
                        return None;
                    }
                },
            };

            let candidate_paths =
                match_existing_face_paths(&query, &existing_faces_for_select_fallback);
            for existing_path in candidate_paths {
                let id =
                    match ensure_font_loaded(db, &loaded_paths_for_select_fallback, &existing_path)
                    {
                        Ok(id) => id,
                        Err(err) => {
                            if let Ok(mut slot) = resolver_error_for_select_fallback.lock() {
                                if slot.is_none() {
                                    *slot = Some(err);
                                }
                            }
                            return None;
                        }
                    };

                if used_fonts.contains(&id) {
                    continue;
                }

                if !db_face_supports_char(db, id, missing_char) {
                    continue;
                }

                if let Ok(mut map) = face_query_context_for_select_fallback.lock() {
                    map.insert(id, query.clone());
                }
                return Some(id);
            }

            if let Ok(mut slot) = resolver_error_for_select_fallback.lock() {
                if slot.is_none() {
                    *slot = Some("font is not inlined in SVG @font-face data source".to_string());
                }
            }
            None
        }),
    };

    let tree = usvg::Tree::from_data(stripped_svg.as_bytes(), &options)
        .map_err(|e| format!("failed to parse SVG: {e}"))?;

    if let Ok(slot) = resolver_error.lock() {
        if let Some(err) = slot.as_ref() {
            return Err(err.clone());
        }
    }

    Ok(tree)
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
