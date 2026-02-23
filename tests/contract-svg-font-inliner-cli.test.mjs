import assert from "node:assert/strict";
import {
  assertFailure,
  assertInlinedFonts,
  assertSuccess,
  runSvgFontInliner,
  writeArtifact,
} from "./_helpers.mjs";

const validSvg = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"120\" height=\"40\"><text x=\"5\" y=\"20\" font-family=\"monospace\">hello</text></svg>";

const successRes = await runSvgFontInliner({ stdin: validSvg });
await writeArtifact("svg-font-inliner.success.stdin.svg", validSvg);
await writeArtifact("svg-font-inliner.success.stdout.svg", successRes.stdout);
await writeArtifact("svg-font-inliner.success.stderr.txt", successRes.stderr);
assertSuccess(successRes);
assertInlinedFonts(successRes.stdout, "svg-font-inliner success output");

const badArgsRes = await runSvgFontInliner({ args: ["unexpected"], stdin: validSvg });
await writeArtifact("svg-font-inliner.bad-args.stdout.txt", badArgsRes.stdout);
await writeArtifact("svg-font-inliner.bad-args.stderr.txt", badArgsRes.stderr);
assertFailure(badArgsRes);
assert(badArgsRes.stderr.includes("Usage:"), "bad-args: expected usage in stderr");

const invalidInputRes = await runSvgFontInliner({ stdin: "not svg" });
await writeArtifact("svg-font-inliner.bad-input.stdout.txt", invalidInputRes.stdout);
await writeArtifact("svg-font-inliner.bad-input.stderr.txt", invalidInputRes.stderr);
assertFailure(invalidInputRes);
assert(invalidInputRes.stderr.length > 0, "bad-input: expected stderr output");

await writeArtifact("success.txt", "ok\n");
