use std::env;
use std::io::{self, Read, Write};
use std::process;

fn usage(binary_name: &str) {
    eprintln!(
        "Usage: {binary_name} [--zoom <float>] [--background <color>] < input.svg > output.png"
    );
}

struct CliOptions {
    zoom: f32,
    background: Option<svg_to_png::BackgroundColor>,
}

fn parse_args() -> Result<CliOptions, String> {
    let mut args = env::args().skip(1);
    let mut zoom = 1.0f32;
    let mut background = None;

    while let Some(arg) = args.next() {
        if arg == "--zoom" {
            let value = args
                .next()
                .ok_or_else(|| "--zoom requires a numeric value".to_string())?;
            zoom = value
                .parse::<f32>()
                .map_err(|_| "--zoom must be a finite number greater than 0".to_string())?;
        } else if arg == "--background" {
            let value = args
                .next()
                .ok_or_else(|| "--background requires a color value".to_string())?;
            background = Some(svg_to_png::BackgroundColor::parse(&value)?);
        } else if arg == "-h" || arg == "--help" {
            usage(
                &env::args()
                    .next()
                    .unwrap_or_else(|| "svg-to-png".to_string()),
            );
            process::exit(0);
        } else {
            return Err(format!("unknown argument: {arg}"));
        }
    }

    if !zoom.is_finite() || zoom <= 0.0 {
        return Err("--zoom must be a finite number greater than 0".to_string());
    }

    Ok(CliOptions { zoom, background })
}

fn main() {
    let options = parse_args().unwrap_or_else(|err| {
        eprintln!("{err}");
        usage(
            &env::args()
                .next()
                .unwrap_or_else(|| "svg-to-png".to_string()),
        );
        process::exit(2);
    });

    let mut stdin = String::new();
    io::stdin().read_to_string(&mut stdin).unwrap_or_else(|e| {
        eprintln!("failed to read stdin: {e}");
        process::exit(1);
    });

    let png = svg_to_png::render_svg_to_png(&stdin, options.zoom, options.background)
        .unwrap_or_else(|e| {
            eprintln!("{e}");
            process::exit(1);
        });

    io::stdout().write_all(&png).unwrap_or_else(|e| {
        eprintln!("failed to write stdout: {e}");
        process::exit(1);
    });
}
