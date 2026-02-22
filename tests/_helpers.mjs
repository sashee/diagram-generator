import assert from "node:assert/strict";
import child_process from "node:child_process";
import fs from "node:fs/promises";
import path from "node:path";
import process from "node:process";

const diagramGeneratorBin = process.env.DIAGRAM_GENERATOR_BIN;
assert(diagramGeneratorBin, "DIAGRAM_GENERATOR_BIN is required");

const testOutDir = process.env.TEST_OUT_DIR;
assert(testOutDir, "TEST_OUT_DIR is required");
await fs.mkdir(testOutDir, { recursive: true });

export const outDir = testOutDir;

export const writeArtifact = async (name, content) => {
  const filePath = path.join(testOutDir, name);
  await fs.mkdir(path.dirname(filePath), { recursive: true });
  await fs.writeFile(filePath, content, "utf8");
  return filePath;
};

export const writeBytesArtifact = async (name, bytes) => {
  const filePath = path.join(testOutDir, name);
  await fs.mkdir(path.dirname(filePath), { recursive: true });
  await fs.writeFile(filePath, bytes);
  return filePath;
};

export const runCli = async ({ args = [], stdin = "" } = {}) => {
  const child = child_process.spawn(diagramGeneratorBin, args, {
    stdio: ["pipe", "pipe", "pipe"],
  });

  const stdoutChunks = [];
  const stderrChunks = [];
  child.stdout.on("data", (chunk) => stdoutChunks.push(chunk));
  child.stderr.on("data", (chunk) => stderrChunks.push(chunk));

  child.stdin.on("error", (err) => {
    if (err.code !== "EPIPE") {
      throw err;
    }
  });
  child.stdin.end(stdin);

  const { code, signal } = await new Promise((resolve, reject) => {
    child.on("error", reject);
    child.on("close", (statusCode, statusSignal) => resolve({ code: statusCode, signal: statusSignal }));
  });

  return {
    code,
    signal,
    stdout: Buffer.concat(stdoutChunks).toString("utf8"),
    stderr: Buffer.concat(stderrChunks).toString("utf8"),
  };
};

export const assertSuccess = (res) => {
  assert.equal(res.signal, null, `unexpected signal: ${res.signal}`);
  assert.equal(res.code, 0, `expected success, got code ${res.code}\nstderr:\n${res.stderr}`);
};

export const assertFailure = (res) => {
  assert.equal(res.signal, null, `unexpected signal: ${res.signal}`);
  assert.notEqual(res.code, 0, "expected non-zero exit code");
};

export const parseJson = (str) => JSON.parse(str);

export const listRenderers = async () => {
  const res = await runCli({ args: ["--list-available-renderers"] });
  assertSuccess(res);
  return parseJson(res.stdout);
};

export const pickPlantumlRenderer = async () => {
  const available = await listRenderers();
  const first = available.plantuml?.[0]?.renderer;
  assert(first, "expected at least one plantuml renderer");
  return first;
};

export const validPlantumlCode = "@startuml\na -> b\n@enduml\n";
export const invalidPlantumlCode = "@startuml\nactor a\nacctor b\n@enduml\n";
