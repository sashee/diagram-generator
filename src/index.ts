import util from "node:util";
import child_process from "node:child_process";
import process from "node:process";
import os from "node:os";
import * as plantUml from "plantuml-v1.2025.3";
import fs from "node:fs/promises";
import path from "node:path";

const createTempDir = async () => fs.mkdtemp(await fs.realpath(os.tmpdir()) + path.sep);

export const initRenderer = () => {
	const delimitor = `END${Math.random()}`;

	const plantumlProm = (async () => {
		{
			try {
				await util.promisify(child_process.execFile)("java", ["-jar", plantUml.path, "-testdot"], {env: {"JAVA_TOOL_OPTIONS": `-XX:+SuppressFatalErrorMessage -Djava.io.tmpdir=${os.tmpdir()}`, PATH: process.env["PATH"], GRAPHVIZ_DOT: process.env["GRAPHVIZ_DOT"]}});
			}catch(e) {
				console.error("PLANTUML GENERATION FAILED");
				console.error(e);
				process.exit(1);
			}
		}

		const child = child_process.spawn("java", ["-jar", plantUml.path, "-tsvg", "-pipe", "-pipedelimitor", `<!--${delimitor}-->`, "-noerror", "-nometadata", "-pipeNoStdErr"], {stdio: ["pipe", "pipe", "pipe"], env: {"JAVA_TOOL_OPTIONS": `-XX:+SuppressFatalErrorMessage -Djava.io.tmpdir=${os.tmpdir()}`, "PLANTUML_SECURITY_PROFILE": "SANDBOX", PATH: process.env["PATH"], GRAPHVIZ_DOT: process.env["GRAPHVIZ_DOT"]}, cwd: await createTempDir()});

		return child;
	})();

	const shutdownPlantuml = async () => {
		const child = await plantumlProm;
		new Promise((res) => {
			child.stdin.end();
			child.on("exit", res);
		});
	};

	const generatePlantuml = (() => {
		let queue = Promise.resolve("");

		return (code: string) => {
			const result = queue.then(async () => {
				const child = await plantumlProm;
				return new Promise<string>((res, rej) => {
					let result = "";

					const stdoutListener = (data: Buffer) => {
						result += data.toString("utf8");
						if (data.toString("utf8").indexOf(delimitor) !== -1) {
							child.stdout.removeListener("data", stdoutListener);
							if (result.trim().startsWith("ERROR")) {
								console.error(result);
								console.error(code);
								rej(result);
							} else{
								// sometimes there is some garbage before the svg
								// so cut everything before the first <
								res(result.substring(result.indexOf("<")));
							}
						}
					};
					child.stdout.on("data", stdoutListener);
					child.stdin.write(`@startuml\n${code}\n@enduml\n`);
				})
			});

			queue = result.catch(() => "");
			return result;
		};
	})();

	return {
		generatePlantuml,
		shutdownPlantuml,
	};
};
