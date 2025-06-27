import util from "node:util";
import child_process from "node:child_process";
import process from "node:process";
import os from "node:os";
import packageJson from "../package.json" with {type: "json"};
import fs from "node:fs/promises";
import path from "node:path";

const plantumlDepPattern = /^plantuml-(?<version>.*)$/;
const rechartsDepPattern = /^recharts-(?<version>.*)$/;

const availableRenderers = await (async () => {
	const loadAvailableVersions = async (pattern: RegExp) => {
		const versions = Object.keys(packageJson.dependencies).filter((name) => name.match(pattern));

		return Object.fromEntries(await Promise.all(versions.map(async (d) => {
			return [d, await import(d)]; 
		})));
	};

	const [plantuml, recharts] = await Promise.all([loadAvailableVersions(plantumlDepPattern), loadAvailableVersions(rechartsDepPattern)]);

	return {
		plantuml,
		recharts,
	};
})();

export const renderers = Object.values(availableRenderers).reduce((m, r) => [...m, ...Object.keys(r)], []);

console.log(renderers)

const createTempDir = async () => fs.mkdtemp(await fs.realpath(os.tmpdir()) + path.sep);

const withTempDir = async <T> (fn: (path: string) => Promise<T>) => {
	const dir = await createTempDir();
	try {
		return await fn(dir);
	}finally {
		await fs.rm(dir, {recursive: true});
	}
};

export const render = async (code: string, renderer: string) => {
	if (renderer.match(plantumlDepPattern)) {
		return withTempDir(async (cwd) => {
			await fs.writeFile(path.join(cwd, "in.puml"), code, "utf8");
			await util.promisify(child_process.execFile)("java", ["-jar", availableRenderers.plantuml[renderer].path, "in.puml", "-tsvg", "-o", "out"], {env: {"JAVA_TOOL_OPTIONS": `-XX:+SuppressFatalErrorMessage -Djava.io.tmpdir=${os.tmpdir()}`, "PLANTUML_SECURITY_PROFILE": "SANDBOX", PATH: process.env["PATH"], GRAPHVIZ_DOT: process.env["GRAPHVIZ_DOT"]}, cwd});
			return fs.readFile(path.join(cwd, "out", "in.svg"), "utf8");
		});
	}else if (renderer.match(rechartsDepPattern)) {
		return availableRenderers.recharts[renderer].renderRecharts(code);
	}else {
		throw new Error(`Not supported renderer: ${renderer}`);
	}
};

{
const b=new Date();
	const res = await render(`
@startuml
caption Intelligent-Tiering

hide empty description

state "Frequently accessed" as fr
state "Infrequently accessed" as ia

[*] -> fr: put object
fr --> ia: 30 days inactivity
ia --> fr: read
@enduml

															 `, "plantuml-v1.2025.2");
															 console.log(`${new Date().getTime() - b.getTime()}`)
	console.log(res);
}
{
const b=new Date();
	const res = await render(`
const data = [40, 70, 100, 130, 160, 190, 220].map((r) => ({r, ia: 128 / Math.min(r, 128) * 1.25}));

<LineChart data={data} width={400} height={300}
	margin={{ top: 10, right: 20, left: 20, bottom: 20 }}>
	<CartesianGrid strokeDasharray="3 3" />
	<YAxis>
		<Label angle={-90} position="insideLeft">$/month/GB</Label>
	</YAxis>
	<XAxis dataKey="r" label="Object size (KB)" position="insideBottom" height={60}/>
	<Line type="monotone" dataKey="ia" stroke="red"/>
	<ReferenceLine y={2.3} label={<Label value="S3 Standard" position="insideBottomRight"/>} stroke="orange" strokeDasharray="3 3" strokeWidth={2}/>
</LineChart>

															 `, "recharts-2.15.4");
															 console.log(`${new Date().getTime() - b.getTime()}`)
	console.log(res);
}
