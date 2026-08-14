import assert from "node:assert/strict";
import test from "node:test";

import {
  acceptsVisualSessionEvent,
  initialRuntimeCursor,
  reduceRuntimeSnapshot
} from "../src/runtime-state.ts";

import type { RuntimeSnapshot, SessionLifecycle, SessionMode } from "../src/types.ts";

const CONTRACT_VERSION = 1;

function snapshot(
  revision: number,
  sessionId: number | null,
  mode: SessionMode | null,
  lifecycle: SessionLifecycle
): RuntimeSnapshot {
  return {
    contractVersion: CONTRACT_VERSION,
    revision,
    sessionId,
    mode,
    source: null,
    lifecycle,
    health: { level: "healthy", progress: lifecycle === "running" ? "live" : "idle", message: null },
    failure: null
  };
}

test("runtime snapshots reject duplicate and older revisions", () => {
  const initial = initialRuntimeCursor();
  const running = reduceRuntimeSnapshot(
    initial,
    snapshot(5, 2, "audioCaptions", "running"),
    CONTRACT_VERSION
  );

  assert.equal(running.accepted, true);
  assert.equal(
    reduceRuntimeSnapshot(running.cursor, snapshot(4, 2, "audioCaptions", "starting"), CONTRACT_VERSION).accepted,
    false
  );
  assert.equal(
    reduceRuntimeSnapshot(running.cursor, snapshot(5, 2, "audioCaptions", "waiting"), CONTRACT_VERSION).accepted,
    false
  );
});

test("contract mismatches never replace the current runtime", () => {
  const current = reduceRuntimeSnapshot(
    initialRuntimeCursor(),
    snapshot(3, 1, "audioCaptions", "running"),
    CONTRACT_VERSION
  ).cursor;
  const incompatible = { ...snapshot(4, 1, "audioCaptions", "running"), contractVersion: 99 };
  const result = reduceRuntimeSnapshot(current, incompatible, CONTRACT_VERSION);

  assert.equal(result.accepted, false);
  assert.equal(result.contractMismatch, true);
  assert.equal(result.cursor.snapshot?.revision, 3);
});

test("visual events must belong to the current session and revision epoch", () => {
  let cursor = reduceRuntimeSnapshot(
    initialRuntimeCursor(),
    snapshot(8, 4, "visualTranslation", "running"),
    CONTRACT_VERSION
  ).cursor;
  assert.equal(acceptsVisualSessionEvent(cursor, 4, 8, false), true);
  assert.equal(acceptsVisualSessionEvent(cursor, 3, 8, false), false);

  cursor = reduceRuntimeSnapshot(
    cursor,
    snapshot(9, 4, "visualTranslation", "waiting"),
    CONTRACT_VERSION
  ).cursor;
  assert.equal(acceptsVisualSessionEvent(cursor, 4, 8, false), false);
  assert.equal(acceptsVisualSessionEvent(cursor, 4, 9, false), false);
  assert.equal(acceptsVisualSessionEvent(cursor, 4, 9, true), true);

  cursor = reduceRuntimeSnapshot(
    cursor,
    snapshot(10, 4, "visualTranslation", "running"),
    CONTRACT_VERSION
  ).cursor;
  assert.equal(acceptsVisualSessionEvent(cursor, 4, 9, false), false);
  assert.equal(acceptsVisualSessionEvent(cursor, 4, 10, false), true);
});

test("a replacement visual session rejects delayed output from its predecessor", () => {
  const first = reduceRuntimeSnapshot(
    initialRuntimeCursor(),
    snapshot(2, 1, "visualTranslation", "running"),
    CONTRACT_VERSION
  ).cursor;
  const stopped = reduceRuntimeSnapshot(
    first,
    snapshot(3, null, null, "stopped"),
    CONTRACT_VERSION
  ).cursor;
  const second = reduceRuntimeSnapshot(
    stopped,
    snapshot(4, 2, "visualTranslation", "starting"),
    CONTRACT_VERSION
  ).cursor;

  assert.equal(acceptsVisualSessionEvent(second, 1, 4, false), false);
  assert.equal(acceptsVisualSessionEvent(second, 2, 4, false), true);
});
