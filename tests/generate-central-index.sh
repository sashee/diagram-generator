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
      for f in "${svg_files[@]}"; do
        base="$(basename "${f%.svg}")"
        svg_by_base["$base"]="$f"
      done

      declare -A png_by_base
      declare -A svg_to_png_by_base
      for f in "${png_files[@]}"; do
        base="$(basename "${f%.png}")"
        # Check if this is an svg-to-png output file
        if [[ "$base" == *.svg-to-png.stdout ]]; then
          source_base="${base%.svg-to-png.stdout}"
          svg_to_png_by_base["$source_base"]="$f"
        else
          # Native PNG
          png_by_base["$base"]="$f"
        fi
      done

      all_bases=()
      declare -A seen_bases
      for base in "${!svg_by_base[@]}" "${!png_by_base[@]}" "${!svg_to_png_by_base[@]}"; do
        if [ -z "${seen_bases[$base]:-}" ]; then
          all_bases+=("$base")
          seen_bases["$base"]=1
        fi
      done

      for base in "${all_bases[@]}"; do
        svg="${svg_by_base[$base]:-}"
        png="${png_by_base[$base]:-}"
        svg_to_png="${svg_to_png_by_base[$base]:-}"

        # Skip if no SVG or PNG at all
        if [ -z "$svg" ] && [ -z "$png" ] && [ -z "$svg_to_png" ]; then
          continue
        fi

        html+="<div class=\"diagram\">"
        html+="<div class=\"diagram-title\">$base</div>"
        html+="<div class=\"comparison\">"

        if [ -n "$svg" ]; then
          html+="<div><div class=\"label\">SVG</div><img src=\"$testName/$(basename "$svg")\" alt=\"SVG\"></div>"
        fi
        if [ -n "$png" ]; then
          html+="<div><div class=\"label\">PNG (native)</div><img src=\"$testName/$(basename "$png")\" alt=\"PNG (native)\"></div>"
        fi
        if [ -n "$svg_to_png" ]; then
          html+="<div><div class=\"label\">PNG (svg-to-png)</div><img src=\"$testName/$(basename "$svg_to_png")\" alt=\"PNG (svg-to-png)\"></div>"
        fi

        html+="</div></div>"
      done
    fi

    html+="</div>"
  fi
done

html+="</body></html>"
echo "$html" > "$outDir/index.html"
