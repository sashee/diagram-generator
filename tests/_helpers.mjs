import assert from "node:assert/strict";
import crypto from "node:crypto";
import child_process from "node:child_process";
import fs from "node:fs/promises";
import path from "node:path";
import process from "node:process";

const diagramGeneratorBin = process.env.DIAGRAM_GENERATOR_BIN;
assert(diagramGeneratorBin, "DIAGRAM_GENERATOR_BIN is required");

export const svgToPngBin = process.env.SVG_TO_PNG_BIN;
export const svgFontInlinerBin = process.env.SVG_FONT_INLINER_BIN;
export const diagramGeneratorBinA = process.env.DIAGRAM_GENERATOR_BIN_A;
export const diagramGeneratorBinB = process.env.DIAGRAM_GENERATOR_BIN_B;
export const svgFontInlinerBinA = process.env.SVG_FONT_INLINER_BIN_A;
export const svgFontInlinerBinB = process.env.SVG_FONT_INLINER_BIN_B;

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

const runTextBin = async ({ bin, args = [], stdin = "" }) => {
  const child = child_process.spawn(bin, args, {
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

export const runCli = async ({ args = [], stdin = "" } = {}) => runTextBin({
  bin: diagramGeneratorBin,
  args,
  stdin,
});

export const runCliWithBin = async ({ bin, args = [], stdin = "" } = {}) => {
  assert(bin, "runCliWithBin requires bin");
  return runTextBin({ bin, args, stdin });
};

export const runSvgToPng = async ({ args = [], stdin = "" } = {}) => {
  assert(svgToPngBin, "SVG_TO_PNG_BIN is required");

  const child = child_process.spawn(svgToPngBin, args, {
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
    stdout: Buffer.concat(stdoutChunks),
    stderr: Buffer.concat(stderrChunks).toString("utf8"),
  };
};

export const runSvgFontInliner = async ({ args = [], stdin = "" } = {}) => {
  assert(svgFontInlinerBin, "SVG_FONT_INLINER_BIN is required");
  return runTextBin({ bin: svgFontInlinerBin, args, stdin });
};

export const runSvgFontInlinerWithBin = async ({ bin, args = [], stdin = "" } = {}) => {
  assert(bin, "runSvgFontInlinerWithBin requires bin");
  return runTextBin({ bin, args, stdin });
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

export const assertInlinedFonts = (svg, label = "svg") => {
  assert.equal(typeof svg, "string", `${label}: expected svg string`);
  assert(svg.includes("<svg"), `${label}: expected <svg`);
  assert(svg.includes("</svg>"), `${label}: expected </svg>`);
  assert(svg.includes("@font-face"), `${label}: expected @font-face rule`);
  assert(/url\(data:[^)]+;base64,[^)]+\)/.test(svg), `${label}: expected base64 data URL in @font-face`);
};

export const listRenderers = async () => {
  const res = await runCli({ args: ["--list-available-renderers"] });
  assertSuccess(res);
  return parseJson(res.stdout);
};

export const listRenderersWithBin = async (bin) => {
  const res = await runCliWithBin({ bin, args: ["--list-available-renderers"] });
  assertSuccess(res);
  return parseJson(res.stdout);
};

export const embeddedFontHashes = (svg) => {
  const hashes = [];
  const re = /url\((?:"|')?data:[^;]+;base64,([^)'"\s]+)(?:"|')?\)/g;
  for (const match of svg.matchAll(re)) {
    const bytes = Buffer.from(match[1], "base64");
    const hash = crypto.createHash("sha256").update(bytes).digest("hex");
    hashes.push(hash);
  }
  return hashes.sort();
};

export const pickPlantumlRenderer = async () => {
  const available = await listRenderers();
  const first = available.plantuml?.[0]?.renderer;
  assert(first, "expected at least one plantuml renderer");
  return first;
};

export const validPlantumlCode = "@startuml\na -> b\n@enduml\n";
export const invalidPlantumlCode = "@startuml\nactor a\nacctor b\n@enduml\n";
