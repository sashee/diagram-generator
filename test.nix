let
  nixpkgs = fetchTarball "https://github.com/NixOS/nixpkgs/tarball/nixos-25.05";
  pkgs = import nixpkgs { config = {}; overlays = []; };

	fontconfig = (import ./tests/default-fontconfig.nix {inherit pkgs;});

	diagram_generator = (import ./default.nix {inherit pkgs fontconfig;});

	pngandsvgtest = file: pkgs.runCommand (builtins.baseNameOf file) {} ''
mkdir $out

${pkgs.nodejs_latest}/bin/node ${file} | \
	${pkgs.jq}/bin/jq '[.[] | .format = "png"]' | \
	${diagram_generator}/bin/diagram-generator | \
	${pkgs.jq}/bin/jq -r '.[0].result' | \
	${pkgs.coreutils}/bin/base64 -d > \
	$out/${builtins.baseNameOf file}.png

${pkgs.nodejs_latest}/bin/node ${file} | \
	${pkgs.jq}/bin/jq '[.[] | .format = "svg"]' | \
	${diagram_generator}/bin/diagram-generator | \
	${pkgs.jq}/bin/jq -r '.[0].result' > \
	$out/${builtins.baseNameOf file}.svg
	'';
	svgtest = file: pkgs.runCommand (builtins.baseNameOf file) {} ''
mkdir $out

${pkgs.nodejs_latest}/bin/node ${file} | \
	${pkgs.jq}/bin/jq '[.[] | .format = "svg"]' | \
	${diagram_generator}/bin/diagram-generator | \
	${pkgs.jq}/bin/jq -r '.[0].result' > \
	$out/${builtins.baseNameOf file}.svg
	'';

	errortest = pkgs.runCommand "error.json" {} ''
mkdir -p $out

${pkgs.writeScriptBin "script" ''
#!${pkgs.nodejs_latest}/bin/node
import child_process from "node:child_process";
import stream from "node:stream";
import util from "node:util";
import assert from "node:assert/strict";

const prom = util.promisify(child_process.execFile)("${diagram_generator}/bin/diagram-generator");
const stdinStream = new stream.Readable();
stdinStream.push(JSON.stringify([
	{renderer: "plantuml-v1.2025.3", format: "svg", code: `@startuml
actor a
acctor b
	@enduml`},
]));
stdinStream.push(null);
stdinStream.pipe(prom.child.stdin);
const res = await prom;
const result = JSON.parse(res.stdout)[0];
assert(result.error, "result should have an error, ''${JSON.stringify(result, undefined, 4)}");
console.log(result)
''}/bin/script > $out/error.json
	'';

	multierrortest = pkgs.runCommand "multierror.json" {} ''
mkdir -p $out

${pkgs.writeScriptBin "script" ''
#!${pkgs.nodejs_latest}/bin/node
import child_process from "node:child_process";
import stream from "node:stream";
import util from "node:util";
import assert from "node:assert/strict";

const prom = util.promisify(child_process.execFile)("${diagram_generator}/bin/diagram-generator");
const stdinStream = new stream.Readable();
stdinStream.push(JSON.stringify([
	{renderer: "plantuml-v1.2025.3", format: "svg", code: `@startuml
actor a
actor b
	@enduml`},
	{renderer: "plantuml-v1.2025.3", format: "svg", code: `@startuml
actor a
acctor b
	@enduml`},
]));
stdinStream.push(null);
stdinStream.pipe(prom.child.stdin);
const res = await prom;
const ressvg = JSON.parse(res.stdout);
assert(ressvg[0].result, "result[0] should have a result");
assert(ressvg[1].error, "result[1] should have an error");
console.log(ressvg)
''}/bin/script > $out/multierror.json
	'';

	allVersions = pkgs.runCommand "all_versions" {} ''
mkdir -p $out

${pkgs.writeScriptBin "script" ''
#!${pkgs.nodejs_latest}/bin/node
import child_process from "node:child_process";
import stream from "node:stream";
import util from "node:util";
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import path from "node:path";

const render = async (code, renderer, format) => {
	const prom = util.promisify(child_process.execFile)("${diagram_generator}/bin/diagram-generator");
	const stdinStream = new stream.Readable();
	stdinStream.push(JSON.stringify([{renderer, code, format}]));
	stdinStream.push(null);
	stdinStream.pipe(prom.child.stdin);
	const res = await prom;
	const ressvg = JSON.parse(res.stdout);
	assert(ressvg[0].result, `result[0] should have a result, result: ''${JSON.stringify(ressvg, undefined, 4)}`);
	return ressvg[0].result;
}

const versions = JSON.parse(await fs.readFile("${./supported-versions.json}", "utf8"));
await Promise.all(versions.plantuml.map(async (v) => {
	const code = "@startuml\na -> b\n@enduml";
	await render(code, "plantuml-" + v, "png").then((r) => fs.writeFile(path.join(process.env.out, "plantuml-" + v + ".png"), Buffer.from(r, "base64")));
	await render(code, "plantuml-" + v, "svg").then((r) => fs.writeFile(path.join(process.env.out, "plantuml-" + v + ".svg"), r));
}));

await Promise.all(versions.recharts.map(async (v) => {
	const code = `
const data = [0, 1, 2, 3].map((r) => ({ia: 1.25 + r * 1}));

<LineChart data={data} width={400} height={300}
	margin={{ top: 10, right: 10, left: 20, bottom: 20 }}>
	<CartesianGrid strokeDasharray="3 3" />
	<YAxis width={40}>
		<Label angle={-90} position="insideLeft">$/month/GB</Label>
	</YAxis>
	<XAxis label="Retrievals/month" position="insideBottom" height={60}/>
	<Line type="monotone" dataKey="ia" stroke="red"/>
	<ReferenceLine y={2.3} label={<Label value="S3 Standard" position="insideBottomRight"/>} stroke="orange" strokeDasharray="3 3" strokeWidth={2}/>
</LineChart>
	`;
	await render(code, "recharts-" + v, "svg").then((r) => fs.writeFile(path.join(process.env.out, "recharts-" + v + ".svg"), r));
}));

await Promise.all(versions.swirly.map(async (v) => {
	const code = `
-1-2-3-4-5|

> orderedMergeMap

--A--BC--D--E|
A := P1
B := P2
C := P3
D := P4
E := P5
	`;
	await render(code, "swirly-" + v, "svg").then((r) => fs.writeFile(path.join(process.env.out, "swirly-" + v + ".svg"), r));
}));

''}/bin/script
	'';

in
	pkgs.symlinkJoin {
		name = "test";
		paths = [
			(svgtest ./tests/test.txt)
			(pngandsvgtest ./tests/test2.txt)
			(pngandsvgtest ./tests/test3.txt)
			(pngandsvgtest ./tests/test4.txt)
			(pngandsvgtest ./tests/test5.txt)
			(pngandsvgtest ./tests/test6.txt)
			errortest
			multierrortest
			allVersions
		];
	}
