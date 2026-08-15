import assert from "node:assert/strict";
import test from "node:test";

import {
  previewCaptureStatus,
  previewRuntimeSnapshot,
  previewSpeechCatalog,
  previewVisualStatus
} from "../src/preview-fixtures.ts";
import { RUNTIME_CONTRACT_VERSION } from "../src/generated/runtime.ts";

test("preview catalogs are isolated fixture instances rather than shared production state", () => {
  const first = previewSpeechCatalog();
  first.models[0]!.phase = "ready";
  first.models[0]!.downloadedBytes = first.models[0]!.totalBytes;

  const second = previewSpeechCatalog();
  assert.equal(second.models[0]?.phase, "notInstalled");
  assert.match(second.models[0]?.modelId ?? "", /^preview-/u);
  assert.equal(second.models.some(({ modelId }) => modelId.includes("nemotron")), false);
});

test("preview runtime fixtures use the generated session contract", () => {
  const snapshot = previewRuntimeSnapshot({
    revision: 4,
    activeSessionId: 9,
    audio: previewCaptureStatus({ state: "capturing" }),
    visual: previewVisualStatus()
  });

  assert.equal(snapshot.contractVersion, RUNTIME_CONTRACT_VERSION);
  assert.equal(snapshot.revision, 4);
  assert.equal(snapshot.sessionId, 9);
  assert.equal(snapshot.mode, "audioCaptions");
  assert.equal(snapshot.lifecycle, "running");
  assert.equal(snapshot.health.progress, "live");
});
