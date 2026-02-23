use std::sync::{Arc, Mutex};

fn is_allowed_ref(value: &str) -> bool {
    let trimmed = value.trim();
    trimmed.is_empty()
        || trimmed.starts_with('#')
        || trimmed.to_ascii_lowercase().starts_with("data:")
}

fn validate_css_urls(svg: &str) -> Result<(), String> {
    let lower = svg.to_ascii_lowercase();
    let mut cursor = 0usize;
    while let Some(rel_idx) = lower[cursor..].find("url(") {
        let start = cursor + rel_idx + 4;
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
            } else if b == b')' {
                let raw = svg[start..i].trim();
                let target = if (raw.starts_with('"') && raw.ends_with('"'))
                    || (raw.starts_with('\'') && raw.ends_with('\''))
                {
                    &raw[1..raw.len().saturating_sub(1)]
                } else {
                    raw
                };

                if !is_allowed_ref(target) {
                    return Err(format!("external resource is not allowed: {target}"));
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

    if lower.contains("@import") {
        return Err("external stylesheet imports are not allowed".to_string());
    }

    Ok(())
}

fn validate_xml_refs(svg: &str) -> Result<(), String> {
    if svg.to_ascii_lowercase().contains("<?xml-stylesheet") {
        return Err("xml-stylesheet processing instruction is not allowed".to_string());
    }

    let doc = roxmltree::Document::parse(svg).map_err(|e| format!("failed to parse SVG: {e}"))?;
    for node in doc.descendants().filter(|n| n.is_element()) {
        for attr in node.attributes() {
            let name = attr.name().to_ascii_lowercase();
            if name == "href" || name == "xlink:href" || name == "src" {
                let value = attr.value();
                if !is_allowed_ref(value) {
                    return Err(format!("external resource is not allowed: {value}"));
                }
            }
        }
    }

    Ok(())
}

pub fn render_svg_to_png(svg: &str, zoom: f32) -> Result<Vec<u8>, String> {
    if !zoom.is_finite() || zoom <= 0.0 {
        return Err("--zoom must be a finite number greater than 0".to_string());
    }

    validate_xml_refs(svg)?;
    validate_css_urls(svg)?;

    let external_image_ref: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let external_image_ref_for_resolver = Arc::clone(&external_image_ref);

    let mut options = usvg::Options::default();
    options.image_href_resolver = usvg::ImageHrefResolver {
        resolve_data: usvg::ImageHrefResolver::default_data_resolver(),
        resolve_string: Box::new(move |href, _opts| {
            if let Ok(mut slot) = external_image_ref_for_resolver.lock() {
                *slot = Some(href.to_string());
            }
            None
        }),
    };

    let tree = svg_font_inliner::parse_svg_tree_inline_fonts_only(svg)?;

    if let Ok(slot) = external_image_ref.lock() {
        if let Some(href) = slot.as_ref() {
            return Err(format!("external image resource is not allowed: {href}"));
        }
    }

    let width = ((tree.size().width() * zoom).ceil() as u32).max(1);
    let height = ((tree.size().height() * zoom).ceil() as u32).max(1);
    let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height)
        .ok_or_else(|| "failed to allocate output pixmap".to_string())?;

    let transform = resvg::tiny_skia::Transform::from_scale(zoom, zoom);
    resvg::render(&tree, transform, &mut pixmap.as_mut());

    pixmap
        .encode_png()
        .map_err(|e| format!("failed to encode PNG: {e}"))
}
