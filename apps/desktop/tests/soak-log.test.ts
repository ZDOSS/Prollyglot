import assert from "node:assert/strict";
import test from "node:test";

import { auditSoakLog } from "../../../scripts/soak-log.mjs";

const runtime = (sessionId: number, lifecycle: string) =>
  `2026-08-14T00:00:00Z INFO runtime state changed revision=1 session_id=${sessionId} session_active=true mode=Some(AudioCaptions) lifecycle=${lifecycle} progress=Live`;
const loaded = (sessionId: number, kind: string, model = "model-a", bytes = 200_000_000) =>
  `2026-08-14T00:00:01Z INFO local inference resource loaded session_id=${sessionId} mode=AudioCaptions kind=${kind} model_id=${model} cold_start_ms=10 resident_bytes=Some(${bytes})`;
const unloaded = (sessionId: number, kind: string, model = "model-a") =>
  `2026-08-14T00:00:02Z INFO local inference resource unloaded session_id=${sessionId} mode=AudioCaptions kind=${kind} model_id=${model}`;
const released = (sessionId: number, bytes = 210_000_000) =>
  `2026-08-14T00:00:03Z INFO session inference resources released session_id=${sessionId} resident_bytes=Some(${bytes})`;

test("a balanced lifecycle log reports session, resource, and memory evidence", () => {
  const log = [
    runtime(1, "Starting"),
    loaded(1, "Speech"),
    loaded(1, "Translation", "translator-a", 260_000_000),
    unloaded(1, "Translation", "translator-a"),
    unloaded(1, "Speech"),
    released(1),
    runtime(0, "Stopped"),
    runtime(2, "Starting"),
    loaded(2, "VisualOcr", "ocr-a", 240_000_000),
    unloaded(2, "VisualOcr", "ocr-a"),
    released(2, 215_000_000)
  ].join("\n");
  const audit = auditSoakLog(log, { minSessions: 2 });

  assert.equal(audit.ok, true);
  assert.equal(audit.startedSessions, 2);
  assert.deepEqual(audit.loadsByKind, { Speech: 1, VisualOcr: 1, Translation: 1 });
  assert.equal(audit.peakResidentBytes, 260_000_000);
});

test("an orphaned inference resource fails the audit", () => {
  const audit = auditSoakLog([
    runtime(4, "Starting"),
    loaded(4, "Speech"),
    released(4)
  ].join("\n"), {
    minSessions: 1,
    requiredKinds: ["Speech"]
  });

  assert.equal(audit.ok, false);
  assert.match(audit.failures.join("\n"), /remained loaded/u);
});

test("forbidden media-content fields fail the privacy audit", () => {
  const audit = auditSoakLog([
    runtime(5, "Starting"),
    loaded(5, "Speech"),
    unloaded(5, "Speech"),
    released(5),
    "WARN frontend diagnostic caption_text=private-words"
  ].join("\n"), {
    minSessions: 1,
    requiredKinds: ["Speech"]
  });

  assert.equal(audit.ok, false);
  assert.equal(audit.privacyViolations.length, 1);
});

test("session IDs reused after an application restart count as distinct sessions", () => {
  const log = [
    "2026-08-14T00:00:00Z INFO Prollyglot started version=0.1.12",
    runtime(1, "Starting"),
    loaded(1, "Speech"),
    unloaded(1, "Speech"),
    released(1),
    "2026-08-14T01:00:00Z INFO Prollyglot started version=0.1.12",
    runtime(1, "Starting"),
    loaded(1, "Speech"),
    unloaded(1, "Speech"),
    released(1, 212_000_000)
  ].join("\n");
  const audit = auditSoakLog(log, {
    minSessions: 2,
    requiredKinds: ["Speech"]
  });

  assert.equal(audit.ok, true);
  assert.equal(audit.startedSessions, 2);
  assert.equal(audit.releaseSamples.length, 2);
});
