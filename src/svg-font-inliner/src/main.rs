use std::env;
use std::fs;
use std::process;

fn usage(binary_name: &str) {
    eprintln!("Usage: {binary_name} <input-svg> <output-svg>");
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        usage(&args[0]);
        process::exit(2);
    }

    let input_svg_path = &args[1];
    let output_svg_path = &args[2];

    let input_svg = fs::read_to_string(input_svg_path).unwrap_or_else(|e| {
        eprintln!("failed to read SVG file '{input_svg_path}': {e}");
        process::exit(1);
    });

    let output_svg = svg_font_inliner::embed_svg_fonts(&input_svg).unwrap_or_else(|e| {
        eprintln!("{e}");
        process::exit(1);
    });

    fs::write(output_svg_path, output_svg).unwrap_or_else(|e| {
        eprintln!("failed to write output SVG '{output_svg_path}': {e}");
        process::exit(1);
    });

    eprintln!("wrote SVG with embedded fonts to '{output_svg_path}'");
}
