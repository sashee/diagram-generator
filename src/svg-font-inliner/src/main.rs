use std::env;
use std::io::{self, Read, Write};
use std::process;

fn usage(binary_name: &str) {
    eprintln!("Usage: {binary_name} < input.svg > output.svg");
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 1 {
        usage(&args[0]);
        process::exit(2);
    }

    let mut input_svg = String::new();
    io::stdin()
        .read_to_string(&mut input_svg)
        .unwrap_or_else(|e| {
            eprintln!("failed to read stdin: {e}");
            process::exit(1);
        });

    let output_svg =
        svg_font_inliner::embed_svg_fonts(&input_svg, svg_font_inliner::resolve_font_with_fc_match)
            .unwrap_or_else(|e| {
                eprintln!("{e}");
                process::exit(1);
            });

    io::stdout()
        .write_all(output_svg.as_bytes())
        .unwrap_or_else(|e| {
            eprintln!("failed to write stdout: {e}");
            process::exit(1);
        });
}
