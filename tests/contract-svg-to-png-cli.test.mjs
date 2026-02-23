import assert from "node:assert/strict";
import {
  assertFailure,
  assertSuccess,
  runSvgToPng,
  writeArtifact,
  writeBytesArtifact,
} from "./_helpers.mjs";

const readPngSize = (bytes) => {
  const signature = bytes.subarray(0, 8).toString("hex");
  assert.equal(signature, "89504e470d0a1a0a", "expected PNG signature");
  const ihdrChunkType = bytes.subarray(12, 16).toString("ascii");
  assert.equal(ihdrChunkType, "IHDR", "expected IHDR chunk");
  return {
    width: bytes.readUInt32BE(16),
    height: bytes.readUInt32BE(20),
  };
};

const svg = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"8\" height=\"8\"><circle cx=\"4\" cy=\"4\" r=\"2\" fill=\"#000\"/></svg>";

const defaultRes = await runSvgToPng({ stdin: svg });
await writeArtifact("svg-to-png.default.stderr.txt", defaultRes.stderr);
await writeBytesArtifact("svg-to-png.default.stdout.png", defaultRes.stdout);
assertSuccess({ ...defaultRes, stdout: "" });
assert(defaultRes.stdout.length > 8, "default: expected png bytes");
assert.equal(defaultRes.stdout.subarray(0, 8).toString("hex"), "89504e470d0a1a0a", "default: png signature mismatch");
const defaultPngSize = readPngSize(defaultRes.stdout);

const zoomRes = await runSvgToPng({ args: ["--zoom", "2"], stdin: svg });
await writeArtifact("svg-to-png.zoom.stderr.txt", zoomRes.stderr);
await writeBytesArtifact("svg-to-png.zoom.stdout.png", zoomRes.stdout);
assertSuccess({ ...zoomRes, stdout: "" });
assert(zoomRes.stdout.length > 8, "zoom: expected png bytes");
assert.equal(zoomRes.stdout.subarray(0, 8).toString("hex"), "89504e470d0a1a0a", "zoom: png signature mismatch");
const zoomPngSize = readPngSize(zoomRes.stdout);
assert(zoomPngSize.width > defaultPngSize.width, "zoom: expected output width to increase");
assert(zoomPngSize.height > defaultPngSize.height, "zoom: expected output height to increase");

const backgroundRes = await runSvgToPng({ args: ["--background", "#ffffff"], stdin: svg });
await writeArtifact("svg-to-png.background.stderr.txt", backgroundRes.stderr);
await writeBytesArtifact("svg-to-png.background.stdout.png", backgroundRes.stdout);
assertSuccess({ ...backgroundRes, stdout: "" });
assert(backgroundRes.stdout.length > 8, "background: expected png bytes");
assert.notDeepEqual(
  backgroundRes.stdout,
  defaultRes.stdout,
  "background: expected output bytes to differ from transparent default",
);

const badBackgroundRes = await runSvgToPng({ args: ["--background", "not-a-color"], stdin: svg });
await writeArtifact("svg-to-png.bad-background.stderr.txt", badBackgroundRes.stderr);
await writeBytesArtifact("svg-to-png.bad-background.stdout.bin", badBackgroundRes.stdout);
assertFailure({ ...badBackgroundRes, stdout: "" });
assert(badBackgroundRes.stderr.includes("--background"), "bad-background: expected background error in stderr");

const badZoomRes = await runSvgToPng({ args: ["--zoom", "0"], stdin: svg });
await writeArtifact("svg-to-png.bad-zoom.stderr.txt", badZoomRes.stderr);
await writeBytesArtifact("svg-to-png.bad-zoom.stdout.bin", badZoomRes.stdout);
assertFailure({ ...badZoomRes, stdout: "" });
assert(badZoomRes.stderr.includes("--zoom"), "bad-zoom: expected zoom error in stderr");

const badInputRes = await runSvgToPng({ stdin: "not svg" });
await writeArtifact("svg-to-png.bad-input.stderr.txt", badInputRes.stderr);
await writeBytesArtifact("svg-to-png.bad-input.stdout.bin", badInputRes.stdout);
assertFailure({ ...badInputRes, stdout: "" });
assert(badInputRes.stderr.length > 0, "bad-input: expected stderr output");

await writeArtifact("success.txt", "ok\n");
