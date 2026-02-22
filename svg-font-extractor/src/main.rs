use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;
use std::process::Command;
use std::sync::{Arc, Mutex};

use base64::Engine;

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct FontRequest {
    families: Vec<String>,
    style: String,
    weight: u16,
    stretch: String,
    variations: String,
}

#[derive(Clone, Debug)]
struct EmbeddedFont {
    path: PathBuf,
    mime: &'static str,
    format_hint: &'static str,
    base64_data: String,
}

#[derive(Clone, Debug)]
struct ResolveAttempt {
    family: String,
    pattern: String,
    result_path: Option<PathBuf>,
}

#[derive(Clone, Debug)]
struct ResolveDebug {
    request: FontRequest,
    attempts: Vec<ResolveAttempt>,
    selected_family: String,
    selected_pattern: String,
    selected_path: PathBuf,
}

fn usage(binary_name: &str) {
    eprintln!("Usage: {binary_name} <input-svg> <output-svg>");
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

fn build_fc_match_pattern(request: &FontRequest, family: &str) -> String {
    let mut pattern = format!("{family}:weight={}", request.weight);
    match request.style.as_str() {
        "Italic" => pattern.push_str(":slant=italic"),
        "Oblique" => pattern.push_str(":slant=oblique"),
        _ => {}
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

fn resolve_font_path(request: &FontRequest, family: &str) -> Option<PathBuf> {
    let pattern = build_fc_match_pattern(request, family);
    resolve_with_fc_match_pattern(&pattern)
}

fn ensure_font_loaded(
    db: &mut Arc<usvg::fontdb::Database>,
    loaded_paths: &Arc<Mutex<BTreeSet<PathBuf>>>,
    path: &Path,
) {
    let should_load = {
        let mut loaded = loaded_paths.lock().unwrap_or_else(|_| {
            eprintln!("failed to access loaded font path set");
            process::exit(1);
        });
        loaded.insert(path.to_path_buf())
    };

    if should_load {
        if let Err(err) = Arc::make_mut(db).load_font_file(path) {
            eprintln!(
                "failed to load font '{}' into usvg db: {err}",
                path.display()
            );
            process::exit(1);
        }
    }
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
    request_to_font: &[(FontRequest, PathBuf)],
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

fn build_debug_comments(debug_entries: &[ResolveDebug]) -> String {
    let mut out = String::new();
    for debug in debug_entries {
        let attempts = debug
            .attempts
            .iter()
            .map(|a| {
                serde_json::json!({
                    "family": a.family,
                    "fc_match_pattern": a.pattern,
                    "result_path": a.result_path.as_ref().map(|p| p.display().to_string()),
                })
            })
            .collect::<Vec<_>>();
        let json = serde_json::json!({
            "request": {
                "families": debug.request.families,
                "style": debug.request.style,
                "weight": debug.request.weight,
                "stretch": debug.request.stretch,
                "variations": debug.request.variations,
            },
            "attempts": attempts,
            "selected": {
                "family": debug.selected_family,
                "fc_match_pattern": debug.selected_pattern,
                "result_path": debug.selected_path.display().to_string(),
            }
        });
        let mut serialized = serde_json::to_string(&json).unwrap_or_else(|e| {
            eprintln!("failed to serialize debug comment: {e}");
            process::exit(1);
        });
        serialized = serialized.replace("--", "- -");
        out.push_str("<!-- font-embed: ");
        out.push_str(&serialized);
        out.push_str(" -->\n");
    }
    out
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

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        usage(&args[0]);
        process::exit(2);
    }

    let input_svg_path = &args[1];
    let output_svg_path = &args[2];

    let data = fs::read(input_svg_path).unwrap_or_else(|e| {
        eprintln!("failed to read SVG file '{input_svg_path}': {e}");
        process::exit(1);
    });
    let input_svg = std::str::from_utf8(&data).unwrap_or_else(|e| {
        eprintln!("failed to decode SVG file '{input_svg_path}' as UTF-8: {e}");
        process::exit(1);
    });

    if contains_font_face_rule(input_svg) {
        eprintln!("refusing to process '{input_svg_path}': SVG already contains @font-face rules");
        process::exit(1);
    }

    let requests: Arc<Mutex<Vec<FontRequest>>> = Arc::new(Mutex::new(Vec::new()));
    let requests_for_resolver = Arc::clone(&requests);
    let loaded_paths: Arc<Mutex<BTreeSet<PathBuf>>> = Arc::new(Mutex::new(BTreeSet::new()));
    let loaded_paths_for_resolver = Arc::clone(&loaded_paths);

    let mut options = usvg::Options::default();
    let default_select_font = usvg::FontResolver::default_font_selector();
    let default_select_fallback = usvg::FontResolver::default_fallback_selector();
    options.font_resolver = usvg::FontResolver {
        select_font: Box::new(move |font, db| {
            let request = FontRequest {
                families: font
                    .families()
                    .iter()
                    .map(font_family_to_string)
                    .collect::<Vec<_>>(),
                style: format!("{:?}", font.style()),
                weight: font.weight(),
                stretch: format!("{:?}", font.stretch()),
                variations: format!("{:?}", font.variations()),
            };

            eprintln!(
                "[font-resolver/select_font] families={:?} style={} weight={} stretch={} size=n/a variant_caps=n/a variations={} db_faces={}",
                request.families,
                request.style,
                request.weight,
                request.stretch,
                request.variations,
                db.len()
            );

            if let Ok(mut guard) = requests_for_resolver.lock() {
                guard.push(request);
            }

            for family in font.families().iter().map(font_family_to_string) {
                let req = FontRequest {
                    families: vec![family.clone()],
                    style: format!("{:?}", font.style()),
                    weight: font.weight(),
                    stretch: format!("{:?}", font.stretch()),
                    variations: format!("{:?}", font.variations()),
                };

                if let Some(path) = resolve_font_path(&req, &family) {
                    ensure_font_loaded(db, &loaded_paths_for_resolver, &path);
                    break;
                }
            }

            let selected = default_select_font(font, db);
            eprintln!("[font-resolver/select_font_result] selected={selected:?}");
            selected
        }),
        select_fallback: Box::new(move |ch, used_fonts, db| {
            eprintln!(
                "[font-resolver/select_fallback] char={:?} used_fonts={} db_faces={}",
                ch,
                used_fonts.len(),
                db.len()
            );
            let selected = default_select_fallback(ch, used_fonts, db);
            eprintln!("[font-resolver/select_fallback_result] selected={selected:?}");
            selected
        }),
    };

    usvg::Tree::from_data(&data, &options).unwrap_or_else(|e| {
        eprintln!("failed to parse SVG '{input_svg_path}': {e}");
        process::exit(1);
    });

    let mut deduped_requests = {
        let guard = requests.lock().unwrap_or_else(|_| {
            eprintln!("failed to collect font requests");
            process::exit(1);
        });
        guard
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
    };
    deduped_requests.sort();

    let mut request_to_font = Vec::<(FontRequest, PathBuf)>::new();
    let mut resolve_debug = Vec::<ResolveDebug>::new();
    let mut needed_paths = BTreeSet::<PathBuf>::new();
    for request in &deduped_requests {
        let mut chosen: Option<PathBuf> = None;
        let mut selected_family = String::new();
        let mut selected_pattern = String::new();
        let mut attempts = Vec::<ResolveAttempt>::new();
        for family in &request.families {
            let pattern = build_fc_match_pattern(request, family);
            let maybe_path = resolve_with_fc_match_pattern(&pattern);
            attempts.push(ResolveAttempt {
                family: family.clone(),
                pattern: pattern.clone(),
                result_path: maybe_path.clone(),
            });
            if let Some(path) = maybe_path {
                chosen = Some(path);
                selected_family = family.clone();
                selected_pattern = pattern;
                break;
            }
        }

        let chosen = chosen.unwrap_or_else(|| {
            eprintln!("failed to resolve font file for request: {:?}", request);
            process::exit(1);
        });
        if !chosen.exists() {
            eprintln!(
                "font resolver returned non-existent file for request {:?}: {}",
                request,
                chosen.display()
            );
            process::exit(1);
        }

        eprintln!(
            "[font-resolver/resolved] request={:?} path={}",
            request,
            chosen.display()
        );
        needed_paths.insert(chosen.clone());
        request_to_font.push((request.clone(), chosen));
        resolve_debug.push(ResolveDebug {
            request: request.clone(),
            attempts,
            selected_family,
            selected_pattern,
            selected_path: request_to_font
                .last()
                .map(|(_, p)| p.clone())
                .unwrap_or_else(|| {
                    eprintln!("internal error: missing selected path");
                    process::exit(1);
                }),
        });
    }

    let mut embedded = BTreeMap::<PathBuf, EmbeddedFont>::new();
    for path in needed_paths {
        let bytes = fs::read(&path).unwrap_or_else(|e| {
            eprintln!("failed to read font file '{}': {e}", path.display());
            process::exit(1);
        });
        let (mime, format_hint) = detect_font_type(&path);
        let base64_data = base64::engine::general_purpose::STANDARD.encode(bytes);
        embedded.insert(
            path.clone(),
            EmbeddedFont {
                path,
                mime,
                format_hint,
                base64_data,
            },
        );
    }

    let css = build_css(&request_to_font, &embedded);
    if css.trim().is_empty() {
        eprintln!("no font requests were found; refusing to emit unchanged SVG");
        process::exit(1);
    }

    let debug_comments = build_debug_comments(&resolve_debug);

    let output_svg = inject_style_block(input_svg, &debug_comments, &css).unwrap_or_else(|e| {
        eprintln!("failed to inject embedded font style block: {e}");
        process::exit(1);
    });

    fs::write(output_svg_path, output_svg).unwrap_or_else(|e| {
        eprintln!("failed to write output SVG '{}': {e}", output_svg_path);
        process::exit(1);
    });

    for item in embedded.values() {
        eprintln!("[font-resolver/embedded] {}", item.path.display());
    }
    eprintln!("wrote SVG with embedded fonts to '{}'", output_svg_path);
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
