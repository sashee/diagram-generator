import assert from "node:assert/strict";
import {
  assertSuccess,
  listRenderers,
  parseJson,
  runCli,
  writeArtifact,
} from "./_helpers.mjs";

// PlantUML passes the SVG of DOT diagrams through from Graphviz, including javascript: links (the reason 1.2026
// suppresses it by default). The output must not contain them, as diagrams may be embedded into web pages.
const code = `
@startdot
digraph a {
  a [URL="javascript:alert(1)", label="click"];
  b [label=<<table><tr><td href="javascript:alert(2)">cell</td></tr></table>>, shape=plain];
  a -> b [URL="JaVaScRiPt:alert(3)"];
}
@enddot
`;

const available = await listRenderers();
const plantumlRenderers = available.plantuml ?? [];
assert(plantumlRenderers.length > 0, "expected at least one plantuml renderer");

for (const { renderer } of plantumlRenderers) {
  const payload = [{ renderer, format: "svg", code }];
  const result = await runCli({ stdin: JSON.stringify(payload) });

  await writeArtifact(`${renderer}.stdout.json`, result.stdout);
  await writeArtifact(`${renderer}.stderr.txt`, result.stderr);

  assertSuccess(result);
  const [item] = parseJson(result.stdout);
  assert.equal(typeof item.result, "string", `${renderer}: expected svg result, got ${JSON.stringify(item)}`);

  const svg = item.result;
  await writeArtifact(`${renderer}.svg`, svg);
  assert(svg.includes("click") && svg.includes("cell"), `${renderer}: expected the rendered DOT diagram`);

  const markup = svg.replace(/url\(data:[^)]*\)/g, "url(data:)");
  assert(!/javascript/i.test(markup), `${renderer}: javascript: link survived in the svg`);
  assert(!/<a[\s>]/.test(markup), `${renderer}: link element survived in the svg`);
}

await writeArtifact("success.txt", "ok\n");
