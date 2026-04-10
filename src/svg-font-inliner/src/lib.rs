use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use tempfile::TempDir;

use base64::Engine;
use lightningcss::printer::PrinterOptions;
use lightningcss::properties::font::{AbsoluteFontWeight, FontStretch, FontWeight};
use lightningcss::rules::font_face::{FontFaceProperty, FontFaceRule, FontStyle, Source};
use lightningcss::rules::{CssRule, CssRuleList};
use lightningcss::stylesheet::{ParserOptions, StyleSheet};
use lightningcss::traits::ToCss;
use lightningcss::values::angle::Angle;
use lightningcss::values::size::Size2D;

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
    style: StyleRange,
    weight: WeightRange,
    stretch: StretchRange,
    data_url: String,
    unicode_range: Option<String>,
}

#[derive(Clone, Copy, Debug)]
enum StyleRange {
    Normal,
    Italic,
    ObliqueRange { min_deg: f32, max_deg: f32 },
}

impl StyleRange {
    fn matches(self, query: QueryStyle) -> bool {
        match (self, query) {
            (StyleRange::Normal, QueryStyle::Normal) => true,
            (StyleRange::Italic, QueryStyle::Italic) => true,
            (StyleRange::ObliqueRange { min_deg, max_deg }, QueryStyle::Oblique { deg }) => {
                min_deg <= deg && deg <= max_deg
            }
            _ => false,
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum QueryStyle {
    Normal,
    Italic,
    Oblique { deg: f32 },
}

#[derive(Clone, Copy, Debug)]
struct WeightRange {
    min: u16,
    max: u16,
}

impl WeightRange {
    fn exact(value: u16) -> Self {
        Self {
            min: value,
            max: value,
        }
    }

    fn includes(self, value: u16) -> bool {
        self.min <= value && value <= self.max
    }
}

#[derive(Clone, Debug)]
struct ExistingLoadedFace {
    family: String,
    style: StyleRange,
    weight: WeightRange,
    stretch: StretchRange,
    runtime_path: PathBuf,
}

#[derive(Clone, Copy, Debug)]
struct StretchRange {
    min: f32,
    max: f32,
}

impl StretchRange {
    fn exact(value: f32) -> Self {
        Self {
            min: value,
            max: value,
        }
    }

    fn includes(self, value: f32) -> bool {
        self.min <= value && value <= self.max
    }
}

#[cfg(test)]
fn remove_inliner_debug_comments(svg: &str) -> String {
    let Ok(doc) = roxmltree::Document::parse(svg) else {
        return svg.to_string();
    };

    let mut remove_ranges = Vec::<(usize, usize)>::new();
    for node in doc
        .descendants()
        .filter(|n| n.node_type() == roxmltree::NodeType::Comment)
    {
        let text = node.text().unwrap_or("").trim_start();
        if text.starts_with("svg-font-inliner:") {
            let range = node.range();
            remove_ranges.push((range.start, range.end));
        }
    }

    if remove_ranges.is_empty() {
        return svg.to_string();
    }

    remove_ranges.sort_by_key(|(start, _)| *start);
    let mut out = String::with_capacity(svg.len());
    let mut cursor = 0usize;
    for (start, end) in remove_ranges {
        if start > cursor {
            out.push_str(&svg[cursor..start]);
        }
        cursor = end;
    }
    if cursor < svg.len() {
        out.push_str(&svg[cursor..]);
    }

    out
}

fn find_tag_end_outside_quotes(fragment: &str) -> Option<usize> {
    let bytes = fragment.as_bytes();
    let mut quote: Option<u8> = None;
    for (idx, b) in bytes.iter().copied().enumerate() {
        if let Some(q) = quote {
            if b == q {
                quote = None;
            }
            continue;
        }
        match b {
            b'\'' | b'"' => quote = Some(b),
            b'>' => return Some(idx),
            _ => {}
        }
    }
    None
}

fn is_svg_local_name(name: &[u8]) -> bool {
    let local = match name.rsplit(|b| *b == b':').next() {
        Some(value) => value,
        None => name,
    };
    local.eq_ignore_ascii_case(b"svg")
}

fn find_doctype_range_before_root_svg(svg: &str) -> Result<Option<(usize, usize)>, String> {
    let mut reader = quick_xml::Reader::from_str(svg);
    let mut doctype_range: Option<(usize, usize)> = None;

    loop {
        let start = usize::try_from(reader.buffer_position())
            .map_err(|_| "failed to scan SVG prolog: parser position overflow".to_string())?;
        match reader.read_event() {
            Ok(quick_xml::events::Event::DocType(_)) => {
                let end = usize::try_from(reader.buffer_position()).map_err(|_| {
                    "failed to scan SVG prolog: parser position overflow".to_string()
                })?;
                doctype_range = Some((start, end));
            }
            Ok(quick_xml::events::Event::Start(tag)) | Ok(quick_xml::events::Event::Empty(tag)) => {
                if is_svg_local_name(tag.name().as_ref()) {
                    return Ok(doctype_range);
                }
                return Ok(None);
            }
            Ok(quick_xml::events::Event::Eof) => return Ok(doctype_range),
            Ok(_) => {}
            Err(err) => return Err(format!("failed to scan SVG prolog: {err}")),
        }
    }
}

fn scrub_range_preserve_offsets(svg: &str, start: usize, end: usize) -> String {
    let mut out = String::with_capacity(svg.len());
    out.push_str(&svg[..start]);
    for b in svg[start..end].bytes() {
        match b {
            b'\n' => out.push('\n'),
            b'\r' => out.push('\r'),
            b'\t' => out.push('\t'),
            _ => out.push(' '),
        }
    }
    out.push_str(&svg[end..]);
    out
}

fn normalize_svg_for_strict_parse(svg: &str) -> Result<Cow<'_, str>, String> {
    let Some((start, end)) = find_doctype_range_before_root_svg(svg)? else {
        return Ok(Cow::Borrowed(svg));
    };

    if start >= end || end > svg.len() {
        return Ok(Cow::Borrowed(svg));
    }

    Ok(Cow::Owned(scrub_range_preserve_offsets(svg, start, end)))
}

fn find_root_svg_open_tag_end(svg: &str) -> Option<usize> {
    let mut cursor = 0usize;
    while let Some(rel_start) = svg[cursor..].find('<') {
        let start = cursor + rel_start;
        let fragment = &svg[start..];
        if fragment.starts_with("<!--") {
            let rel_end = fragment.find("-->")?;
            cursor = start + rel_end + 3;
            continue;
        }
        if fragment.starts_with("<?") {
            let rel_end = fragment.find("?>")?;
            cursor = start + rel_end + 2;
            continue;
        }
        if fragment.starts_with("<!") {
            let rel_end = find_tag_end_outside_quotes(fragment)?;
            cursor = start + rel_end + 1;
            continue;
        }
        if !fragment[1..].to_ascii_lowercase().starts_with("svg") {
            cursor = start + 1;
            continue;
        }

        let next = fragment.as_bytes().get(4).copied();
        if next.is_some_and(|b| !(b.is_ascii_whitespace() || b == b'>' || b == b'/')) {
            cursor = start + 1;
            continue;
        }

        let open_rel = find_tag_end_outside_quotes(fragment)?;
        return Some(start + open_rel + 1);
    }

    None
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
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

fn parse_absolute_weight(value: &AbsoluteFontWeight) -> u16 {
    match value {
        AbsoluteFontWeight::Weight(weight) => *weight as u16,
        AbsoluteFontWeight::Normal => 400,
        AbsoluteFontWeight::Bold => 700,
    }
}

fn parse_weight(value: &Size2D<FontWeight>) -> Result<WeightRange, String> {
    if matches!(value.0, FontWeight::Bolder | FontWeight::Lighter)
        || matches!(value.1, FontWeight::Bolder | FontWeight::Lighter)
    {
        return Err(
            "existing @font-face has relative font-weight (bolder/lighter), which is not supported"
                .to_string(),
        );
    }

    let FontWeight::Absolute(first) = &value.0 else {
        return Err(
            "existing @font-face has relative font-weight (bolder/lighter), which is not supported"
                .to_string(),
        );
    };

    let FontWeight::Absolute(second) = &value.1 else {
        return Err(
            "existing @font-face has relative font-weight (bolder/lighter), which is not supported"
                .to_string(),
        );
    };

    let min = parse_absolute_weight(first);
    let max = parse_absolute_weight(second);
    if min > max {
        return Err(format!(
            "existing @font-face has descending font-weight range ({min} {max}), which is invalid"
        ));
    }

    Ok(WeightRange { min, max })
}

fn parse_stretch_token(value: &str) -> Option<f32> {
    let token = value.trim().to_ascii_lowercase();
    match token.as_str() {
        "ultra-condensed" => Some(50.0),
        "extra-condensed" => Some(62.5),
        "condensed" => Some(75.0),
        "semi-condensed" => Some(87.5),
        "normal" => Some(100.0),
        "semi-expanded" => Some(112.5),
        "expanded" => Some(125.0),
        "extra-expanded" => Some(150.0),
        "ultra-expanded" => Some(200.0),
        _ => token.strip_suffix('%').and_then(|n| n.parse::<f32>().ok()),
    }
}

fn parse_stretch_component(value: &FontStretch) -> Result<f32, String> {
    let css = css_to_string(value)?;
    parse_stretch_token(&css).ok_or_else(|| {
        format!(
            "existing @font-face has unsupported font-stretch value '{}': expected keyword or percentage",
            css
        )
    })
}

fn parse_stretch(value: &Size2D<FontStretch>) -> Result<StretchRange, String> {
    let min = parse_stretch_component(&value.0)?;
    let max = parse_stretch_component(&value.1)?;
    if min > max {
        return Err(format!(
            "existing @font-face has descending font-stretch range ({min}% {max}%), which is invalid"
        ));
    }
    Ok(StretchRange { min, max })
}

fn parse_angle_token_to_degrees(value: &str) -> Option<f32> {
    let token = value.trim().to_ascii_lowercase();
    if let Some(raw) = token.strip_suffix("deg") {
        return raw.trim().parse::<f32>().ok();
    }
    if let Some(raw) = token.strip_suffix("grad") {
        return raw.trim().parse::<f32>().ok().map(|v| v * 0.9);
    }
    if let Some(raw) = token.strip_suffix("rad") {
        return raw
            .trim()
            .parse::<f32>()
            .ok()
            .map(|v| v * (180.0 / std::f32::consts::PI));
    }
    if let Some(raw) = token.strip_suffix("turn") {
        return raw.trim().parse::<f32>().ok().map(|v| v * 360.0);
    }
    None
}

fn parse_angle_to_degrees(value: &Angle) -> f32 {
    value.to_degrees()
}

fn parse_style(value: &FontStyle) -> Result<StyleRange, String> {
    match value {
        FontStyle::Normal => Ok(StyleRange::Normal),
        FontStyle::Italic => Ok(StyleRange::Italic),
        FontStyle::Oblique(range) => {
            let min_deg = parse_angle_to_degrees(&range.0);
            let max_deg = parse_angle_to_degrees(&range.1);
            if min_deg > max_deg {
                return Err(format!(
                    "existing @font-face has descending font-style oblique range ({min_deg}deg {max_deg}deg), which is invalid"
                ));
            }
            Ok(StyleRange::ObliqueRange { min_deg, max_deg })
        }
    }
}

fn parse_absolute_weight_token(value: &str) -> Option<u16> {
    let token = value.trim();
    if token.eq_ignore_ascii_case("normal") {
        return Some(400);
    }
    if token.eq_ignore_ascii_case("bold") {
        return Some(700);
    }
    token
        .parse::<u16>()
        .ok()
        .filter(|parsed| (1..=1000).contains(parsed))
}

fn validate_font_weight_declaration_value(value: &str) -> Result<(), String> {
    let tokens = value
        .split_whitespace()
        .map(|token| token.trim_end_matches("!important"))
        .filter(|token| !token.is_empty() && !token.eq_ignore_ascii_case("!important"))
        .collect::<Vec<_>>();

    if tokens.len() != 2 {
        return Ok(());
    }

    let Some(min) = parse_absolute_weight_token(tokens[0]) else {
        return Ok(());
    };
    let Some(max) = parse_absolute_weight_token(tokens[1]) else {
        return Ok(());
    };

    if min > max {
        return Err(format!(
            "existing @font-face has descending font-weight range ({min} {max}), which is invalid"
        ));
    }

    Ok(())
}

fn validate_font_stretch_declaration_value(value: &str) -> Result<(), String> {
    let tokens = value
        .split_whitespace()
        .map(|token| token.trim_end_matches("!important"))
        .filter(|token| !token.is_empty() && !token.eq_ignore_ascii_case("!important"))
        .collect::<Vec<_>>();

    if tokens.len() != 2 {
        return Ok(());
    }

    let Some(min) = parse_stretch_token(tokens[0]) else {
        return Ok(());
    };
    let Some(max) = parse_stretch_token(tokens[1]) else {
        return Ok(());
    };

    if min > max {
        return Err(format!(
            "existing @font-face has descending font-stretch range ({min}% {max}%), which is invalid"
        ));
    }

    Ok(())
}

fn validate_font_style_declaration_value(value: &str) -> Result<(), String> {
    let tokens = value
        .split_whitespace()
        .map(|token| token.trim_end_matches("!important"))
        .filter(|token| !token.is_empty() && !token.eq_ignore_ascii_case("!important"))
        .collect::<Vec<_>>();

    if tokens.len() < 3 {
        return Ok(());
    }
    if !tokens[0].eq_ignore_ascii_case("oblique") {
        return Ok(());
    }

    let Some(min) = parse_angle_token_to_degrees(tokens[1]) else {
        return Ok(());
    };
    let Some(max) = parse_angle_token_to_degrees(tokens[2]) else {
        return Ok(());
    };

    if min > max {
        return Err(format!(
            "existing @font-face has descending font-style oblique range ({min}deg {max}deg), which is invalid"
        ));
    }

    Ok(())
}

fn find_declaration_terminator(fragment: &str) -> usize {
    let mut quote: Option<u8> = None;
    let mut paren_depth: usize = 0;
    for (idx, b) in fragment.as_bytes().iter().copied().enumerate() {
        if let Some(q) = quote {
            if b == q {
                quote = None;
            }
            continue;
        }
        match b {
            b'\'' | b'"' => quote = Some(b),
            b'(' => paren_depth += 1,
            b')' if paren_depth > 0 => paren_depth -= 1,
            b';' if paren_depth == 0 => return idx,
            _ => {}
        }
    }
    fragment.len()
}

fn find_block_end(fragment: &str) -> Option<usize> {
    let mut quote: Option<u8> = None;
    let mut paren_depth: usize = 0;
    let mut brace_depth: usize = 0;

    for (idx, b) in fragment.as_bytes().iter().copied().enumerate() {
        if let Some(q) = quote {
            if b == q {
                quote = None;
            }
            continue;
        }

        match b {
            b'\'' | b'"' => quote = Some(b),
            b'(' => paren_depth += 1,
            b')' if paren_depth > 0 => paren_depth -= 1,
            b'{' if paren_depth == 0 => brace_depth += 1,
            b'}' if paren_depth == 0 && brace_depth > 0 => {
                brace_depth -= 1;
                if brace_depth == 0 {
                    return Some(idx);
                }
            }
            _ => {}
        }
    }

    None
}

fn validate_font_face_block_weight_ranges(block: &str) -> Result<(), String> {
    let mut cursor = 0usize;
    while cursor < block.len() {
        let decl = &block[cursor..];
        let end = find_declaration_terminator(decl);
        let declaration = decl[..end].trim();

        if let Some((name, value)) = declaration.split_once(':') {
            if name.trim().eq_ignore_ascii_case("font-weight") {
                validate_font_weight_declaration_value(value.trim())?;
            }
            if name.trim().eq_ignore_ascii_case("font-stretch") {
                validate_font_stretch_declaration_value(value.trim())?;
            }
            if name.trim().eq_ignore_ascii_case("font-style") {
                validate_font_style_declaration_value(value.trim())?;
            }
        }

        if end == decl.len() {
            break;
        }
        cursor += end + 1;
    }
    Ok(())
}

fn validate_font_face_weight_ranges(css_input: &str) -> Result<(), String> {
    let lowered = css_input.to_ascii_lowercase();
    let mut cursor = 0usize;

    while let Some(rel_at) = lowered[cursor..].find("@font-face") {
        let at = cursor + rel_at;
        let Some(rel_open) = lowered[at..].find('{') else {
            break;
        };
        let open = at + rel_open;
        let fragment = &css_input[open..];
        let close = find_block_end(fragment).ok_or_else(|| {
            "unterminated @font-face block while validating font-weight".to_string()
        })?;

        let block = &css_input[open + 1..open + close];
        validate_font_face_block_weight_ranges(block)?;
        cursor = open + close + 1;
    }

    Ok(())
}

fn css_to_string<T: ToCss>(value: &T) -> Result<String, String> {
    value
        .to_css_string(PrinterOptions::default())
        .map_err(|e| format!("failed to serialize CSS value: {e}"))
}

fn parse_existing_face_rule(rule: &FontFaceRule<'_>) -> Result<Option<ExistingFace>, String> {
    let mut family: Option<String> = None;
    let mut style = StyleRange::Normal;
    let mut weight = WeightRange::exact(400);
    let mut stretch = StretchRange::exact(100.0);
    let mut data_url: Option<String> = None;
    let mut unicode_range: Option<String> = None;

    for prop in &rule.properties {
        match prop {
            FontFaceProperty::Source(sources) => {
                for src in sources {
                    let Source::Url(url_source) = src else {
                        continue;
                    };
                    let candidate = url_source.url.url.as_ref();
                    if candidate.to_ascii_lowercase().starts_with("data:") {
                        data_url = Some(candidate.to_string());
                        break;
                    }
                }
            }
            FontFaceProperty::FontFamily(font_family) => {
                let value = css_to_string(font_family)?;
                family = Some(normalize_css_string_value(&value));
            }
            FontFaceProperty::FontStyle(font_style) => {
                style = parse_style(font_style)?;
            }
            FontFaceProperty::FontWeight(font_weight) => {
                weight = parse_weight(font_weight)?;
            }
            FontFaceProperty::FontStretch(font_stretch) => {
                stretch = parse_stretch(font_stretch)?;
            }
            FontFaceProperty::UnicodeRange(ranges) => {
                unicode_range = Some(css_to_string(ranges)?);
            }
            FontFaceProperty::Custom(_) => {}
        }
    }

    let Some(family) = family else {
        return Ok(None);
    };
    let Some(data_url) = data_url else {
        return Err(format!(
            "existing @font-face for family '{}' has no data URL src",
            family
        ));
    };

    Ok(Some(ExistingFace {
        family,
        style,
        weight,
        stretch,
        data_url,
        unicode_range,
    }))
}

fn collect_existing_faces_from_style_css(
    style_css: &str,
) -> Result<(String, Vec<ExistingFace>, bool), String> {
    let trimmed = style_css.trim();
    let (css_input, had_cdata, prefix, suffix) =
        if let Some(without_open) = trimmed.strip_prefix("<![CDATA[") {
            if let Some(inner) = without_open.strip_suffix("]]>") {
                let start = style_css.find(trimmed).unwrap_or(0);
                let end = start + trimmed.len();
                (
                    inner,
                    true,
                    style_css[..start].to_string(),
                    style_css[end..].to_string(),
                )
            } else {
                (style_css, false, String::new(), String::new())
            }
        } else {
            (style_css, false, String::new(), String::new())
        };

    validate_font_face_weight_ranges(css_input)?;

    let mut stylesheet = StyleSheet::parse(css_input, ParserOptions::default())
        .map_err(|e| format!("failed to parse <style> CSS: {e}"))?;

    let mut existing_faces = Vec::<ExistingFace>::new();
    let mut kept_rules = Vec::new();
    for rule in stylesheet.rules.0.drain(..) {
        match rule {
            CssRule::FontFace(face_rule) => {
                if let Some(face) = parse_existing_face_rule(&face_rule)? {
                    existing_faces.push(face);
                }
            }
            other => kept_rules.push(other),
        }
    }
    let has_non_font_rules = !kept_rules.is_empty();
    stylesheet.rules = CssRuleList(kept_rules);

    let cleaned_css = stylesheet
        .to_css(PrinterOptions::default())
        .map_err(|e| format!("failed to serialize <style> CSS: {e}"))?
        .code;

    let cleaned_css = if had_cdata {
        format!("{prefix}<![CDATA[{cleaned_css}]]>{suffix}")
    } else {
        cleaned_css
    };

    Ok((cleaned_css, existing_faces, has_non_font_rules))
}

fn collect_existing_faces_and_strip_font_face_rules(
    svg: &str,
) -> Result<(String, Vec<ExistingFace>, usize), String> {
    let normalized_svg: Option<String>;
    let doc = match roxmltree::Document::parse(svg) {
        Ok(doc) => doc,
        Err(original_err) => {
            let quote_normalized = normalize_inner_attribute_quotes_preserve_offsets(svg);
            if roxmltree::Document::parse(&quote_normalized).is_ok() {
                normalized_svg = Some(quote_normalized);
                roxmltree::Document::parse(
                    normalized_svg
                        .as_deref()
                        .expect("quote-normalized SVG should be present"),
                )
                .map_err(|_| format!("failed to parse SVG: {original_err}"))?
            } else {
                let normalized = normalize_svg_for_strict_parse(svg)?;
                let Cow::Owned(owned) = normalized else {
                    let Some(root_open_end) = find_root_svg_open_tag_end(svg) else {
                        return Err(format!("failed to parse SVG: {original_err}"));
                    };
                    return Ok((svg.to_string(), Vec::new(), root_open_end));
                };
                normalized_svg = Some(owned);
                match roxmltree::Document::parse(
                    normalized_svg
                        .as_deref()
                        .expect("normalized SVG should be present"),
                ) {
                    Ok(doc) => doc,
                    Err(_) => {
                        let Some(root_open_end) = find_root_svg_open_tag_end(svg) else {
                            return Err(format!("failed to parse SVG: {original_err}"));
                        };
                        return Ok((svg.to_string(), Vec::new(), root_open_end));
                    }
                }
            }
        }
    };

    let root = doc.root_element();
    if !root.is_element() || root.tag_name().name() != "svg" {
        return Err("failed to parse SVG: missing root <svg> element".to_string());
    }
    let root_range = root.range();
    let root_fragment = &svg[root_range.start..root_range.end];
    let root_open_rel = find_tag_end_outside_quotes(root_fragment)
        .ok_or_else(|| "failed to locate root <svg> opening tag end".to_string())?;
    let root_open_end = root_range.start + root_open_rel + 1;

    #[derive(Clone)]
    struct Replacement {
        start: usize,
        end: usize,
        replacement: String,
        is_removal: bool,
    }

    let mut replacements = Vec::<Replacement>::new();
    let mut existing_faces = Vec::<ExistingFace>::new();

    for node in doc
        .descendants()
        .filter(|n| n.node_type() == roxmltree::NodeType::Comment)
    {
        let text = node.text().unwrap_or("").trim_start();
        if text.starts_with("svg-font-inliner:") {
            let range = node.range();
            replacements.push(Replacement {
                start: range.start,
                end: range.end,
                replacement: String::new(),
                is_removal: true,
            });
        }
    }

    for node in doc
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "style")
    {
        let range = node.range();
        let style_fragment = &svg[range.start..range.end];

        let Some(open_tag_end) = find_tag_end_outside_quotes(style_fragment) else {
            continue;
        };
        let fallback_css_start = range.start + open_tag_end + 1;
        let css_start = node
            .first_child()
            .map(|n| n.range().start)
            .unwrap_or(fallback_css_start);
        let css_end = node
            .last_child()
            .map(|n| n.range().end)
            .unwrap_or(css_start);
        if css_end < css_start {
            continue;
        }

        let style_css = &svg[css_start..css_end];
        let (cleaned_css, mut parsed_faces, has_non_font_rules) =
            collect_existing_faces_from_style_css(style_css)?;
        existing_faces.append(&mut parsed_faces);

        if has_non_font_rules && cleaned_css == style_css {
            continue;
        }

        if !has_non_font_rules {
            replacements.push(Replacement {
                start: range.start,
                end: range.end,
                replacement: String::new(),
                is_removal: true,
            });
            continue;
        }

        let inner_start = css_start.saturating_sub(range.start);
        let inner_end = css_end.saturating_sub(range.start);
        let mut replacement = String::with_capacity(style_fragment.len() + cleaned_css.len());
        replacement.push_str(&style_fragment[..inner_start]);
        replacement.push_str(&cleaned_css);
        replacement.push_str(&style_fragment[inner_end..]);
        replacements.push(Replacement {
            start: range.start,
            end: range.end,
            replacement,
            is_removal: false,
        });
    }

    for node in doc
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "defs")
    {
        let mut meaningful_child_found = false;
        for child in node.children() {
            if child.is_text() {
                if !child.text().unwrap_or("").trim().is_empty() {
                    meaningful_child_found = true;
                    break;
                }
                continue;
            }

            let child_range = child.range();
            let child_removed = replacements
                .iter()
                .any(|r| r.is_removal && r.start <= child_range.start && r.end >= child_range.end);
            if !child_removed {
                meaningful_child_found = true;
                break;
            }
        }

        if !meaningful_child_found {
            let range = node.range();
            replacements.push(Replacement {
                start: range.start,
                end: range.end,
                replacement: String::new(),
                is_removal: true,
            });
        }
    }

    if replacements.is_empty() {
        let rest = &svg[root_open_end..];
        let trimmed = rest.trim_start_matches(char::is_whitespace);
        if trimmed.len() == rest.len() {
            return Ok((svg.to_string(), existing_faces, root_open_end));
        }
        let mut out = String::with_capacity(svg.len());
        out.push_str(&svg[..root_open_end]);
        out.push_str(trimmed);
        return Ok((out, existing_faces, root_open_end));
    }

    replacements.sort_by(|a, b| a.start.cmp(&b.start).then_with(|| b.end.cmp(&a.end)));

    let mut final_replacements = Vec::<Replacement>::new();
    for replacement in replacements {
        let shadowed_by_removal = final_replacements.iter().any(|existing| {
            existing.is_removal
                && existing.start <= replacement.start
                && existing.end >= replacement.end
        });
        if !shadowed_by_removal {
            final_replacements.push(replacement);
        }
    }

    let mut adjusted_root_open_end = root_open_end;
    for replacement in &final_replacements {
        if replacement.end <= root_open_end {
            let old_len = replacement.end - replacement.start;
            let new_len = replacement.replacement.len();
            if new_len >= old_len {
                adjusted_root_open_end += new_len - old_len;
            } else {
                adjusted_root_open_end -= old_len - new_len;
            }
        }
    }

    let mut cleaned = String::with_capacity(svg.len());
    let mut cursor = 0usize;
    for replacement in final_replacements {
        if replacement.start > cursor {
            cleaned.push_str(&svg[cursor..replacement.start]);
        }
        cleaned.push_str(&replacement.replacement);
        cursor = replacement.end;
    }
    if cursor < svg.len() {
        cleaned.push_str(&svg[cursor..]);
    }

    let rest = &cleaned[adjusted_root_open_end..];
    let trimmed = rest.trim_start_matches(char::is_whitespace);
    if trimmed.len() == rest.len() {
        return Ok((cleaned, existing_faces, adjusted_root_open_end));
    }

    let mut out = String::with_capacity(cleaned.len());
    out.push_str(&cleaned[..adjusted_root_open_end]);
    out.push_str(trimmed);
    Ok((out, existing_faces, adjusted_root_open_end))
}

fn has_base64_header(data_url: &str) -> bool {
    let trimmed = data_url.trim();
    let Some((prefix, _)) = trimmed.split_once(',') else {
        return false;
    };
    prefix.to_ascii_lowercase().trim_end().ends_with(";base64")
}

fn decode_data_url(data_url: &str) -> Result<(String, Vec<u8>), String> {
    let parsed =
        data_url::DataUrl::process(data_url).map_err(|_| "expected data URL source".to_string())?;

    if !has_base64_header(data_url) {
        return Err("invalid data URL: expected ';base64' payload".to_string());
    }

    let mime = parsed.mime_type();
    let mime_value = format!("{}/{}", mime.type_, mime.subtype);
    let (bytes, _) = parsed
        .decode_to_vec()
        .map_err(|e| format!("invalid base64 payload in data URL: {e}"))?;

    Ok((mime_value, bytes))
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
    let style = parse_query_style(&query.style);
    let stretch = stretch_to_percent(&query.stretch);
    let mut paths = Vec::new();

    for family in &query.families {
        for face in existing_faces.iter().filter(|face| {
            face.family == *family
                && face.style.matches(style)
                && face.weight.includes(query.weight)
                && face.stretch.includes(stretch)
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

fn parse_query_style(style: &str) -> QueryStyle {
    match style {
        "Italic" => QueryStyle::Italic,
        "Oblique" => QueryStyle::Oblique { deg: 14.0 },
        _ => QueryStyle::Normal,
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

fn stretch_to_percent(stretch: &str) -> f32 {
    match stretch {
        "UltraCondensed" => 50.0,
        "ExtraCondensed" => 62.5,
        "Condensed" => 75.0,
        "SemiCondensed" => 87.5,
        "SemiExpanded" => 112.5,
        "Expanded" => 125.0,
        "ExtraExpanded" => 150.0,
        "UltraExpanded" => 200.0,
        _ => 100.0,
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

fn paths_equivalent(left: &Path, right: &Path) -> bool {
    if left == right {
        return true;
    }

    let Ok(left_canonical) = std::fs::canonicalize(left) else {
        return false;
    };
    let Ok(right_canonical) = std::fs::canonicalize(right) else {
        return false;
    };

    left_canonical == right_canonical
}

fn find_face_id_for_path(db: &usvg::fontdb::Database, path: &Path) -> Option<usvg::fontdb::ID> {
    for face in db.faces() {
        match &face.source {
            usvg::fontdb::Source::File(face_path) if paths_equivalent(face_path, path) => {
                return Some(face.id);
            }
            usvg::fontdb::Source::SharedFile(face_path, _) if paths_equivalent(face_path, path) => {
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
    if let Some(id) = find_face_id_for_path(db, path) {
        let mut loaded = loaded_paths
            .lock()
            .map_err(|_| "failed to access loaded font path set".to_string())?;
        loaded.insert(path.to_path_buf());
        return Ok(id);
    }

    let before_ids = db.faces().map(|face| face.id).collect::<BTreeSet<_>>();

    Arc::make_mut(db).load_font_file(path).map_err(|err| {
        format!(
            "failed to load font '{}' into usvg db: {err}",
            path.display()
        )
    })?;

    let mut loaded = loaded_paths
        .lock()
        .map_err(|_| "failed to access loaded font path set".to_string())?;
    loaded.insert(path.to_path_buf());

    if let Some(id) = find_face_id_for_path(db, path) {
        return Ok(id);
    }

    if let Some(id) = db.faces().find_map(|face| {
        if before_ids.contains(&face.id) {
            return None;
        }
        Some(face.id)
    }) {
        return Ok(id);
    }

    Err(format!(
        "loaded font '{}' but could not find face id in font database",
        path.display()
    ))
}

fn remove_font_family_attr_from_start_tag(start_tag: &str) -> String {
    let marker = " font-family=\"";
    let Some(attr_start) = start_tag.find(marker) else {
        return start_tag.to_string();
    };
    let value_start = attr_start + marker.len();
    let Some(rel_end_quote) = start_tag[value_start..].find('"') else {
        return start_tag.to_string();
    };
    let attr_end = value_start + rel_end_quote + 1;

    let mut out = String::with_capacity(start_tag.len());
    out.push_str(&start_tag[..attr_start]);
    out.push_str(&start_tag[attr_end..]);
    out
}

fn rewrite_font_family_in_start_tag(start_tag: &str, family: Option<&str>) -> String {
    let mut out = remove_font_family_attr_from_start_tag(start_tag);
    let insert_at = if out.ends_with("/>") {
        out.len().saturating_sub(2)
    } else {
        out.len().saturating_sub(1)
    };

    if let Some(family) = family {
        let escaped = xml_escape_attr(family);
        out.insert_str(insert_at, &format!(" font-family=\"{escaped}\""));
    }

    out
}

fn sanitize_css_declaration_block(block: &str) -> String {
    let mut out = String::new();
    let mut cursor = 0usize;

    while cursor < block.len() {
        let decl = &block[cursor..];
        let end = find_declaration_terminator(decl);
        let declaration = decl[..end].trim();

        let keep = declaration
            .split_once(':')
            .map(|(name, _)| {
                let name = name.trim();
                !name.eq_ignore_ascii_case("font-family") && !name.eq_ignore_ascii_case("font")
            })
            .unwrap_or(true);

        if keep && !declaration.is_empty() {
            if !out.is_empty() && !out.ends_with(|c: char| c.is_whitespace() || c == '{') {
                out.push(' ');
            }
            out.push_str(declaration);
            out.push(';');
        }

        if end == decl.len() {
            break;
        }
        cursor += end + 1;
    }

    out
}

fn sanitize_preserved_css(css: &str) -> String {
    let mut out = String::new();
    let mut cursor = 0usize;

    while let Some(rel_open) = css[cursor..].find('{') {
        let open = cursor + rel_open;
        let Some(close_rel) = find_block_end(&css[open..]) else {
            out.push_str(&css[cursor..]);
            return out;
        };
        let close = open + close_rel;
        out.push_str(&css[cursor..=open]);
        let inner = &css[open + 1..close];
        if inner.contains('{') {
            out.push_str(&sanitize_preserved_css(inner));
        } else {
            out.push_str(&sanitize_css_declaration_block(inner));
        }
        out.push('}');
        cursor = close + 1;
    }

    out.push_str(&css[cursor..]);
    out
}

fn collect_preserved_style_css(svg: &str) -> Result<String, String> {
    let normalized_svg: Option<String>;
    let doc = match roxmltree::Document::parse(svg) {
        Ok(doc) => doc,
        Err(original_err) => {
            let quote_normalized = normalize_inner_attribute_quotes_preserve_offsets(svg);
            if roxmltree::Document::parse(&quote_normalized).is_ok() {
                normalized_svg = Some(quote_normalized);
                roxmltree::Document::parse(
                    normalized_svg
                        .as_deref()
                        .expect("normalized SVG should be present"),
                )
                .map_err(|_| {
                    format!("failed to parse SVG while collecting preserved CSS: {original_err}")
                })?
            } else {
                let normalized = normalize_svg_for_strict_parse(svg)?;
                let Cow::Owned(owned) = normalized else {
                    return Err(format!(
                        "failed to parse SVG while collecting preserved CSS: {original_err}"
                    ));
                };
                normalized_svg = Some(owned);
                roxmltree::Document::parse(
                    normalized_svg
                        .as_deref()
                        .expect("normalized SVG should be present"),
                )
                .map_err(|_| {
                    format!("failed to parse SVG while collecting preserved CSS: {original_err}")
                })?
            }
        }
    };
    let css = doc
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "style")
        .filter_map(|n| n.text())
        .map(sanitize_preserved_css)
        .filter(|css| !css.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    Ok(css)
}

fn collect_preserved_defs_content(
    source_svg: &str,
    normalized_svg: &str,
) -> Result<String, String> {
    let normalized_source_svg: Option<String>;
    let source_doc = match roxmltree::Document::parse(source_svg) {
        Ok(doc) => doc,
        Err(original_err) => {
            let normalized = normalize_svg_for_strict_parse(source_svg)?;
            let Cow::Owned(owned) = normalized else {
                return Ok(String::new());
            };
            normalized_source_svg = Some(owned);
            roxmltree::Document::parse(
                normalized_source_svg
                    .as_deref()
                    .expect("normalized SVG should be present"),
            )
            .unwrap_or_else(|_| {
                let _ = original_err;
                return roxmltree::Document::parse("<svg xmlns=\"http://www.w3.org/2000/svg\"/>")
                    .expect("fallback svg should parse");
            })
        }
    };
    let Ok(normalized_doc) = roxmltree::Document::parse(normalized_svg) else {
        return Ok(String::new());
    };

    let existing_ids = normalized_doc
        .descendants()
        .filter(|n| n.is_element())
        .filter_map(|n| n.attribute("id").map(str::to_string))
        .collect::<BTreeSet<_>>();

    let mut fragments = Vec::new();
    for defs in source_doc
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "defs")
    {
        for child in defs.children().filter(|n| n.is_element()) {
            if child.tag_name().name() == "style" {
                continue;
            }
            if child
                .attribute("id")
                .is_some_and(|id| existing_ids.contains(id))
            {
                continue;
            }
            let range = child.range();
            fragments.push(source_svg[range.start..range.end].to_string());
        }
    }

    Ok(fragments.join(""))
}

fn alias_key_for_request(request: &FontQuery, path: &Path) -> (PathBuf, String, u16, String) {
    (
        path.to_path_buf(),
        request.style.clone(),
        request.weight,
        request.stretch.clone(),
    )
}

fn synthetic_family_name(request: &FontQuery, font: &EmbeddedFont) -> String {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(font.base64_data.as_bytes());
    bytes.extend_from_slice(request.style.as_bytes());
    bytes.extend_from_slice(request.stretch.as_bytes());
    bytes.extend_from_slice(request.weight.to_string().as_bytes());
    let hash = fnv1a64(&bytes);
    format!("svg-font-{hash:016x}")
}

fn path_supports_char(
    db: &mut Arc<usvg::fontdb::Database>,
    loaded_paths: &Arc<Mutex<BTreeSet<PathBuf>>>,
    cache: &mut BTreeMap<(PathBuf, char), bool>,
    path: &Path,
    c: char,
) -> Result<bool, String> {
    let key = (path.to_path_buf(), c);
    if let Some(result) = cache.get(&key) {
        return Ok(*result);
    }

    let id = ensure_font_loaded(db, loaded_paths, path)?;
    let supports = db_face_supports_char(db, id, c);
    cache.insert(key, supports);
    Ok(supports)
}

fn collect_span_alias_lists(
    tree: &usvg::Tree,
    resolved_paths: &BTreeMap<FontQuery, PathBuf>,
    runtime_to_emit: &BTreeMap<PathBuf, PathBuf>,
    alias_by_key: &BTreeMap<(PathBuf, String, u16, String), String>,
    alias_by_descriptor: &BTreeMap<(String, u16, String), String>,
) -> Result<Vec<String>, String> {
    fn visit_group(
        group: &usvg::Group,
        resolved_paths: &BTreeMap<FontQuery, PathBuf>,
        runtime_to_emit: &BTreeMap<PathBuf, PathBuf>,
        alias_by_key: &BTreeMap<(PathBuf, String, u16, String), String>,
        alias_by_descriptor: &BTreeMap<(String, u16, String), String>,
        db: &mut Arc<usvg::fontdb::Database>,
        loaded_paths: &Arc<Mutex<BTreeSet<PathBuf>>>,
        support_cache: &mut BTreeMap<(PathBuf, char), bool>,
        out: &mut Vec<String>,
    ) -> Result<(), String> {
        for node in group.children() {
            match node {
                usvg::Node::Group(child) => visit_group(
                    child,
                    resolved_paths,
                    runtime_to_emit,
                    alias_by_key,
                    alias_by_descriptor,
                    db,
                    loaded_paths,
                    support_cache,
                    out,
                )?,
                usvg::Node::Text(text) => {
                    for chunk in text.chunks() {
                        for span in chunk.spans() {
                            let primary_query = font_spec_to_query(span.font(), None);
                            let primary_runtime = resolved_paths.get(&primary_query).ok_or_else(|| {
                                format!(
                                    "failed to map normalized text span to resolved font request: {primary_query:?}"
                                )
                            })?;
                            let primary_emit = runtime_to_emit
                                .get(primary_runtime)
                                .cloned()
                                .unwrap_or_else(|| primary_runtime.clone());

                            let primary_key = alias_key_for_request(&primary_query, &primary_emit);
                            let primary_alias = alias_by_key
                                .get(&primary_key)
                                .cloned()
                                .or_else(|| {
                                    alias_by_descriptor
                                        .get(&(
                                            primary_query.style.clone(),
                                            primary_query.weight,
                                            primary_query.stretch.clone(),
                                        ))
                                        .cloned()
                                })
                                .ok_or_else(|| {
                                    format!(
                                        "missing synthetic alias for normalized span face: {primary_query:?} -> {}",
                                        primary_emit.display()
                                    )
                                })?;

                            let mut aliases = vec![primary_alias];
                            let mut used_runtime_paths = vec![primary_runtime.clone()];

                            for c in chunk.text()[span.start()..span.end()].chars() {
                                let already_supported = used_runtime_paths.iter().try_fold(
                                    false,
                                    |supported, runtime_path| {
                                        if supported {
                                            return Ok(true);
                                        }
                                        path_supports_char(
                                            db,
                                            loaded_paths,
                                            support_cache,
                                            runtime_path,
                                            c,
                                        )
                                    },
                                )?;
                                if already_supported {
                                    continue;
                                }

                                let mut fallback_query = primary_query.clone();
                                fallback_query.missing_char = Some(c);
                                let fallback_runtime = resolved_paths.get(&fallback_query).ok_or_else(|| {
                                    format!(
                                        "failed to map normalized span fallback request: {fallback_query:?}"
                                    )
                                })?;
                                if used_runtime_paths.contains(fallback_runtime) {
                                    continue;
                                }

                                used_runtime_paths.push(fallback_runtime.clone());
                                let fallback_emit = runtime_to_emit
                                    .get(fallback_runtime)
                                    .cloned()
                                    .unwrap_or_else(|| fallback_runtime.clone());
                                let fallback_key =
                                    alias_key_for_request(&fallback_query, &fallback_emit);
                                let fallback_alias = alias_by_key
                                    .get(&fallback_key)
                                    .cloned()
                                    .or_else(|| {
                                        alias_by_descriptor
                                            .get(&(
                                                fallback_query.style.clone(),
                                                fallback_query.weight,
                                                fallback_query.stretch.clone(),
                                            ))
                                            .cloned()
                                    })
                                    .ok_or_else(|| {
                                        format!(
                                            "missing synthetic alias for normalized fallback face: {fallback_query:?} -> {}",
                                            fallback_emit.display()
                                        )
                                    })?;
                                aliases.push(fallback_alias);
                            }

                            aliases.dedup();
                            out.push(aliases.join(", "));
                        }
                    }
                }
                _ => {}
            }
        }

        Ok(())
    }

    let mut db = Arc::new(usvg::fontdb::Database::new());
    let loaded_paths = Arc::new(Mutex::new(BTreeSet::new()));
    let mut support_cache = BTreeMap::new();
    let mut out = Vec::new();
    visit_group(
        tree.root(),
        resolved_paths,
        runtime_to_emit,
        alias_by_key,
        alias_by_descriptor,
        &mut db,
        &loaded_paths,
        &mut support_cache,
        &mut out,
    )?;
    Ok(out)
}

fn escape_inner_quotes_in_font_family_attrs(svg: &str) -> String {
    let marker = "font-family=\"";
    let mut out = String::with_capacity(svg.len());
    let mut cursor = 0usize;

    while let Some(rel_start) = svg[cursor..].find(marker) {
        let attr_start = cursor + rel_start;
        let value_start = attr_start + marker.len();
        out.push_str(&svg[cursor..value_start]);

        let mut i = value_start;
        while i < svg.len() {
            let mut chars = svg[i..].chars();
            let Some(ch) = chars.next() else {
                break;
            };
            let ch_len = ch.len_utf8();
            if ch == '"' {
                let next = svg[i + ch_len..].chars().next();
                let is_closing = next.is_none()
                    || next.is_some_and(|c| c.is_whitespace() || c == '/' || c == '>');
                if is_closing {
                    out.push('"');
                    i += ch_len;
                    break;
                }
                out.push_str("&quot;");
                i += ch_len;
                continue;
            }

            out.push(ch);
            i += ch_len;
        }

        cursor = i;
    }

    out.push_str(&svg[cursor..]);
    out
}

fn normalize_inner_attribute_quotes_preserve_offsets(svg: &str) -> String {
    let bytes = svg.as_bytes();
    let mut out = String::with_capacity(svg.len());
    let mut i = 0usize;
    let mut in_tag = false;
    let mut in_double_quote = false;
    let mut in_special = false;

    while i < bytes.len() {
        if !in_tag {
            if bytes[i] == b'<' {
                in_tag = true;
                in_special = bytes.get(i + 1).copied() == Some(b'!')
                    || bytes.get(i + 1).copied() == Some(b'?');
            }
            out.push(bytes[i] as char);
            i += 1;
            continue;
        }

        let b = bytes[i];
        if in_special {
            out.push(b as char);
            if b == b'>' {
                in_tag = false;
                in_special = false;
            }
            i += 1;
            continue;
        }

        if in_double_quote {
            if b == b'"' {
                let next = bytes.get(i + 1).copied();
                let is_closing = next.is_none()
                    || next.is_some_and(|n| n.is_ascii_whitespace() || n == b'/' || n == b'>');
                out.push(if is_closing { '"' } else { '\'' });
                if is_closing {
                    in_double_quote = false;
                }
            } else if b == b'<' {
                out.push('.');
            } else {
                out.push(b as char);
            }
            i += 1;
            continue;
        }

        match b {
            b'"' => {
                in_double_quote = true;
                out.push('"');
            }
            b'>' => {
                in_tag = false;
                out.push('>');
            }
            _ => out.push(b as char),
        }
        i += 1;
    }

    out
}

fn rewrite_normalized_svg_font_families(
    svg: &str,
    span_alias_lists: &[String],
) -> Result<String, String> {
    let mut span_index = 0usize;

    let source = escape_inner_quotes_in_font_family_attrs(svg);
    let mut out = String::with_capacity(source.len());
    let mut cursor = 0usize;
    while let Some(rel_start) = source[cursor..].find('<') {
        let start = cursor + rel_start;
        out.push_str(&source[cursor..start]);

        let fragment = &source[start..];
        let Some(end_rel) = find_tag_end_outside_quotes(fragment) else {
            out.push_str(fragment);
            cursor = source.len();
            break;
        };
        let end = start + end_rel + 1;
        let start_tag = &source[start..end];

        let replacement = if start_tag.starts_with("<svg")
            || start_tag.starts_with("<text ")
            || start_tag.starts_with("<text>")
            || start_tag.starts_with("<textPath")
            || start_tag.starts_with("<tspan")
        {
            if start_tag.starts_with("<tspan") && start_tag.contains(" font-size=\"") {
                let family = span_alias_lists.get(span_index).ok_or_else(|| {
                    "missing span alias list during normalized SVG rewrite".to_string()
                })?;
                span_index += 1;
                rewrite_font_family_in_start_tag(start_tag, Some(family))
            } else if start_tag.contains(" font-family=\"") {
                rewrite_font_family_in_start_tag(start_tag, None)
            } else {
                start_tag.to_string()
            }
        } else {
            start_tag.to_string()
        };

        out.push_str(&replacement);
        cursor = end;
    }

    if cursor < source.len() {
        out.push_str(&source[cursor..]);
    }

    if span_index != span_alias_lists.len() {
        return Err(format!(
            "normalized SVG span count mismatch: writer emitted {} styled tspans but tree reported {} spans",
            span_index,
            span_alias_lists.len()
        ));
    }

    Ok(out)
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
    emitted_faces: &[(String, FontQuery, PathBuf)],
    preserved_css: &str,
    embedded: &BTreeMap<PathBuf, EmbeddedFont>,
) -> String {
    let mut css = String::new();
    if !preserved_css.trim().is_empty() {
        css.push_str(preserved_css.trim());
        css.push('\n');
    }
    for (family, request, path) in emitted_faces {
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
        out.push_str("<!-- svg-font-inliner: ");
        out.push_str(&serialized);
        out.push_str(" -->\n");
    }
    Ok(out)
}

fn emit_font_embed_debug_comments() -> bool {
    std::env::var("SVG_FONT_EMBED_DEBUG")
        .map(|v| v == "1")
        .unwrap_or(false)
}

fn inject_style_block_at(
    svg: &str,
    insert_at: usize,
    debug_comments: &str,
    css: &str,
) -> Result<String, String> {
    if insert_at > svg.len() {
        return Err("failed to find root <svg> opening tag".to_string());
    }
    let style_block = format!("{debug_comments}<defs><style><![CDATA[\n{css}]]></style></defs>");
    let mut out = String::with_capacity(svg.len() + style_block.len());
    out.push_str(&svg[..insert_at]);
    out.push_str(&style_block);
    out.push_str(&svg[insert_at..]);
    Ok(out)
}

fn inject_raw_at(svg: &str, insert_at: usize, raw: &str) -> Result<String, String> {
    if insert_at > svg.len() {
        return Err("failed to find root <svg> opening tag".to_string());
    }
    let mut out = String::with_capacity(svg.len() + raw.len());
    out.push_str(&svg[..insert_at]);
    out.push_str(raw);
    out.push_str(&svg[insert_at..]);
    Ok(out)
}

#[cfg(test)]
fn inject_style_block(svg: &str, debug_comments: &str, css: &str) -> Result<String, String> {
    let insert_at = find_root_svg_open_tag_end(svg)
        .ok_or_else(|| "failed to find root <svg> opening tag".to_string())?;
    inject_style_block_at(svg, insert_at, debug_comments, css)
}

pub fn embed_svg_fonts<F>(input_svg: &str, resolver: F) -> Result<String, String>
where
    F: Fn(&FontQuery) -> Result<PathBuf, String> + Send + Sync + 'static,
{
    let (stripped_svg, existing_faces, _root_open_end) =
        collect_existing_faces_and_strip_font_face_rules(input_svg)?;

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
            style: face.style,
            weight: face.weight,
            stretch: face.stretch,
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
    let image_resolver_error: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let image_resolver_error_for_resolve_string = Arc::clone(&image_resolver_error);

    let mut options = usvg::Options::default();
    options.image_href_resolver = usvg::ImageHrefResolver {
        resolve_data: usvg::ImageHrefResolver::default_data_resolver(),
        resolve_string: Box::new(move |href, _opts| {
            if let Ok(mut slot) = image_resolver_error_for_resolve_string.lock() {
                if slot.is_none() {
                    *slot = Some(format!("external image resource is not allowed: {href}"));
                }
            }
            None
        }),
    };
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
                            if used_fonts.contains(&id) {
                                return None;
                            }
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

    let usvg_input = normalize_inner_attribute_quotes_preserve_offsets(&stripped_svg);
    let tree = usvg::Tree::from_data(usvg_input.as_bytes(), &options)
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

    let preserved_css = collect_preserved_style_css(&stripped_svg)?;
    let emitted_faces = merged_request_to_font
        .iter()
        .map(|(request, path)| {
            let font = embedded.get(path).ok_or_else(|| {
                format!(
                    "missing embedded font payload for emitted path: {}",
                    path.display()
                )
            })?;
            Ok((
                synthetic_family_name(request, font),
                request.clone(),
                path.clone(),
            ))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let alias_by_key = emitted_faces
        .iter()
        .map(|(alias, request, path)| (alias_key_for_request(request, path), alias.clone()))
        .collect::<BTreeMap<_, _>>();
    let mut alias_descriptor_candidates =
        BTreeMap::<(String, u16, String), BTreeSet<String>>::new();
    for (alias, request, _) in &emitted_faces {
        alias_descriptor_candidates
            .entry((
                request.style.clone(),
                request.weight,
                request.stretch.clone(),
            ))
            .or_default()
            .insert(alias.clone());
    }
    let alias_by_descriptor = alias_descriptor_candidates
        .into_iter()
        .filter_map(|(descriptor, aliases)| {
            if aliases.len() == 1 {
                aliases.into_iter().next().map(|alias| (descriptor, alias))
            } else {
                None
            }
        })
        .collect::<BTreeMap<_, _>>();

    let normalized_svg = tree.to_string(&usvg::WriteOptions {
        preserve_text: true,
        ..usvg::WriteOptions::default()
    });
    let span_alias_lists = collect_span_alias_lists(
        &tree,
        &resolved_paths_snapshot,
        &runtime_to_emit,
        &alias_by_key,
        &alias_by_descriptor,
    )?;
    let rewritten_svg = rewrite_normalized_svg_font_families(&normalized_svg, &span_alias_lists)?;
    let preserved_defs_content = collect_preserved_defs_content(&stripped_svg, &rewritten_svg)?;
    let rewritten_svg = if preserved_defs_content.is_empty() {
        rewritten_svg
    } else {
        let insert_at = find_root_svg_open_tag_end(&rewritten_svg)
            .ok_or_else(|| "failed to find root <svg> opening tag in normalized SVG".to_string())?;
        inject_raw_at(
            &rewritten_svg,
            insert_at,
            &format!("<defs>{preserved_defs_content}</defs>"),
        )?
    };

    let css = build_css(&emitted_faces, &preserved_css, &embedded);
    if css.trim().is_empty() {
        return Err("no font requests were found; refusing to emit unchanged SVG".to_string());
    }

    let debug_comments = if emit_font_embed_debug_comments() {
        build_debug_comments(&resolve_debug)?
    } else {
        String::new()
    };
    let normalized_root_open_end = find_root_svg_open_tag_end(&rewritten_svg)
        .ok_or_else(|| "failed to find root <svg> opening tag in rewritten SVG".to_string())?;
    let output_svg = inject_style_block_at(
        &rewritten_svg,
        normalized_root_open_end,
        &debug_comments,
        &css,
    )?;

    Ok(output_svg)
}

pub fn ensure_text_fonts_inline(input_svg: &str) -> Result<(), String> {
    embed_svg_fonts(input_svg, |_query| {
        Err("font is not inlined in SVG @font-face data source".to_string())
    })
    .map(|_| ())
}

pub fn parse_svg_tree_inline_fonts_only(input_svg: &str) -> Result<usvg::Tree, String> {
    let (stripped_svg, existing_faces, _root_open_end) =
        collect_existing_faces_and_strip_font_face_rules(input_svg)?;

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
            style: face.style,
            weight: face.weight,
            stretch: face.stretch,
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
    let image_resolver_error: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let image_resolver_error_for_resolve_string = Arc::clone(&image_resolver_error);

    let mut options = usvg::Options::default();
    options.image_href_resolver = usvg::ImageHrefResolver {
        resolve_data: usvg::ImageHrefResolver::default_data_resolver(),
        resolve_string: Box::new(move |href, _opts| {
            if let Ok(mut slot) = image_resolver_error_for_resolve_string.lock() {
                if slot.is_none() {
                    *slot = Some(format!("external image resource is not allowed: {href}"));
                }
            }
            None
        }),
    };
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

    let usvg_input = normalize_inner_attribute_quotes_preserve_offsets(&stripped_svg);
    let tree = usvg::Tree::from_data(usvg_input.as_bytes(), &options)
        .map_err(|e| format!("failed to parse SVG: {e}"))?;

    if let Ok(slot) = image_resolver_error.lock() {
        if let Some(err) = slot.as_ref() {
            return Err(err.clone());
        }
    }

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

    fn fixture_font_path(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join(name)
    }

    #[test]
    fn inject_style_block_preserves_xml_declaration_and_text() {
        let input = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\"><text>hello</text></svg>";
        let out = inject_style_block(
            input,
            "<!-- svg-font-inliner: {\"debug\":true} -->\n",
            "@font-face { font-family: \"x\"; }",
        )
        .expect("injection should succeed");

        assert!(out.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
        assert!(out.contains("<text>hello</text>"));
        assert!(out.contains("@font-face"));
    }

    #[test]
    fn inject_style_block_ignores_svg_marker_inside_comment() {
        let input = "<!-- prelude <svg fake-root> -->\n<svg xmlns=\"http://www.w3.org/2000/svg\"><text>hello</text></svg>";
        let out = inject_style_block(input, "", "@font-face { font-family: \"x\"; }")
            .expect("injection should succeed");

        let root_idx = out
            .find("<svg xmlns=\"http://www.w3.org/2000/svg\">")
            .expect("expected real root svg element");
        let style_idx = out
            .find("<defs><style><![CDATA[")
            .expect("expected injected style block");
        assert!(
            style_idx > root_idx,
            "style block should be inserted inside real root svg, not inside comments"
        );
    }

    #[test]
    fn remove_inliner_debug_comments_keeps_unrelated_comments() {
        let input = "<!-- keep-me -->\n<!-- svg-font-inliner: {\"debug\":true} -->\n<svg xmlns=\"http://www.w3.org/2000/svg\"/>";
        let out = remove_inliner_debug_comments(input);
        assert!(out.contains("<!-- keep-me -->"));
        assert!(!out.contains("svg-font-inliner:"));
    }

    #[test]
    fn decode_data_url_accepts_leading_whitespace() {
        let url = " \t\n data:font/ttf;base64,AA==";
        let (mime, bytes) = decode_data_url(url).expect("data URL should decode");
        assert_eq!(mime, "font/ttf");
        assert_eq!(bytes, vec![0u8]);
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

    #[test]
    fn ensure_font_loaded_recovers_when_path_already_marked_loaded() {
        let font_path = fixture_font_path("font-a.ttf");
        let mut db = Arc::new(usvg::fontdb::Database::new());
        let loaded_paths = Arc::new(Mutex::new(BTreeSet::from([font_path.clone()])));

        let id = ensure_font_loaded(&mut db, &loaded_paths, &font_path)
            .expect("should recover and resolve face id even when path was already marked loaded");

        assert!(
            db.face(id).is_some(),
            "resolved face id should exist in database"
        );
    }

    #[test]
    fn parse_svg_tree_rejects_external_image_href_url() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="2" height="2"><image href="https://example.com/a.png" width="2" height="2"/></svg>"#;
        let err =
            parse_svg_tree_inline_fonts_only(svg).expect_err("external image href should fail");
        assert!(err.contains("external image resource is not allowed"));
    }

    #[test]
    fn parse_svg_tree_rejects_external_image_href_path() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="2" height="2"><image href="a.png" width="2" height="2"/></svg>"#;
        let err = parse_svg_tree_inline_fonts_only(svg).expect_err("file image href should fail");
        assert!(err.contains("external image resource is not allowed"));
    }

    #[test]
    fn parse_svg_tree_allows_data_url_image_href() {
        let one_px =
            "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+/a0cAAAAASUVORK5CYII=";
        let svg = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1\" height=\"1\"><image href=\"data:image/png;base64,{one_px}\" width=\"1\" height=\"1\"/></svg>"
        );

        parse_svg_tree_inline_fonts_only(&svg).expect("data URL image href should be allowed");
    }
}
