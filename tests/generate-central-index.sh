#!/usr/bin/env bash
set +u

outDir="$1"
cd "$outDir"

html='<!DOCTYPE html>
<html>
<head>
  <meta charset="utf-8">
  <title>Diagram Test Results</title>
  <style>
    body { font-family: system-ui, sans-serif; margin: 20px; background: #f5f5f5; }
    h1 { color: #333; }
    h2 { color: #666; margin-top: 30px; border-bottom: 1px solid #ddd; padding-bottom: 10px; }
    .diagram { margin-bottom: 40px; background: white; padding: 20px; border-radius: 8px; }
    .diagram-title { font-weight: bold; margin-bottom: 10px; color: #333; }
    .comparison { display: flex; gap: 20px; flex-wrap: wrap; }
    .comparison > div { flex: 1; min-width: 300px; }
    .comparison img { max-width: 100%; height: auto; border: 1px solid #ddd; }
    .label { font-weight: bold; margin-bottom: 5px; color: #666; }
    .test-section { margin-bottom: 50px; }
  </style>
</head>
<body>
  <h1>Diagram Test Results</h1>
'

for testDir in "$outDir"/*/; do
  testName="$(basename "$testDir")"
  if [ -d "$testDir" ]; then
    html+="<div class=\"test-section\"><h2>$testName</h2>"

    shopt -s nullglob
    svg_files=("$testDir"*.svg)
    png_files=("$testDir"*.png)
    shopt -u nullglob

    if [ ${#svg_files[@]} -eq 0 ] && [ ${#png_files[@]} -eq 0 ]; then
      html+="<p>No diagram files found.</p>"
    else
      declare -A svg_by_base
      declare -A svg_cfg_a_by_base
      declare -A svg_cfg_b_by_base
      for f in "${svg_files[@]}"; do
        base="$(basename "${f%.svg}")"
        if [[ "$base" == *.cfg-a ]]; then
          grouped_base="${base%.cfg-a}"
          svg_cfg_a_by_base["$grouped_base"]="$f"
        elif [[ "$base" == *.cfg-b ]]; then
          grouped_base="${base%.cfg-b}"
          svg_cfg_b_by_base["$grouped_base"]="$f"
        else
          svg_by_base["$base"]="$f"
        fi
      done

      declare -A png_by_base
      declare -A png_cfg_a_by_base
      declare -A png_cfg_b_by_base
      declare -A svg_to_png_by_base
      declare -A svg_to_png_2x_by_base
      declare -A svg_to_png_cfg_a_by_base
      declare -A svg_to_png_cfg_b_by_base
      declare -A svg_to_png_cfg_a_2x_by_base
      declare -A svg_to_png_cfg_b_2x_by_base
      for f in "${png_files[@]}"; do
        base="$(basename "${f%.png}")"
        # Check if this is an svg-to-png output file
        if [[ "$base" == *.svg-to-png@2x.stdout ]]; then
          source_base="${base%.svg-to-png@2x.stdout}"
          if [[ "$source_base" == *.cfg-a ]]; then
            grouped_base="${source_base%.cfg-a}"
            svg_to_png_cfg_a_2x_by_base["$grouped_base"]="$f"
          elif [[ "$source_base" == *.cfg-b ]]; then
            grouped_base="${source_base%.cfg-b}"
            svg_to_png_cfg_b_2x_by_base["$grouped_base"]="$f"
          else
            svg_to_png_2x_by_base["$source_base"]="$f"
          fi
        elif [[ "$base" == *.svg-to-png.stdout ]]; then
          source_base="${base%.svg-to-png.stdout}"
          if [[ "$source_base" == *.cfg-a ]]; then
            grouped_base="${source_base%.cfg-a}"
            svg_to_png_cfg_a_by_base["$grouped_base"]="$f"
          elif [[ "$source_base" == *.cfg-b ]]; then
            grouped_base="${source_base%.cfg-b}"
            svg_to_png_cfg_b_by_base["$grouped_base"]="$f"
          else
            svg_to_png_by_base["$source_base"]="$f"
          fi
        else
          # Native PNG
          if [[ "$base" == *.cfg-a ]]; then
            grouped_base="${base%.cfg-a}"
            png_cfg_a_by_base["$grouped_base"]="$f"
          elif [[ "$base" == *.cfg-b ]]; then
            grouped_base="${base%.cfg-b}"
            png_cfg_b_by_base["$grouped_base"]="$f"
          else
            png_by_base["$base"]="$f"
          fi
        fi
      done

      all_bases=()
      declare -A seen_bases
      for base in "${!svg_by_base[@]}" "${!svg_cfg_a_by_base[@]}" "${!svg_cfg_b_by_base[@]}" "${!png_by_base[@]}" "${!png_cfg_a_by_base[@]}" "${!png_cfg_b_by_base[@]}" "${!svg_to_png_by_base[@]}" "${!svg_to_png_2x_by_base[@]}" "${!svg_to_png_cfg_a_by_base[@]}" "${!svg_to_png_cfg_a_2x_by_base[@]}" "${!svg_to_png_cfg_b_by_base[@]}" "${!svg_to_png_cfg_b_2x_by_base[@]}"; do
        if [ -z "${seen_bases[$base]:-}" ]; then
          all_bases+=("$base")
          seen_bases["$base"]=1
        fi
      done

      for base in "${all_bases[@]}"; do
        svg="${svg_by_base[$base]:-}"
        svg_cfg_a="${svg_cfg_a_by_base[$base]:-}"
        svg_cfg_b="${svg_cfg_b_by_base[$base]:-}"
        png="${png_by_base[$base]:-}"
        png_cfg_a="${png_cfg_a_by_base[$base]:-}"
        png_cfg_b="${png_cfg_b_by_base[$base]:-}"
        svg_to_png="${svg_to_png_2x_by_base[$base]:-${svg_to_png_by_base[$base]:-}}"
        svg_to_png_cfg_a="${svg_to_png_cfg_a_2x_by_base[$base]:-${svg_to_png_cfg_a_by_base[$base]:-}}"
        svg_to_png_cfg_b="${svg_to_png_cfg_b_2x_by_base[$base]:-${svg_to_png_cfg_b_by_base[$base]:-}}"
        svg_to_png_is_2x=0
        svg_to_png_cfg_a_is_2x=0
        svg_to_png_cfg_b_is_2x=0
        if [ -n "${svg_to_png_2x_by_base[$base]:-}" ]; then
          svg_to_png_is_2x=1
        fi
        if [ -n "${svg_to_png_cfg_a_2x_by_base[$base]:-}" ]; then
          svg_to_png_cfg_a_is_2x=1
        fi
        if [ -n "${svg_to_png_cfg_b_2x_by_base[$base]:-}" ]; then
          svg_to_png_cfg_b_is_2x=1
        fi

        # Skip if no SVG or PNG at all
        if [ -z "$svg" ] && [ -z "$svg_cfg_a" ] && [ -z "$svg_cfg_b" ] && [ -z "$png" ] && [ -z "$png_cfg_a" ] && [ -z "$png_cfg_b" ] && [ -z "$svg_to_png" ] && [ -z "$svg_to_png_cfg_a" ] && [ -z "$svg_to_png_cfg_b" ]; then
          continue
        fi

        html+="<div class=\"diagram\">"
        html+="<div class=\"diagram-title\">$base</div>"
        html+="<div class=\"comparison\">"

        if [ -n "$svg" ]; then
          html+="<div><div class=\"label\">SVG</div><img src=\"$testName/$(basename "$svg")\" alt=\"SVG\"></div>"
        fi
        if [ -n "$svg_cfg_a" ]; then
          html+="<div><div class=\"label\">SVG (cfg-a)</div><img src=\"$testName/$(basename "$svg_cfg_a")\" alt=\"SVG (cfg-a)\"></div>"
        fi
        if [ -n "$svg_cfg_b" ]; then
          html+="<div><div class=\"label\">SVG (cfg-b)</div><img src=\"$testName/$(basename "$svg_cfg_b")\" alt=\"SVG (cfg-b)\"></div>"
        fi
        if [ -n "$png" ]; then
          html+="<div><div class=\"label\">PNG (native)</div><img src=\"$testName/$(basename "$png")\" alt=\"PNG (native)\"></div>"
        fi
        if [ -n "$png_cfg_a" ]; then
          html+="<div><div class=\"label\">PNG (native, cfg-a)</div><img src=\"$testName/$(basename "$png_cfg_a")\" alt=\"PNG (native, cfg-a)\"></div>"
        fi
        if [ -n "$png_cfg_b" ]; then
          html+="<div><div class=\"label\">PNG (native, cfg-b)</div><img src=\"$testName/$(basename "$png_cfg_b")\" alt=\"PNG (native, cfg-b)\"></div>"
        fi
        if [ -n "$svg_to_png" ]; then
          if [ "$svg_to_png_is_2x" -eq 1 ]; then
            html+="<div><div class=\"label\">PNG (svg-to-png)</div><img src=\"$testName/$(basename "$svg_to_png")\" srcset=\"$testName/$(basename "$svg_to_png") 2x\" alt=\"PNG (svg-to-png)\"></div>"
          else
            html+="<div><div class=\"label\">PNG (svg-to-png)</div><img src=\"$testName/$(basename "$svg_to_png")\" alt=\"PNG (svg-to-png)\"></div>"
          fi
        fi
        if [ -n "$svg_to_png_cfg_a" ]; then
          if [ "$svg_to_png_cfg_a_is_2x" -eq 1 ]; then
            html+="<div><div class=\"label\">PNG (svg-to-png, cfg-a)</div><img src=\"$testName/$(basename "$svg_to_png_cfg_a")\" srcset=\"$testName/$(basename "$svg_to_png_cfg_a") 2x\" alt=\"PNG (svg-to-png, cfg-a)\"></div>"
          else
            html+="<div><div class=\"label\">PNG (svg-to-png, cfg-a)</div><img src=\"$testName/$(basename "$svg_to_png_cfg_a")\" alt=\"PNG (svg-to-png, cfg-a)\"></div>"
          fi
        fi
        if [ -n "$svg_to_png_cfg_b" ]; then
          if [ "$svg_to_png_cfg_b_is_2x" -eq 1 ]; then
            html+="<div><div class=\"label\">PNG (svg-to-png, cfg-b)</div><img src=\"$testName/$(basename "$svg_to_png_cfg_b")\" srcset=\"$testName/$(basename "$svg_to_png_cfg_b") 2x\" alt=\"PNG (svg-to-png, cfg-b)\"></div>"
          else
            html+="<div><div class=\"label\">PNG (svg-to-png, cfg-b)</div><img src=\"$testName/$(basename "$svg_to_png_cfg_b")\" alt=\"PNG (svg-to-png, cfg-b)\"></div>"
          fi
        fi

        html+="</div></div>"
      done
    fi

    html+="</div>"
  fi
done

html+="</body></html>"
echo "$html" > "$outDir/index.html"
