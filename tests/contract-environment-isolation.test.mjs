import assert from "node:assert/strict";
import {
  assertSuccess,
  listRenderers,
  parseJson,
  runCli,
  writeArtifact,
} from "./_helpers.mjs";

// Renderers run user code (recharts evaluates it), so the caller's environment must not reach them:
// a secret in it would end up in the rendered diagram
const secret = "secret-from-the-callers-environment";
process.env.DIAGRAM_GENERATOR_TEST_SECRET = secret;

const available = await listRenderers();
const [recharts] = available.recharts ?? [];
assert(recharts, "expected a recharts renderer");

const code = `
const data = [0, 1, 2, 3].map((r) => ({ia: 1.25 + r * 1}));
<LineChart data={data} width={400} height={300}>
  <XAxis label={String(process.env.DIAGRAM_GENERATOR_TEST_SECRET)} height={60}/>
  <Line type="monotone" dataKey="ia" stroke="red"/>
</LineChart>
`;
const payload = [{ renderer: recharts.renderer, format: "svg", code }];
const result = await runCli({ stdin: JSON.stringify(payload) });

await writeArtifact("stdout.json", result.stdout);
await writeArtifact("stderr.txt", result.stderr);

assertSuccess(result);
const [item] = parseJson(result.stdout);
assert.equal(typeof item.result, "string", `expected svg result, got ${JSON.stringify(item)}`);
assert(item.result.includes("undefined"), "expected the label to show the missing variable as undefined");
assert(!item.result.includes(secret), "the caller's environment reached the renderer");

await writeArtifact("success.txt", "ok\n");
