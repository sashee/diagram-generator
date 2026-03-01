#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BackgroundColor {
    red: u8,
    green: u8,
    blue: u8,
    alpha: u8,
}

impl BackgroundColor {
    pub fn parse(input: &str) -> Result<Self, String> {
        let parsed = input
            .parse::<svgtypes::Color>()
            .map_err(|e| format!("--background must be a valid color: {e}"))?;
        Ok(Self {
            red: parsed.red,
            green: parsed.green,
            blue: parsed.blue,
            alpha: parsed.alpha,
        })
    }

    fn to_tiny_skia(self) -> resvg::tiny_skia::Color {
        resvg::tiny_skia::Color::from_rgba8(self.red, self.green, self.blue, self.alpha)
    }
}

pub fn render_svg_to_png(
    svg: &str,
    zoom: f32,
    background: Option<BackgroundColor>,
) -> Result<Vec<u8>, String> {
    if !zoom.is_finite() || zoom <= 0.0 {
        return Err("--zoom must be a finite number greater than 0".to_string());
    }

    let tree = svg_font_inliner::parse_svg_tree_inline_fonts_only(svg)?;

    let width = ((tree.size().width() * zoom).ceil() as u32).max(1);
    let height = ((tree.size().height() * zoom).ceil() as u32).max(1);
    let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height)
        .ok_or_else(|| "failed to allocate output pixmap".to_string())?;

    if let Some(bg) = background {
        pixmap.fill(bg.to_tiny_skia());
    }

    let transform = resvg::tiny_skia::Transform::from_scale(zoom, zoom);
    resvg::render(&tree, transform, &mut pixmap.as_mut());

    pixmap
        .encode_png()
        .map_err(|e| format!("failed to encode PNG: {e}"))
}
