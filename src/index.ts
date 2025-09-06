import util from "node:util";
import child_process from "node:child_process";
import process from "node:process";
import os from "node:os";
import fs from "node:fs/promises";
import path from "node:path";
import { text } from "node:stream/consumers";
import stream from "node:stream";
import assert from "node:assert/strict";

const depPatterns = {
	plantuml: /^plantuml-(?<version>.*)$/,
	recharts: /^recharts-(?<version>.*)$/,
	swirly: /^swirly-(?<version>.*)$/,
};

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
	[engine: string]: {
		bin: string,
		version: string,
		formats: string[],
		renderer: string,
	}[],
} | undefined | null;
if (availableRenderers === undefined || availableRenderers === null) {
	throw new Error("AVAILABLE_RENDERERS undefined");
}

const extractSvg = (str: string) => str.match(/<svg.*<\/svg>/s)?.[0];

type Stdin = Array<{
	renderer: string,
	format: "svg" | "png",
	code: string,
}>;

const render = async (codes: string[], renderer: string, format: Stdin[0]["format"]): Promise<{result?: string, error?: string}[]> => {
	assert(codes.length !== 0, "codes.length is zero");
	try {
		if (renderer.match(depPatterns.plantuml)) {
			const {version} = renderer.match(depPatterns.plantuml)!.groups!;
			return await withTempDir(async (cwd) => {
				await Promise.all(codes.map(async (code, i) => fs.writeFile(path.join(cwd, `in_${i}.puml`), code, "utf8")));

				const usedRenderer = availableRenderers["plantuml"].find((r) => r.version === version)!;
				if (format === "png") {
					await util.promisify(child_process.execFile)(usedRenderer.bin, [".", "-tpng", "-o", "out", "-nometadata"], {cwd});
					return await Promise.all(codes.map(async (_code, i) => {
						const contents = await fs.readFile(path.join(cwd, "out", `in_${i}.png`))
						return {result: contents.toString("base64")};
					}));
				}else {
					await util.promisify(child_process.execFile)(usedRenderer.bin, [".", "-tsvg", "-o", "out", "-nometadata"], {cwd});
					return await Promise.all(codes.map(async (_code, i) => {
						const contents = await fs.readFile(path.join(cwd, "out", `in_${i}.svg`), "utf8")
						return {result: extractSvg(contents)};
					}));
				}
			});
		}else if (renderer.match(depPatterns.recharts)) {
			const {version} = renderer.match(depPatterns.recharts)!.groups!;
			const usedRenderer = availableRenderers["recharts"].find((r) => r.version === version)!;

			return await Promise.all(codes.map(async (code) => {
				const prom = util.promisify(child_process.execFile)(usedRenderer.bin);
				const stdinStream = new stream.Readable();
				stdinStream.push(code);
				stdinStream.push(null);
				stdinStream.pipe(prom.child.stdin);
				const res = await prom;
				return {result: res.stdout.trim()};
			}));
		}else if (renderer.match(depPatterns.swirly)) {
			const {version} = renderer.match(depPatterns.swirly)!.groups!;
			const usedRenderer = availableRenderers["swirly"].find((r) => r.version === version)!;

			return await Promise.all(codes.map(async (code) => {
				const prom = util.promisify(child_process.execFile)(usedRenderer.bin);
				const stdinStream = new stream.Readable();
				stdinStream.push(code);
				stdinStream.push(null);
				stdinStream.pipe(prom.child.stdin);
				const res = await prom;
				console.error(res.stderr)
				return {result: res.stdout.trim()};
			}));
		}else {
			throw new Error(`Not supported renderer: ${renderer}`);
		}
	}catch(e) {
		if (codes.length === 1) {
			return [{error: e.stderr}];
		}else {
			return await Promise.all(codes.map(async (code) => {
				return (await render([code], renderer, format))[0];
			}));
		}
	}
};

const parseStdin = (stdin: string): Stdin => {
	const parsed = JSON.parse(stdin);
	console.error(parsed);
	assert(Array.isArray(parsed), `Stdin must be Array, got: ${parsed}`);
	parsed.forEach((r) => {
		assert.equal(typeof r, "object");
		assert.equal(typeof r["renderer"], "string");
		assert.equal(typeof r["code"], "string");
		assert.equal(typeof r["format"], "string");
		const availableRendererStrings = Object.entries(availableRenderers).flatMap(([, configs]) => configs.flatMap(({renderer}) => renderer));
		console.error(r);
		assert(availableRendererStrings.includes(r["renderer"]), `renderer not available. Renderer: ${r["renderer"]}, available renderers: ${availableRendererStrings}`);
		const foundEngine = Object.entries(availableRenderers).find(([, configs]) => configs.find(({renderer}) => renderer === r["renderer"]));
		assert(foundEngine);
		const foundRenderer = foundEngine[1].find(({renderer}) => renderer === r["renderer"]);
		assert(foundRenderer);
		assert(foundRenderer?.formats.includes(r["format"]));
	});

	return parsed;
}

const stdin = parseStdin(await text(process.stdin));

console.error(stdin);

const renderGroups = Object.entries(Object.groupBy(
	stdin.map((r, i) => ({index: i, r})),
	({r}) => JSON.stringify({renderer: r.renderer, format: r.format}),
));

console.error(renderGroups);
const results = await Promise.all(renderGroups.map(async ([rendererAndFormatString, groups]) => {
	const {renderer, format} = JSON.parse(rendererAndFormatString);
	const results = await render(groups!.map(({index, r}) => r.code), renderer, format);
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
