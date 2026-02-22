use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;
use std::process::{self, Command, Stdio};

use base64::Engine;
use regex::Regex;
use serde::Deserialize;
use serde::Serialize;

#[derive(Clone, Debug, Deserialize)]
struct RendererConfig {
    bin: String,
    version: String,
    formats: Vec<String>,
    renderer: String,
}

type AvailableRenderers = HashMap<String, Vec<RendererConfig>>;

#[derive(Clone, Debug)]
struct Request {
    renderer: String,
    format: String,
    code: String,
}

#[derive(Clone, Debug, Serialize)]
struct OutputItem {
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

fn fail(msg: impl AsRef<str>) -> ! {
    eprintln!("{}", msg.as_ref());
    process::exit(1);
}

fn parse_available_renderers() -> AvailableRenderers {
    let raw =
        env::var("AVAILABLE_RENDERERS").unwrap_or_else(|_| fail("AVAILABLE_RENDERERS undefined"));
    serde_json::from_str(&raw)
        .unwrap_or_else(|e| fail(format!("failed to parse AVAILABLE_RENDERERS: {e}")))
}

fn parse_stdin(stdin: &str, available: &AvailableRenderers) -> Vec<Request> {
    let value: serde_json::Value = serde_json::from_str(stdin)
        .unwrap_or_else(|e| fail(format!("failed to parse stdin JSON: {e}")));

    let arr = value
        .as_array()
        .unwrap_or_else(|| fail(format!("Stdin must be Array, got: {value}")));

    let renderer_index: HashMap<&str, &RendererConfig> = available
        .values()
        .flat_map(|configs| configs.iter())
        .map(|config| (config.renderer.as_str(), config))
        .collect();

    arr.iter()
        .map(|item| {
            let obj = item
                .as_object()
                .unwrap_or_else(|| fail("each stdin item must be an object"));

            let renderer = obj
                .get("renderer")
                .and_then(|v| v.as_str())
                .unwrap_or_else(|| fail("renderer must be a string"));
            let format = obj
                .get("format")
                .and_then(|v| v.as_str())
                .unwrap_or_else(|| fail("format must be a string"));
            let code = obj
                .get("code")
                .and_then(|v| v.as_str())
                .unwrap_or_else(|| fail("code must be a string"));

            let config = renderer_index.get(renderer).unwrap_or_else(|| {
                let available_renderers = renderer_index.keys().copied().collect::<Vec<_>>().join(",");
                fail(format!(
                    "renderer not available. Renderer: {renderer}, available renderers: {available_renderers}"
                ));
            });

            if !config.formats.iter().any(|f| f == format) {
                fail(format!("format not supported for renderer {renderer}: {format}"));
            }

            Request {
                renderer: renderer.to_string(),
                format: format.to_string(),
                code: code.to_string(),
            }
        })
        .collect()
}

fn extract_svg(input: &str) -> Option<String> {
    let start = input.find("<svg")?;
    let end = input.rfind("</svg>")? + "</svg>".len();
    Some(input[start..end].to_string())
}

fn run_command_with_stdin(
    bin: &str,
    args: &[&str],
    cwd: Option<&Path>,
    stdin: &str,
) -> Result<String, String> {
    let mut command = Command::new(bin);
    command.args(args);
    if let Some(dir) = cwd {
        command.current_dir(dir);
    }
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    if let Ok(fontconfig_file) = env::var("FONTCONFIG_FILE") {
        command.env("FONTCONFIG_FILE", fontconfig_file);
    }

    let mut child = command
        .spawn()
        .map_err(|e| format!("failed to spawn '{bin}': {e}"))?;

    {
        let mut child_stdin = child
            .stdin
            .take()
            .ok_or_else(|| format!("failed to open stdin for '{bin}'"))?;
        child_stdin
            .write_all(stdin.as_bytes())
            .map_err(|e| format!("failed to write stdin for '{bin}': {e}"))?;
    }

    let output = child
        .wait_with_output()
        .map_err(|e| format!("failed to wait for '{bin}': {e}"))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        if stderr.trim().is_empty() {
            Err(format!("'{bin}' exited with status {}", output.status))
        } else {
            Err(stderr)
        }
    }
}

