import assert from "node:assert/strict";
import crypto from "node:crypto";
import {
  assertInlinedFonts,
  assertSuccess,
  diagramGeneratorBinA,
  diagramGeneratorBinB,
  embeddedFontHashes,
  listRenderersWithBin,
  runCliWithBin,
  runSvgToPng,
  runSvgFontInlinerWithBin,
  svgFontInlinerBinA,
  svgFontInlinerBinB,
  writeArtifact,
  writeBytesArtifact,
} from "./_helpers.mjs";

assert(diagramGeneratorBinA, "DIAGRAM_GENERATOR_BIN_A is required");
assert(diagramGeneratorBinB, "DIAGRAM_GENERATOR_BIN_B is required");
assert(svgFontInlinerBinA, "SVG_FONT_INLINER_BIN_A is required");
assert(svgFontInlinerBinB, "SVG_FONT_INLINER_BIN_B is required");

const plantumlCode = "@startuml\nskinparam defaultFontName sans-serif\nskinparam defaultFontSize 30\na -> b : font profile check\n@enduml\n";
const inlinerInputSvg =
  "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"420\" height=\"120\"><text x=\"10\" y=\"45\" font-family=\"sans-serif\" font-size=\"32\">font profile check</text></svg>";

const pngSignatureHex = "89504e470d0a1a0a";

const hashBytes = (bytes) => crypto.createHash("sha256").update(bytes).digest("hex");

const renderDeterministicPng = async ({ baseName, svg, label }) => {
  const args = ["--zoom", "2"];
  const run1 = await runSvgToPng({ args, stdin: svg });
  const run2 = await runSvgToPng({ args, stdin: svg });

  assertSuccess({ ...run1, stdout: "" });
  assertSuccess({ ...run2, stdout: "" });
  assert(run1.stdout.length > 8, `${label}: expected non-empty PNG output (run1)`);
  assert(run2.stdout.length > 8, `${label}: expected non-empty PNG output (run2)`);
  assert.equal(run1.stdout.subarray(0, 8).toString("hex"), pngSignatureHex, `${label}: invalid PNG signature (run1)`);
  assert.equal(run2.stdout.subarray(0, 8).toString("hex"), pngSignatureHex, `${label}: invalid PNG signature (run2)`);

  const hash1 = hashBytes(run1.stdout);
  const hash2 = hashBytes(run2.stdout);
  assert.equal(hash1, hash2, `${label}: expected deterministic png hash for identical input`);

  await writeBytesArtifact(`${baseName}.svg-to-png@2x.run1.stdout.png`, run1.stdout);
  await writeBytesArtifact(`${baseName}.svg-to-png@2x.run2.stdout.png`, run2.stdout);
  await writeArtifact(`${baseName}.svg-to-png@2x.run1.stderr.txt`, run1.stderr);
  await writeArtifact(`${baseName}.svg-to-png@2x.run2.stderr.txt`, run2.stderr);

  await writeBytesArtifact(`${baseName}.svg-to-png@2x.stdout.png`, run1.stdout);
  await writeArtifact(`${baseName}.svg-to-png@2x.stderr.txt`, run1.stderr);
  await writeArtifact(`${baseName}.svg-to-png@2x.hash.txt`, `${hash1}\n`);

  return hash1;
};

const listA = await listRenderersWithBin(diagramGeneratorBinA);
const listB = await listRenderersWithBin(diagramGeneratorBinB);
const plantumlRendererA = listA.plantuml?.[0]?.renderer;
const plantumlRendererB = listB.plantuml?.[0]?.renderer;

assert(plantumlRendererA, "expected at least one plantuml renderer for profile A");
assert(plantumlRendererB, "expected at least one plantuml renderer for profile B");

const diagramPayloadA = JSON.stringify([{ renderer: plantumlRendererA, format: "svg", code: plantumlCode }]);
const diagramPayloadB = JSON.stringify([{ renderer: plantumlRendererB, format: "svg", code: plantumlCode }]);

const diagramA = await runCliWithBin({
  bin: diagramGeneratorBinA,
  stdin: diagramPayloadA,
});
const diagramB = await runCliWithBin({
  bin: diagramGeneratorBinB,
  stdin: diagramPayloadB,
});

assertSuccess(diagramA);
assertSuccess(diagramB);

const diagramOutA = JSON.parse(diagramA.stdout);
const diagramOutB = JSON.parse(diagramB.stdout);

assert.equal(diagramOutA.length, 1, "diagram-generator profile A: expected one output item");
assert.equal(diagramOutB.length, 1, "diagram-generator profile B: expected one output item");

