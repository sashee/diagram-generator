import assert from "node:assert/strict";
import {
  assertSuccess,
  invalidPlantumlCode,
  listRenderers,
  parseJson,
  pickPlantumlRenderer,
  runCli,
  validPlantumlCode,
  writeArtifact,
} from "./_helpers.mjs";

const renderer = await pickPlantumlRenderer();

const allSuccessPayload = [
  { renderer, format: "svg", code: validPlantumlCode },
  { renderer, format: "svg", code: "@startuml\nb -> c\n@enduml\n" },
  { renderer, format: "svg", code: "@startuml\nc -> d\n@enduml\n" },
];
const allSuccessStdin = JSON.stringify(allSuccessPayload);
const allSuccessRes = await runCli({ stdin: allSuccessStdin });
await writeArtifact("all-success.stdin.json", JSON.stringify(allSuccessPayload, null, 2));
await writeArtifact("all-success.stdout.json", allSuccessRes.stdout);
await writeArtifact("all-success.stderr.txt", allSuccessRes.stderr);
assertSuccess(allSuccessRes);

const allSuccessParsed = parseJson(allSuccessRes.stdout);
assert.equal(allSuccessParsed.length, allSuccessPayload.length);
for (const item of allSuccessParsed) {
  const hasResult = Object.hasOwn(item, "result");
  const hasError = Object.hasOwn(item, "error");
  assert.notEqual(hasResult, hasError, "each output item must have exactly one of result/error");
  assert.equal(typeof item.result, "string");
}

const mixedPayload = [
  { renderer, format: "svg", code: validPlantumlCode },
  { renderer, format: "svg", code: invalidPlantumlCode },
  { renderer, format: "svg", code: "@startuml\nx -> y\n@enduml\n" },
];
const mixedStdin = JSON.stringify(mixedPayload);
const mixedRes = await runCli({ stdin: mixedStdin });
await writeArtifact("mixed.stdin.json", JSON.stringify(mixedPayload, null, 2));
await writeArtifact("mixed.stdout.json", mixedRes.stdout);
await writeArtifact("mixed.stderr.txt", mixedRes.stderr);
assertSuccess(mixedRes);

const mixedParsed = parseJson(mixedRes.stdout);
assert.equal(mixedParsed.length, mixedPayload.length);
assert.equal(typeof mixedParsed[0].result, "string");
assert.equal(typeof mixedParsed[1].error, "string");
assert.equal(typeof mixedParsed[2].result, "string");

const singleErrorPayload = [{ renderer, format: "svg", code: invalidPlantumlCode }];
const singleErrorStdin = JSON.stringify(singleErrorPayload);
const singleErrorRes = await runCli({ stdin: singleErrorStdin });
await writeArtifact("single-error.stdin.json", JSON.stringify(singleErrorPayload, null, 2));
await writeArtifact("single-error.stdout.json", singleErrorRes.stdout);
await writeArtifact("single-error.stderr.txt", singleErrorRes.stderr);
assertSuccess(singleErrorRes);

const singleErrorParsed = parseJson(singleErrorRes.stdout);
assert.equal(singleErrorParsed.length, 1);
assert.equal(typeof singleErrorParsed[0].error, "string");
assert(!Object.hasOwn(singleErrorParsed[0], "result"));

const groupedPayload = [
  { renderer, format: "svg", code: "@startuml\na -> b\n@enduml\n" },
  { renderer, format: "svg", code: "@startuml\na -> c\n@enduml\n" },
  { renderer, format: "svg", code: "@startuml\na -> d\n@enduml\n" },
  { renderer, format: "svg", code: "@startuml\na -> e\n@enduml\n" },
];
const groupedStdin = JSON.stringify(groupedPayload);
const groupedRes = await runCli({ stdin: groupedStdin });
await writeArtifact("grouped.stdin.json", JSON.stringify(groupedPayload, null, 2));
await writeArtifact("grouped.stdout.json", groupedRes.stdout);
await writeArtifact("grouped.stderr.txt", groupedRes.stderr);
assertSuccess(groupedRes);

const groupedParsed = parseJson(groupedRes.stdout);
assert.equal(groupedParsed.length, groupedPayload.length);
groupedParsed.forEach((item) => assert.equal(typeof item.result, "string"));
const uniqueResults = new Set(groupedParsed.map((item) => item.result));
assert.equal(uniqueResults.size, groupedPayload.length, "expected unique output per grouped input");

const available = await listRenderers();
const plantuml = available.plantuml?.[0]?.renderer;
const recharts = available.recharts?.[0]?.renderer;
const swirly = available.swirly?.[0]?.renderer;

assert(plantuml, "expected at least one plantuml renderer");
assert(recharts, "expected at least one recharts renderer");
assert(swirly, "expected at least one swirly renderer");

const crossEnginePayload = [
  { renderer: recharts, format: "svg", code: `
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
` },
  { renderer: plantuml, format: "svg", code: validPlantumlCode },
  { renderer: swirly, format: "svg", code: `
-1-2-3-4-5|

> orderedMergeMap

--A--BC--D--E|
A := P1
B := P2
C := P3
D := P4
E := P5
` },
];

const crossEngineStdin = JSON.stringify(crossEnginePayload);
const crossEngineRes = await runCli({ stdin: crossEngineStdin });
await writeArtifact("cross-engine.stdin.json", JSON.stringify(crossEnginePayload, null, 2));
await writeArtifact("cross-engine.stdout.json", crossEngineRes.stdout);
await writeArtifact("cross-engine.stderr.txt", crossEngineRes.stderr);
assertSuccess(crossEngineRes);

const crossEngineParsed = parseJson(crossEngineRes.stdout);
assert.equal(crossEngineParsed.length, crossEnginePayload.length);
crossEngineParsed.forEach((item) => assert.equal(typeof item.result, "string"));
crossEngineParsed.forEach((item, idx) => {
  const svg = item.result;
  assert(svg.includes("<svg"), `cross-engine item ${idx}: expected <svg`);
  assert(svg.includes("</svg>"), `cross-engine item ${idx}: expected </svg>`);
});

await writeArtifact("success.txt", "ok\n");