fn run_command(bin: &str, args: &[&str], cwd: Option<&Path>) -> Result<(), String> {
    let mut command = Command::new(bin);
    command.args(args);
    if let Some(dir) = cwd {
        command.current_dir(dir);
    }
    command.stdout(Stdio::piped()).stderr(Stdio::piped());

    if let Ok(fontconfig_file) = env::var("FONTCONFIG_FILE") {
        command.env("FONTCONFIG_FILE", fontconfig_file);
    }

    let output = command
        .output()
        .map_err(|e| format!("failed to run '{bin}': {e}"))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        if stderr.trim().is_empty() {
            Err(format!("'{bin}' exited with status {}", output.status))
        } else {
            Err(stderr)
        }
    }
}

fn embed_svg_fonts(svg: &str) -> Result<String, String> {
    svg_font_inliner::embed_svg_fonts(svg)
}

fn find_renderer<'a>(
    available: &'a AvailableRenderers,
    engine: &str,
    version: &str,
) -> Result<&'a RendererConfig, String> {
    available
        .get(engine)
        .and_then(|list| list.iter().find(|r| r.version == version))
        .ok_or_else(|| format!("renderer version not found: {engine}-{version}"))
}

fn render_internal(
    codes: &[String],
    renderer: &str,
    format: &str,
    available: &AvailableRenderers,
    plantuml_re: &Regex,
    recharts_re: &Regex,
    swirly_re: &Regex,
) -> Result<Vec<String>, String> {
    if let Some(caps) = plantuml_re.captures(renderer) {
        let version = caps
            .name("version")
            .map(|m| m.as_str())
            .ok_or_else(|| "missing plantuml version".to_string())?;
        let used = find_renderer(available, "plantuml", version)?;

        let temp = tempfile::Builder::new()
            .prefix("diagram-generator-")
            .tempdir()
            .map_err(|e| format!("failed to create temp dir: {e}"))?;
        let cwd = temp.path();

        for (i, code) in codes.iter().enumerate() {
            fs::write(cwd.join(format!("in_{i}.puml")), code)
                .map_err(|e| format!("failed to write plantuml input file: {e}"))?;
        }

        if format == "png" {
            run_command(
                &used.bin,
                &[".", "-tpng", "-o", "out", "-nometadata"],
                Some(cwd),
            )?;
            let mut outputs = Vec::with_capacity(codes.len());
            for i in 0..codes.len() {
                let bytes = fs::read(cwd.join("out").join(format!("in_{i}.png")))
                    .map_err(|e| format!("failed to read plantuml png output: {e}"))?;
                outputs.push(base64::engine::general_purpose::STANDARD.encode(bytes));
            }
            Ok(outputs)
        } else {
            run_command(
                &used.bin,
                &[".", "-tsvg", "-o", "out", "-nometadata"],
                Some(cwd),
            )?;
            let mut outputs = Vec::with_capacity(codes.len());
            for i in 0..codes.len() {
                let contents = fs::read_to_string(cwd.join("out").join(format!("in_{i}.svg")))
                    .map_err(|e| format!("failed to read plantuml svg output: {e}"))?;
                let extracted = extract_svg(&contents)
                    .ok_or_else(|| "plantuml output did not contain an svg root".to_string())?;
                let embedded = embed_svg_fonts(&extracted)?;
                outputs.push(embedded);
            }
            Ok(outputs)
        }
    } else if let Some(caps) = recharts_re.captures(renderer) {
        let version = caps
            .name("version")
            .map(|m| m.as_str())
            .ok_or_else(|| "missing recharts version".to_string())?;
        let used = find_renderer(available, "recharts", version)?;

        let mut outputs = Vec::with_capacity(codes.len());
        for code in codes {
            let stdout = run_command_with_stdin(&used.bin, &[], None, code)?;
            let embedded = embed_svg_fonts(stdout.trim())?;
            outputs.push(embedded);
        }
        Ok(outputs)
    } else if let Some(caps) = swirly_re.captures(renderer) {
        let version = caps
            .name("version")
            .map(|m| m.as_str())
            .ok_or_else(|| "missing swirly version".to_string())?;
        let used = find_renderer(available, "swirly", version)?;

        let mut outputs = Vec::with_capacity(codes.len());
        for code in codes {
            let stdout = run_command_with_stdin(&used.bin, &[], None, code)?;
            let embedded = embed_svg_fonts(stdout.trim())?;
            outputs.push(embedded);
        }
        Ok(outputs)
    } else {
        Err(format!("Not supported renderer: {renderer}"))
    }
}