const diagramSvgA = diagramOutA[0]?.result;
const diagramSvgB = diagramOutB[0]?.result;
assert.equal(typeof diagramSvgA, "string", "diagram-generator profile A: expected SVG string");
assert.equal(typeof diagramSvgB, "string", "diagram-generator profile B: expected SVG string");
assertInlinedFonts(diagramSvgA, "diagram-generator profile A");
assertInlinedFonts(diagramSvgB, "diagram-generator profile B");

const diagramHashesA = embeddedFontHashes(diagramSvgA);
const diagramHashesB = embeddedFontHashes(diagramSvgB);

await writeArtifact("fontconfig-wiring.diagram-generator.cfg-a.svg", diagramSvgA);
await writeArtifact("fontconfig-wiring.diagram-generator.cfg-b.svg", diagramSvgB);
await writeArtifact("fontconfig-wiring.diagram-generator.cfg-a.hashes.json", `${JSON.stringify(diagramHashesA, null, 2)}\n`);
await writeArtifact("fontconfig-wiring.diagram-generator.cfg-b.hashes.json", `${JSON.stringify(diagramHashesB, null, 2)}\n`);
await writeArtifact("fontconfig-wiring.diagram-generator.cfg-a.stderr.txt", diagramA.stderr);
await writeArtifact("fontconfig-wiring.diagram-generator.cfg-b.stderr.txt", diagramB.stderr);

assert(diagramHashesA.length > 0, "diagram-generator profile A: expected embedded font hashes");
assert(diagramHashesB.length > 0, "diagram-generator profile B: expected embedded font hashes");

const diagramPngHashA = await renderDeterministicPng({
  baseName: "fontconfig-wiring.diagram-generator.cfg-a",
  svg: diagramSvgA,
  label: "diagram-generator profile A",
});
const diagramPngHashB = await renderDeterministicPng({
  baseName: "fontconfig-wiring.diagram-generator.cfg-b",
  svg: diagramSvgB,
  label: "diagram-generator profile B",
});
assert.notEqual(diagramPngHashA, diagramPngHashB, "diagram-generator should render different PNG output for profile A vs B");

const inlinerA = await runSvgFontInlinerWithBin({
  bin: svgFontInlinerBinA,
  stdin: inlinerInputSvg,
});
const inlinerB = await runSvgFontInlinerWithBin({
  bin: svgFontInlinerBinB,
  stdin: inlinerInputSvg,
});

assertSuccess(inlinerA);
assertSuccess(inlinerB);
assertInlinedFonts(inlinerA.stdout, "svg-font-inliner profile A");
assertInlinedFonts(inlinerB.stdout, "svg-font-inliner profile B");

const inlinerHashesA = embeddedFontHashes(inlinerA.stdout);
const inlinerHashesB = embeddedFontHashes(inlinerB.stdout);

await writeArtifact("fontconfig-wiring.svg-font-inliner.cfg-a.svg", inlinerA.stdout);
await writeArtifact("fontconfig-wiring.svg-font-inliner.cfg-b.svg", inlinerB.stdout);
await writeArtifact("fontconfig-wiring.svg-font-inliner.cfg-a.hashes.json", `${JSON.stringify(inlinerHashesA, null, 2)}\n`);
await writeArtifact("fontconfig-wiring.svg-font-inliner.cfg-b.hashes.json", `${JSON.stringify(inlinerHashesB, null, 2)}\n`);
await writeArtifact("fontconfig-wiring.svg-font-inliner.cfg-a.stderr.txt", inlinerA.stderr);
await writeArtifact("fontconfig-wiring.svg-font-inliner.cfg-b.stderr.txt", inlinerB.stderr);

assert(inlinerHashesA.length > 0, "svg-font-inliner profile A: expected embedded font hashes");
assert(inlinerHashesB.length > 0, "svg-font-inliner profile B: expected embedded font hashes");

const inlinerPngHashA = await renderDeterministicPng({
  baseName: "fontconfig-wiring.svg-font-inliner.cfg-a",
  svg: inlinerA.stdout,
  label: "svg-font-inliner profile A",
});
const inlinerPngHashB = await renderDeterministicPng({
  baseName: "fontconfig-wiring.svg-font-inliner.cfg-b",
  svg: inlinerB.stdout,
  label: "svg-font-inliner profile B",
});
assert.notEqual(inlinerPngHashA, inlinerPngHashB, "svg-font-inliner should render different PNG output for profile A vs B");

await writeArtifact("success.txt", "ok\n");
