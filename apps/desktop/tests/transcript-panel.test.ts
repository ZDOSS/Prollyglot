import assert from "node:assert/strict";
import test from "node:test";

import {
  formatTranscriptTimestamp,
  shouldFollowTranscriptLatest
} from "../src/transcript-panel.ts";

test("transcript navigation opens at the latest caption", () => {
  assert.equal(shouldFollowTranscriptLatest({
    forceLatest: true,
    hasPreviousList: true,
    followLatest: false,
    distanceFromBottom: 500
  }), true);
});

test("a deliberate scrollback is preserved across transcript updates", () => {
  assert.equal(shouldFollowTranscriptLatest({
    forceLatest: false,
    hasPreviousList: true,
    followLatest: false,
    distanceFromBottom: 500
  }), false);
  assert.equal(shouldFollowTranscriptLatest({
    forceLatest: false,
    hasPreviousList: true,
    followLatest: false,
    distanceFromBottom: 20
  }), true);
});

test("transcript timestamps are stable and clamp negative input", () => {
  assert.equal(formatTranscriptTimestamp(65_900_000), "01:05");
  assert.equal(formatTranscriptTimestamp(-1), "00:00");
});