fn render(
    codes: &[String],
    renderer: &str,
    format: &str,
    available: &AvailableRenderers,
    plantuml_re: &Regex,
    recharts_re: &Regex,
    swirly_re: &Regex,
) -> Vec<OutputItem> {
    if codes.is_empty() {
        return Vec::new();
    }

    match render_internal(
        codes,
        renderer,
        format,
        available,
        plantuml_re,
        recharts_re,
        swirly_re,
    ) {
        Ok(results) => results
            .into_iter()
            .map(|result| OutputItem {
                result: Some(result),
                error: None,
            })
            .collect(),
        Err(err) => {
            if codes.len() == 1 {
                vec![OutputItem {
                    result: None,
                    error: Some(err),
                }]
            } else {
                codes
                    .iter()
                    .flat_map(|code| {
                        render(
                            &[code.clone()],
                            renderer,
                            format,
                            available,
                            plantuml_re,
                            recharts_re,
                            swirly_re,
                        )
                    })
                    .collect()
            }
        }
    }
}

fn main() {
    let available = parse_available_renderers();

    let mut stdin = String::new();
    io::stdin()
        .read_to_string(&mut stdin)
        .unwrap_or_else(|e| fail(format!("failed to read stdin: {e}")));

    let requests = parse_stdin(&stdin, &available);

    let plantuml_re = Regex::new(r"^plantuml-(?P<version>.*)$")
        .unwrap_or_else(|e| fail(format!("regex compile error: {e}")));
    let recharts_re = Regex::new(r"^recharts-(?P<version>.*)$")
        .unwrap_or_else(|e| fail(format!("regex compile error: {e}")));
    let swirly_re = Regex::new(r"^swirly-(?P<version>.*)$")
        .unwrap_or_else(|e| fail(format!("regex compile error: {e}")));

    let mut groups: HashMap<String, Vec<(usize, Request)>> = HashMap::new();
    let mut group_order: Vec<String> = Vec::new();

    for (index, request) in requests.into_iter().enumerate() {
        let key = format!("{}\u{0}{}", request.renderer, request.format);
        if !groups.contains_key(&key) {
            group_order.push(key.clone());
        }
        groups.entry(key).or_default().push((index, request));
    }

    let mut indexed_results: Vec<(usize, OutputItem)> = Vec::new();
    for key in group_order {
        let group = groups
            .remove(&key)
            .unwrap_or_else(|| fail("internal error: missing group"));
        if group.is_empty() {
            continue;
        }

        let renderer = group[0].1.renderer.clone();
        let format = group[0].1.format.clone();
        let codes = group
            .iter()
            .map(|(_, req)| req.code.clone())
            .collect::<Vec<_>>();

        let results = render(
            &codes,
            &renderer,
            &format,
            &available,
            &plantuml_re,
            &recharts_re,
            &swirly_re,
        );

        if results.len() != group.len() {
            fail(format!(
                "internal error: result length mismatch for renderer {renderer}: expected {}, got {}",
                group.len(),
                results.len()
            ));
        }

        for ((index, _), result) in group.into_iter().zip(results.into_iter()) {
            indexed_results.push((index, result));
        }
    }

    indexed_results.sort_by_key(|(idx, _)| *idx);
    let output = indexed_results
        .into_iter()
        .map(|(_, result)| result)
        .collect::<Vec<_>>();

    let json = serde_json::to_string(&output)
        .unwrap_or_else(|e| fail(format!("failed to serialize output JSON: {e}")));
    println!("{json}");
}
