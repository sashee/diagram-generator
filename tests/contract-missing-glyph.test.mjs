import assert from "node:assert/strict";
import {
  assertSuccess,
  listRenderers,
  parseJson,
  runCli,
  writeArtifact,
} from "./_helpers.mjs";

// The test fonts have no CJK glyphs. The embedded fonts get synthetic names, so a browser can't fall back to a system
// font either: instead of an SVG showing a missing-glyph box, the item fails and names the character.
const code = '@startuml\nrectangle "a 漢 b"\n@enduml\n';

const available = await listRenderers();
const plantumlRenderers = available.plantuml ?? [];
assert(plantumlRenderers.length > 0, "expected at least one plantuml renderer");

for (const { renderer } of plantumlRenderers) {
  const result = await runCli({ stdin: JSON.stringify([{ renderer, format: "svg", code }]) });
  await writeArtifact(`${renderer}.stdout.json`, result.stdout);
  await writeArtifact(`${renderer}.stderr.txt`, result.stderr);

  assertSuccess(result);
  const [item] = parseJson(result.stdout);
  assert.equal(typeof item.error, "string", `${renderer}: expected an error item, got ${JSON.stringify(item)}`);
  assert(item.error.includes("no font provides U+6F22"), `${renderer}: unexpected error: ${item.error}`);
}

await writeArtifact("success.txt", "ok\n");
