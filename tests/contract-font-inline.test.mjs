import assert from "node:assert/strict";
import {
  assertInlinedFonts,
  assertSuccess,
  listRenderers,
  parseJson,
  runCli,
  writeArtifact,
} from "./_helpers.mjs";

const sampleCodeByEngine = {
  plantuml: "@startuml\na -> b\n@enduml\n",
  recharts: `
const data = [0, 1, 2, 3].map((r) => ({ia: 1.25 + r * 1}));

<LineChart data={data} width={400} height={300}
  margin={{ top: 10, right: 10, left: 20, bottom: 20 }}>
  <CartesianGrid strokeDasharray="3 3" />
  <YAxis width={40}>
    <Label angle={-90} position="insideLeft">$/month/GB</Label>
  </YAxis>
  <XAxis label="Retrievals/month" position="insideBottom" height={60}/>
  <Line type="monotone" dataKey="ia" stroke="red"/>
</LineChart>
`,
  swirly: `
-1-2-3-4-5|

> orderedMergeMap

--A--BC--D--E|
A := P1
B := P2
C := P3
D := P4
E := P5
`,
};

const sanitize = (str) => str.replace(/[^a-zA-Z0-9._-]/g, "_");

const available = await listRenderers();

for (const [engine, renderers] of Object.entries(available)) {
  const sampleCode = sampleCodeByEngine[engine];
  if (!sampleCode) {
    continue;
  }

  for (const { renderer, formats } of renderers) {
    if (!formats.includes("svg")) {
      continue;
    }

    const payload = [{ renderer, format: "svg", code: sampleCode }];
    const stdin = JSON.stringify(payload);
    const result = await runCli({ stdin });
    const prefix = sanitize(`${engine}/${renderer}/font-inline`);

    await writeArtifact(`${prefix}.stdin.json`, JSON.stringify(payload, null, 2));
    await writeArtifact(`${prefix}.stdout.json`, result.stdout);
    await writeArtifact(`${prefix}.stderr.txt`, result.stderr);

    assertSuccess(result);
    const parsed = parseJson(result.stdout);
    assert.equal(parsed.length, 1, `${renderer}: expected one svg output item`);
    assert.equal(typeof parsed[0].result, "string", `${renderer}: expected svg result string`);

    const svg = parsed[0].result;
    assertInlinedFonts(svg, `${renderer}`);
    assert(!svg.includes("<!-- svg-font-inliner:"), `${renderer}: expected no svg-font-inliner debug comment`);
    await writeArtifact(`${prefix}.svg`, svg);
  }
}

await writeArtifact("success.txt", "ok\n");
