import assert from "node:assert/strict";
import {
  assertSuccess,
  listRenderers,
  parseJson,
  runCli,
  writeArtifact,
  writeBytesArtifact,
} from "./_helpers.mjs";

const sampleCodeByEngine = {
  plantuml: [
    {
      name: "table",
      code: `
@startuml
digraph a {
node [shape=plain,penwidth="0",fontname = "monospace"]
Table [label=<
<table border="0" cellborder="1" cellspacing="0">
<tr><td colspan="4"><b>Users table</b></td></tr>
<tr>
<td bgcolor="#f0f8ff"><i>ID</i></td>
<td bgcolor="#f0f8ff"><i>email</i></td>
</tr>
<tr><td>user1</td>
<td port="0">test@example.com</td>
</tr>
<tr><td bgcolor="#90EE90">user2</td>
<td port="0" bgcolor="#90EE90">other@example.com</td>
</tr>
</table>>];

Counts [label=<
<table border="0" cellborder="1" cellspacing="0">
<tr><td colspan="4"><b>Counts table</b></td></tr>
<tr>
<td bgcolor="#f0f8ff"><i>type (PK)</i></td>
<td bgcolor="#f0f8ff"><i>count</i></td>
</tr>
<tr><td port="0">users</td>
<td bgcolor="#90EE90"><s>1</s><br/>2</td>
</tr>
</table>>];
}

@enduml
`,
    },
    {
      name: "simple-arrow",
      code: `
@startuml
a -> b

@enduml
`,
    },
    {
      name: "doctors-map",
      code: `
@startuml
caption Both doctors are removed from on-call\\nbecause the two transactions change different rows

map Doctors {
 doctor1 => on-call
 doctor2 => on-call
}

map T1.Doctors {
 doctor1 => reserve
 doctor2 => on-call
}

map T2.Doctors {
 doctor1 => on-call
 doctor2 => reserve
}

map After.Doctors {
 doctor1 => reserve
 doctor2 => reserve
}

Doctors --> T1.Doctors: length(on-call) > 1
Doctors --> T2.Doctors: length(on-call) > 1
T1.Doctors --> After.Doctors
T2.Doctors --> After.Doctors

@enduml
`,
    },
    {
      name: "listfonts",
      code: `
@startuml
listfonts
@enduml
`,
    },
    {
      name: "awslib",
      code: `
@startuml
caption Direct access
skinparam linetype polyline

!include <awslib14/AWSCommon>
!include <awslib14/AWSSimplified>
!include <awslib14/SecurityIdentityCompliance/IdentityAccessManagementRole>
!include <awslib14/SecurityIdentityCompliance/IdentityAccessManagementTemporarySecurityCredential>
!include <awslib14/ApplicationIntegration/APIGateway>
!include <awslib14/Storage/SimpleStorageServiceBucket>
!include <awslib14/Compute/EC2>

actor "Visitor" as visitor
EC2(ec2, "Servers", "")
SimpleStorageServiceBucket(s3, "Static assets", "")
APIGateway(api, "API", "")

visitor --> ec2: 10.0.0.1
visitor --> s3: example.com
visitor --> api: ...execute-api.amazonaws.com
@enduml
`,
    },
  ],
  recharts: [
    {
      name: "linechart",
      code: `
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
    },
  ],
  swirly: [
    {
      name: "ordered-merge-map",
      code: `
-1-2-3-4-5|

> orderedMergeMap

--A--BC--D--E|
A := P1
B := P2
C := P3
D := P4
E := P5
`,
    },
  ],
};

const sanitize = (str) => str.replace(/[^a-zA-Z0-9._-]/g, "_");

const available = await listRenderers();

const plantumlRenderers = available.plantuml ?? [];
const rechartsRenderers = available.recharts ?? [];
const swirlyRenderers = available.swirly ?? [];

assert(plantumlRenderers.length > 0, "expected at least one plantuml renderer");
assert(rechartsRenderers.length > 0, "expected at least one recharts renderer");
assert(swirlyRenderers.length > 0, "expected at least one swirly renderer");

await writeArtifact("plantuml-renderers.json", JSON.stringify(plantumlRenderers, null, 2));
await writeArtifact("recharts-renderers.json", JSON.stringify(rechartsRenderers, null, 2));
await writeArtifact("swirly-renderers.json", JSON.stringify(swirlyRenderers, null, 2));

const jobs = [];

for (const { renderer, formats } of plantumlRenderers) {
  assert(formats.includes("svg"), `${renderer}: expected svg support`);
  assert(formats.includes("png"), `${renderer}: expected png support`);
  for (const { name, code } of sampleCodeByEngine.plantuml) {
    jobs.push({ renderer, name, code, format: "svg" });
    jobs.push({ renderer, name, code, format: "png" });
  }
}

for (const { renderer, formats } of rechartsRenderers) {
  assert(formats.includes("svg"), `${renderer}: expected svg support`);
  for (const { name, code } of sampleCodeByEngine.recharts) {
    jobs.push({ renderer, name, code, format: "svg" });
  }
}

for (const { renderer, formats } of swirlyRenderers) {
  assert(formats.includes("svg"), `${renderer}: expected svg support`);
  for (const { name, code } of sampleCodeByEngine.swirly) {
    jobs.push({ renderer, name, code, format: "svg" });
  }
}

jobs.sort((a, b) => {
  if (a.renderer !== b.renderer) return a.renderer.localeCompare(b.renderer);
  if (a.name !== b.name) return a.name.localeCompare(b.name);
  return a.format.localeCompare(b.format);
});

await Promise.all(jobs.map(async ({ renderer, name, code, format }) => {
  const context = `${renderer}/${name}/${format}`;
  try {
    const payload = [{ renderer, format, code }];
    const stdin = JSON.stringify(payload);
    const result = await runCli({ stdin });
    const prefix = sanitize(`${renderer}/${name}`);

    if (format === "png") {
      await writeArtifact(`${prefix}.png.stdin.json`, JSON.stringify(payload, null, 2));
      await writeArtifact(`${prefix}.png.stdout.json`, result.stdout);
      await writeArtifact(`${prefix}.png.stderr.txt`, result.stderr);

      assertSuccess(result);
      const parsed = parseJson(result.stdout);
      assert.equal(parsed.length, 1, `${renderer}: expected one png output item`);
      assert.equal(typeof parsed[0].result, "string", `${renderer}: expected png result string`);
      const pngBytes = Buffer.from(parsed[0].result, "base64");
      assert(pngBytes.length > 8, `${renderer}: decoded png should not be empty`);
      const signature = pngBytes.subarray(0, 8).toString("hex");
      assert.equal(signature, "89504e470d0a1a0a", `${renderer}: output does not look like png`);
      await writeBytesArtifact(`${prefix}.png`, pngBytes);
      return;
    }

    await writeArtifact(`${prefix}.stdin.json`, JSON.stringify(payload, null, 2));
    await writeArtifact(`${prefix}.stdout.json`, result.stdout);
    await writeArtifact(`${prefix}.stderr.txt`, result.stderr);

    assertSuccess(result);
    const parsed = parseJson(result.stdout);
    assert.equal(parsed.length, 1, `${renderer}: expected one svg output item`);
    assert.equal(typeof parsed[0].result, "string", `${renderer}: expected svg result string`);
    const svg = parsed[0].result;
    assert(svg.includes("<svg"), `${renderer}: expected <svg in output`);
    assert(svg.includes("</svg>"), `${renderer}: expected </svg> in output`);
    await writeArtifact(`${prefix}.svg`, svg);
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    throw new Error(`render-matrix job failed (${context}): ${message}`);
  }
}));

await writeArtifact("success.txt", "ok\n");
