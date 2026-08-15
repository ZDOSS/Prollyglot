import assert from "node:assert/strict";
import test from "node:test";

import {
  PresentationCursor,
  acceptsPresentationFrame,
  captionDisplayState
} from "../src/presentation-state.ts";
import type { CaptionPresentationFrame } from "../src/types.ts";

function captionFrame(
  patch: Partial<CaptionPresentationFrame> = {}
): CaptionPresentationFrame {
  return {
    sessionId: 3,
    runtimeRevision: 10,
    presentationRevision: 1,
    phase: "holding",
    readableAtMs: 1_000,
    mode: "both",
    targetLanguage: "en",
    entries: [{
      key: "ja:1",
      sourceLanguage: "ja",
      original: "ニュース",
      translation: "News",
      translationPending: false,
      isFinal: true
    }],
    ...patch
  };
}

test("presentation revisions reject duplicates and delayed prior sessions", () => {
  const cursor = new PresentationCursor<CaptionPresentationFrame>();
  assert.equal(cursor.accept(captionFrame()), true);
  assert.equal(cursor.accept(captionFrame()), false);
  assert.equal(cursor.accept(captionFrame({ presentationRevision: 2 })), true);
  assert.equal(cursor.accept(captionFrame({
    sessionId: 4,
    runtimeRevision: 12,
    presentationRevision: 1
  })), true);
  assert.equal(cursor.accept(captionFrame({
    sessionId: 3,
    runtimeRevision: 11,
    presentationRevision: 99
  })), false);
  assert.equal(acceptsPresentationFrame(
    { sessionId: 4, runtimeRevision: 12, presentationRevision: 1 },
    { sessionId: 5, runtimeRevision: 12, presentationRevision: 1 }
  ), false);
});

test("active captions remain visible until a holding frame arrives", () => {
  assert.deepEqual(
    captionDisplayState(captionFrame({ phase: "active" }), 15, 800, 1_000_000),
    { phase: "visible" }
  );
});

test("holding captions receive their reading interval and fade", () => {
  const frame = captionFrame();
  assert.deepEqual(captionDisplayState(frame, 15, 800, 15_999), {
    phase: "visible",
    nextAtMs: 16_000
  });
  assert.deepEqual(captionDisplayState(frame, 15, 800, 16_000), {
    phase: "fading",
    nextAtMs: 16_800
  });
  assert.deepEqual(captionDisplayState(frame, 15, 800, 16_800), { phase: "hidden" });
});

test("pending and delayed translations have bounded independent reading time", () => {
  const pending = captionFrame({
    entries: [{
      ...captionFrame().entries[0]!,
      translation: undefined,
      translationPending: true
    }]
  });
  assert.deepEqual(captionDisplayState(pending, 6, 800, 7_000), {
    phase: "visible",
    nextAtMs: 31_000
  });

  const translated = captionFrame({
    presentationRevision: 2,
    readableAtMs: 25_000
  });
  assert.deepEqual(captionDisplayState(translated, 15, 800, 39_999), {
    phase: "visible",
    nextAtMs: 40_000
  });
});

test("cleared presentations hide immediately", () => {
  assert.deepEqual(
    captionDisplayState(captionFrame({ phase: "cleared", entries: [] }), 30, 2_000, 1_000),
    { phase: "hidden" }
  );
});
