import assert from "node:assert/strict";
import {
  assertSuccess,
  listRenderers,
  parseJson,
  runCli,
  runSvgToPng,
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
    {
      name: "salt-security-checklist",
      code: `
@startuml

(*) --> "
{{
salt
{
<b>Security checklist
~~
[] Data encrypted?
~~
}
}}
" as initial

initial -down->[Enable SSE-S3] "
{{
salt
{
<b>Security checklist
~~
[X] Data encrypted?
~~
}
}}
"

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

const assertSingleSuccessfulOutput = ({ item, renderer, format }) => {
  const hasResult = Object.hasOwn(item, "result");
  const hasError = Object.hasOwn(item, "error");
  assert.notEqual(
    hasResult,
    hasError,
    `${renderer}: expected exactly one of result/error for ${format}, got ${JSON.stringify(item)}`,
  );

  if (hasError) {
    const errorMessage = typeof item.error === "string" ? item.error : JSON.stringify(item.error);
    assert.fail(`${renderer}: render ${format} failed: ${errorMessage}`);
  }

  assert.equal(typeof item.result, "string", `${renderer}: expected ${format} result string`);
  return item.result;
};

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

const runWithConcurrency = async (items, limit, worker) => {
  const workerCount = Math.min(limit, items.length);
  let nextIndex = 0;

  const workers = Array.from({ length: workerCount }, async () => {
    while (true) {
      const idx = nextIndex;
      nextIndex += 1;
      if (idx >= items.length) {
        return;
      }
      await worker(items[idx]);
    }
  });

  await Promise.all(workers);
};

await runWithConcurrency(jobs, 5, async ({ renderer, name, code, format }) => {
  const startedAt = Date.now();
  console.log(`Starting render, ${renderer}, ${name}, ${format}`);
  const context = `${renderer}/${name}/${format}`;
  try {
    const payload = [{ renderer, format, code }];
    const stdin = JSON.stringify(payload);

    const runCliStart = Date.now();
    const result = await runCli({ stdin });
    const runCliMs = Date.now() - runCliStart;

    const prefix = sanitize(`${renderer}/${name}`);

    if (format === "png") {
      const writeArtifactsStart = Date.now();
      await writeArtifact(`${prefix}.png.stdin.json`, JSON.stringify(payload, null, 2));
      await writeArtifact(`${prefix}.png.stdout.json`, result.stdout);
      await writeArtifact(`${prefix}.png.stderr.txt`, result.stderr);
      const writeArtifactsMs = Date.now() - writeArtifactsStart;

      const assertStart = Date.now();
      assertSuccess(result);
      const parsed = parseJson(result.stdout);
      assert.equal(parsed.length, 1, `${renderer}: expected one png output item`);
      const pngBase64 = assertSingleSuccessfulOutput({ item: parsed[0], renderer, format: "png" });
      const pngBytes = Buffer.from(pngBase64, "base64");
      assert(pngBytes.length > 8, `${renderer}: decoded png should not be empty`);
      const signature = pngBytes.subarray(0, 8).toString("hex");
      assert.equal(signature, "89504e470d0a1a0a", `${renderer}: output does not look like png`);
      await writeBytesArtifact(`${prefix}.png`, pngBytes);
      const assertMs = Date.now() - assertStart;

      console.log(
        `Ending render, ${renderer}, ${name}, ${format}, total=${Date.now() - startedAt}ms runCli=${runCliMs}ms writeArtifacts=${writeArtifactsMs}ms parseAssert=${assertMs}ms`,
      );
      return;
    }

    const writeArtifactsStart = Date.now();
    await writeArtifact(`${prefix}.stdin.json`, JSON.stringify(payload, null, 2));
    await writeArtifact(`${prefix}.stdout.json`, result.stdout);
    await writeArtifact(`${prefix}.stderr.txt`, result.stderr);
    const writeArtifactsMs = Date.now() - writeArtifactsStart;

    const parseAssertStart = Date.now();
    assertSuccess(result);
    const parsed = parseJson(result.stdout);
    assert.equal(parsed.length, 1, `${renderer}: expected one svg output item`);
    const svg = assertSingleSuccessfulOutput({ item: parsed[0], renderer, format: "svg" });
    assert(svg.includes("<svg"), `${renderer}: expected <svg in output`);
    assert(svg.includes("</svg>"), `${renderer}: expected </svg> in output`);
    await writeArtifact(`${prefix}.svg`, svg);
    const parseAssertMs = Date.now() - parseAssertStart;

    const interopStart = Date.now();
    const interopResult = await runSvgToPng({ stdin: svg });
    const interopMs = Date.now() - interopStart;

    const interopArtifactsStart = Date.now();
    await writeArtifact(`${prefix}.svg-to-png.stderr.txt`, interopResult.stderr);
    await writeBytesArtifact(`${prefix}.svg-to-png.stdout.png`, interopResult.stdout);
    const interopArtifactsMs = Date.now() - interopArtifactsStart;

    const interopAssertStart = Date.now();
    assertSuccess({ ...interopResult, stdout: "" });
    assert(interopResult.stdout.length > 8, `${renderer}: svg-to-png decoded png should not be empty`);
    const interopSignature = interopResult.stdout.subarray(0, 8).toString("hex");
    assert.equal(interopSignature, "89504e470d0a1a0a", `${renderer}: svg output is not valid svg-to-png input`);
    const interopAssertMs = Date.now() - interopAssertStart;

    console.log(
      `Ending render, ${renderer}, ${name}, ${format}, total=${Date.now() - startedAt}ms runCli=${runCliMs}ms writeArtifacts=${writeArtifactsMs}ms parseAssert=${parseAssertMs}ms svgToPng=${interopMs}ms interopArtifacts=${interopArtifactsMs}ms interopAssert=${interopAssertMs}ms`,
    );
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    throw new Error(`render-matrix job failed (${context}): ${message}`);
  }
});

await writeArtifact("success.txt", "ok\n");
