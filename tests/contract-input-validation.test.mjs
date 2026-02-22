import assert from "node:assert/strict";
import {
  assertFailure,
  assertSuccess,
  parseJson,
  pickPlantumlRenderer,
  runCli,
  writeArtifact,
} from "./_helpers.mjs";

const invalidCases = [
  { name: "invalid-json", stdin: "not-json" },
  { name: "json-object", stdin: JSON.stringify({ renderer: "x" }) },
  {
    name: "missing-renderer",
    stdin: JSON.stringify([{ format: "svg", code: "@startuml\na->b\n@enduml" }]),
  },
  {
    name: "missing-format",
    stdin: JSON.stringify([{ renderer: "plantuml-v1.2025.3", code: "@startuml\na->b\n@enduml" }]),
  },
  {
    name: "missing-code",
    stdin: JSON.stringify([{ renderer: "plantuml-v1.2025.3", format: "svg" }]),
  },
  {
    name: "wrong-types",
    stdin: JSON.stringify([{ renderer: 123, format: "svg", code: true }]),
  },
  {
    name: "unsupported-renderer",
    stdin: JSON.stringify([{ renderer: "plantuml-v0.0.0", format: "svg", code: "@startuml\na -> b\n@enduml\n" }]),
  },
];

for (const c of invalidCases) {
  const res = await runCli({ stdin: c.stdin });
  await writeArtifact(`${c.name}.stdin.txt`, c.stdin);
  await writeArtifact(`${c.name}.stdout.txt`, res.stdout);
  await writeArtifact(`${c.name}.stderr.txt`, res.stderr);
  assertFailure(res);
  assert(res.stderr.length > 0, `${c.name}: expected stderr output`);
}

const knownRenderer = await pickPlantumlRenderer();
const unsupportedFormatPayload = [{ renderer: knownRenderer, format: "pdf", code: "@startuml\na -> b\n@enduml\n" }];
const unsupportedFormatStdin = JSON.stringify(unsupportedFormatPayload);
const unsupportedFormatRes = await runCli({ stdin: unsupportedFormatStdin });
await writeArtifact("unsupported-format.stdin.json", JSON.stringify(unsupportedFormatPayload, null, 2));
await writeArtifact("unsupported-format.stdout.txt", unsupportedFormatRes.stdout);
await writeArtifact("unsupported-format.stderr.txt", unsupportedFormatRes.stderr);
assertFailure(unsupportedFormatRes);
assert(unsupportedFormatRes.stderr.length > 0, "unsupported-format: expected stderr output");

const emptyInputRes = await runCli({ stdin: "[]" });
await writeArtifact("empty-input.stdin.json", "[]\n");
await writeArtifact("empty-input.stdout.json", emptyInputRes.stdout);
await writeArtifact("empty-input.stderr.txt", emptyInputRes.stderr);
assertSuccess(emptyInputRes);
assert.deepEqual(parseJson(emptyInputRes.stdout), []);

await writeArtifact("success.txt", "ok\n");
