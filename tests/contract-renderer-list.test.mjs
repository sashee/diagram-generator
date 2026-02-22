import assert from "node:assert/strict";
import fs from "node:fs/promises";
import process from "node:process";
import { listRenderers, writeArtifact } from "./_helpers.mjs";

const first = await listRenderers();
const second = await listRenderers();

await writeArtifact("list-renderers-first.json", JSON.stringify(first, null, 2));
await writeArtifact("list-renderers-second.json", JSON.stringify(second, null, 2));

assert.deepEqual(second, first, "list-renderers should be deterministic");

for (const [engine, renderers] of Object.entries(first)) {
  assert(Array.isArray(renderers), `expected array for engine ${engine}`);
  for (const renderer of renderers) {
    assert.equal(typeof renderer.renderer, "string");
    assert(Array.isArray(renderer.formats), `formats must be array for ${renderer.renderer}`);
    renderer.formats.forEach((format) => assert.equal(typeof format, "string"));
  }
}

const supportedVersionsPath = process.env.SUPPORTED_VERSIONS_JSON;
assert(supportedVersionsPath, "SUPPORTED_VERSIONS_JSON is required");
const supportedVersions = JSON.parse(await fs.readFile(supportedVersionsPath, "utf8"));
await writeArtifact("supported-versions.json", JSON.stringify(supportedVersions, null, 2));

const expectedRenderers = Object.entries(supportedVersions).flatMap(([engine, versions]) =>
  versions.map(({ version }) => `${engine}-${version}`),
).sort();
const actualRenderers = Object.values(first)
  .flatMap((configs) => configs.map(({ renderer }) => renderer))
  .sort();

assert.deepEqual(actualRenderers, expectedRenderers);

const expectedFormatsByRenderer = Object.fromEntries(
  Object.entries(supportedVersions).flatMap(([engine, versions]) =>
    versions.map(({ version, formats }) => [`${engine}-${version}`, [...formats].sort()]),
  ),
);
const actualFormatsByRenderer = Object.fromEntries(
  Object.values(first)
    .flatMap((configs) => configs.map(({ renderer, formats }) => [renderer, [...formats].sort()])),
);
assert.deepEqual(actualFormatsByRenderer, expectedFormatsByRenderer);

await writeArtifact("success.txt", "ok\n");
