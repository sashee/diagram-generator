import assert from "node:assert/strict";
import {
  assertInlinedFonts,
  assertSuccess,
  parseJson,
  runCli,
  writeArtifact,
} from "./_helpers.mjs";

const payload = [{
  renderer: "plantuml-v1.2025.3",
  format: "svg",
  code: "@startuml\na -> b\n@enduml\n",
}];

const stdin = JSON.stringify(payload);
const result = await runCli({ stdin });

await writeArtifact("font-inline-no-debug.stdin.json", JSON.stringify(payload, null, 2));
await writeArtifact("font-inline-no-debug.stdout.json", result.stdout);
await writeArtifact("font-inline-no-debug.stderr.txt", result.stderr);

assertSuccess(result);
const parsed = parseJson(result.stdout);
assert.equal(parsed.length, 1, "expected one svg output item");
assert.equal(typeof parsed[0].result, "string", "expected svg result string");

const svg = parsed[0].result;
assertInlinedFonts(svg, "debug=false renderer output");
assert(!svg.includes("<!-- svg-font-inliner:"), "expected no svg-font-inliner debug comments when debug=false");

await writeArtifact("font-inline-no-debug.svg", svg);
await writeArtifact("success.txt", "ok\n");
