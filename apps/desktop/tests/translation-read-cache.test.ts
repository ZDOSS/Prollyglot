import assert from "node:assert/strict";
import test from "node:test";

import { nativeArtifactResponse } from "../src/translation-native-stream.ts";

test("native translation reads reconstruct an artifact through bounded ranges", async () => {
  const requestedRanges: string[] = [];
  const nativeBytes = new Uint8Array([1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
  const fetchRange: typeof fetch = async (_input, init) => {
    const range = new Headers(init?.headers).get("range");
    assert.ok(range);
    requestedRanges.push(range);
    const match = /^bytes=(\d+)-(\d+)$/u.exec(range);
    assert.ok(match);
    const start = Number(match[1]);
    const end = Number(match[2]);
    return new Response(nativeBytes.slice(start, end + 1), {
      status: 206,
      headers: { "content-range": `bytes ${start}-${end}/${nativeBytes.byteLength}` }
    });
  };

  const response = await nativeArtifactResponse(
    "http://prollyglot-model.localhost/translation/model/artifact.onnx",
    nativeBytes.byteLength,
    "Test translator",
    fetchRange,
    4
  );
  assert.ok(response);
  assert.equal(response.headers.get("x-prollyglot-storage"), "native");
  assert.deepEqual(new Uint8Array(await response.arrayBuffer()), nativeBytes);
  assert.deepEqual(requestedRanges, ["bytes=0-3", "bytes=4-7", "bytes=8-9"]);
});

test("native translation reads report an unavailable native model without allocating", async () => {
  const response = await nativeArtifactResponse(
    "http://prollyglot-model.localhost/translation/model/artifact.onnx",
    100,
    "Test translator",
    async () => new Response("missing", { status: 404 }),
    4
  );
  assert.equal(response, undefined);
});
