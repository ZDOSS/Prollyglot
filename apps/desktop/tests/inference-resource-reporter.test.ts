import assert from "node:assert/strict";
import test from "node:test";

import { InferenceResourceReporter } from "../src/inference-resource-reporter.ts";
import { RUNTIME_CONTRACT_VERSION } from "../src/generated/runtime.ts";
import type {
  ReportInferenceResourceCommand,
  RuntimeSnapshot
} from "../src/types.ts";
import type { TranslationTelemetry } from "../src/translation-scheduler.ts";

function runtime(
  sessionId: number | null,
  lifecycle: RuntimeSnapshot["lifecycle"] = "running"
): RuntimeSnapshot {
  return {
    contractVersion: RUNTIME_CONTRACT_VERSION,
    revision: 4,
    sessionId,
    mode: sessionId === null ? null : "audioCaptions",
    source: null,
    lifecycle,
    health: { level: "healthy", progress: "live", message: null },
    failure: null
  };
}

function telemetry(
  event: "loaded" | "unloaded",
  sessionId: string,
  modelId = "opus-ja-en"
): TranslationTelemetry {
  return {
    event,
    sessionId,
    modelId,
    inferenceMs: event === "loaded" ? 483.6 : undefined,
    queuedJobs: 0
  };
}

test("translation ownership is tied to its active native session", async () => {
  let current = runtime(17);
  const reports: ReportInferenceResourceCommand[] = [];
  const reporter = new InferenceResourceReporter(
    async (command) => {
      reports.push(command);
      return { revision: reports.length, processResidentBytes: null, resources: [] };
    },
    () => current
  );

  reporter.acceptTranslationTelemetry(telemetry("loaded", "captions:3"));
  current = runtime(null, "stopped");
  reporter.acceptTranslationTelemetry(telemetry("unloaded", "captions:3"));
  await reporter.settled();

  assert.deepEqual(reports.map(({ phase, sessionId, ownerId }) => ({
    phase,
    sessionId,
    ownerId
  })), [
    { phase: "loaded", sessionId: 17, ownerId: "captions:3" },
    { phase: "unloaded", sessionId: 17, ownerId: "captions:3" }
  ]);
  assert.equal(reports[0]?.coldStartMillis, 484);
});

test("loads while stopped are not reported as owned resources", async () => {
  const reports: ReportInferenceResourceCommand[] = [];
  const diagnostics: string[] = [];
  const reporter = new InferenceResourceReporter(
    async (command) => {
      reports.push(command);
      return { revision: 0, processResidentBytes: null, resources: [] };
    },
    () => runtime(null, "stopped"),
    (message) => diagnostics.push(message)
  );

  reporter.acceptTranslationTelemetry(telemetry("loaded", "captions:idle"));
  await reporter.settled();

  assert.deepEqual(reports, []);
  assert.match(diagnostics[0] ?? "", /outside an active native session/u);
});

test("resource reports preserve load-before-unload ordering", async () => {
  const phases: string[] = [];
  let releaseFirst!: () => void;
  const first = new Promise<void>((resolve) => { releaseFirst = resolve; });
  const reporter = new InferenceResourceReporter(
    async (command) => {
      if (command.phase === "loaded") await first;
      phases.push(command.phase);
      return { revision: phases.length, processResidentBytes: null, resources: [] };
    },
    () => runtime(8)
  );

  reporter.acceptTranslationTelemetry(telemetry("loaded", "visual:2"));
  reporter.acceptTranslationTelemetry(telemetry("unloaded", "visual:2"));
  await Promise.resolve();
  assert.deepEqual(phases, []);
  releaseFirst();
  await reporter.settled();
  assert.deepEqual(phases, ["loaded", "unloaded"]);
});
