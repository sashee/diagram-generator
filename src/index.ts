import util from "node:util";
import child_process from "node:child_process";
import process from "node:process";
import os from "node:os";
import fs from "node:fs/promises";
import path from "node:path";
import { text } from "node:stream/consumers";
import stream from "node:stream";
import assert from "node:assert/strict";

const plantumlDepPattern = /^plantuml-(?<version>.*)$/;
const rechartsDepPattern = /^recharts-(?<version>.*)$/;

const createTempDir = async () => fs.mkdtemp(await fs.realpath(os.tmpdir()) + path.sep);

const withTempDir = async <T> (fn: (path: string) => Promise<T>) => {
	const dir = await createTempDir();
	try {
		return await fn(dir);
	}finally {
		await fs.rm(dir, {recursive: true});
	}
};

const availableRenderers = JSON.parse(process.env["AVAILABLE_RENDERERS"] ?? "null") as {
	[renderer: string]: {
		bin: string,
		version: string,
	}[],
} | undefined | null;
if (availableRenderers === undefined || availableRenderers === null) {
	throw new Error("AVAILABLE_RENDERERS undefined");
}

const extractSvg = (str: string) => str.match(/<svg.*<\/svg>/s)?.[0];

const render = async (codes: string[], renderer: string) => {
	if (renderer.match(plantumlDepPattern)) {
		const {version} = renderer.match(plantumlDepPattern)!.groups!;
		return withTempDir(async (cwd) => {
			await Promise.all(codes.map(async (code, i) => fs.writeFile(path.join(cwd, `in_${i}.puml`), code, "utf8")));

			const usedRenderer = availableRenderers["plantuml"].find((r) => r.version === version)!;
			/*
			{
				const res = await util.promisify(child_process.execFile)(usedRenderer.bin, [".", "-tpng", "-o", "out2", "-nometadata"], {env: {"PLANTUML_LIMIT_SIZE": "81920"}, cwd});
				console.log(JSON.stringify(res, undefined,4))
				await Promise.all(codes.map(async (_code, i) => {
					console.log(await fs.readdir(path.join(cwd, "out2")));
					const id = crypto.randomUUID();
					await fs.copyFile(path.join(cwd, "out2", `in_${i}.png`), path.join("/", "tmp", `in_${i}_${id}.png`));
					console.log(`file:///tmp/in_${i}_${id}.png`);
				}));
			}
			return null;
			*/
			const res = await util.promisify(child_process.execFile)(usedRenderer.bin, [".", "-tsvg", "-o", "out", "-nometadata"], {cwd});
			try {
				return await Promise.all(codes.map(async (_code, i) => {
					const contents = await fs.readFile(path.join(cwd, "out", `in_${i}.svg`), "utf8")
					return extractSvg(contents);
				}));
			}catch (e) {
				throw new Error(`Plantuml generation failed. info: ${JSON.stringify({renderer, codes, res, e}, undefined, 4)}`);
			}
		});
	}else if (renderer.match(rechartsDepPattern)) {
		const {version} = renderer.match(rechartsDepPattern)!.groups!;
		const usedRenderer = availableRenderers["recharts"].find((r) => r.version === version)!;

		return await Promise.all(codes.map(async (code) => {
			const prom = util.promisify(child_process.execFile)(usedRenderer.bin);
			const stdinStream = new stream.Readable();
			stdinStream.push(code);
			stdinStream.push(null);
			stdinStream.pipe(prom.child.stdin);
			const res = await prom;
			return res.stdout.trim();
		}));
	}else {
		throw new Error(`Not supported renderer: ${renderer}`);
	}
};


type Stdin = {
	renderer: string,
	code: string,
}[];

const parseStdin = (stdin: string): Stdin => {
	const parsed = JSON.parse(stdin);
	assert(Array.isArray(parsed), `Stdin must be Array, got: ${parsed}`);
	parsed.forEach((r) => {
		assert(typeof r === "object");
		assert(typeof r["renderer"] === "string");
		assert(typeof r["code"] === "string");
		const availableRendererStrings = Object.entries(availableRenderers).flatMap(([engine, configs]) => configs.flatMap(({version}) => `${engine}-${version}`));
		assert(availableRendererStrings.includes(r["renderer"]), `renderer not available. Renderer: ${r["renderer"]}, available renderers: ${availableRenderers}`);
	});

	return parsed;
}

const stdin = parseStdin(await text(process.stdin));

console.error(stdin);

const renderGroups = Object.entries(Object.groupBy(
	stdin.map((r, i) => ({index: i, r})),
	({r}) => r.renderer,
));

console.error(renderGroups);
const results = await Promise.all(renderGroups.map(async ([renderer, groups]) => {
	const results = await render(groups!.map(({index, r}) => r.code), renderer);
	return results.map((result, idx) => {
		return {
			result,
			index: groups!.map(({index}) => index)[idx],
		}
	})
}));

console.log(JSON.stringify(
	results
		.flat()
		.toSorted((a, b) => a.index - b.index)
		.map(({result}) => result)
));
