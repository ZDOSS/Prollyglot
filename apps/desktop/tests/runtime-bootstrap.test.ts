import assert from "node:assert/strict";
import test from "node:test";

import { initializeRuntimeBootstrap } from "../src/runtime-bootstrap.ts";
import { RUNTIME_CONTRACT_VERSION } from "../src/generated/runtime.ts";
import type { RuntimeSnapshot } from "../src/types.ts";

function runtime(revision: number): RuntimeSnapshot {
  return {
    contractVersion: RUNTIME_CONTRACT_VERSION,
    revision,
    sessionId: null,
    mode: null,
    source: null,
    lifecycle: "stopped",
    health: { level: "healthy", progress: "idle", message: null },
    failure: null
  };
}

test("bootstrap applies an event that arrived before an older snapshot", async () => {
  let runtimeListener: ((snapshot: RuntimeSnapshot) => void) | undefined;
  const applied: number[] = [];
  await initializeRuntimeBootstrap({
    onRuntimeState: async (listener) => {
      runtimeListener = listener;
      return () => undefined;
    },
    onCaptureStatus: async () => () => undefined,
    onVisualStatus: async () => () => undefined,
    runtimeBootstrap: async () => {
      runtimeListener?.(runtime(5));
      return { snapshot: runtime(3) };
    },
    captureStatus: async () => ({ state: "stopped", peak: 0, droppedFrames: 0 }),
    visualStatus: async () => ({
      active: false,
      state: "stopped",
      framesReceived: 0,
      framesAnalyzed: 0,
      framesUnchanged: 0,
      replacedFrames: 0,
      visibleRegions: 0,
      overlayRegions: 0
    })
  }, {
    applyRuntime: (snapshot) => applied.push(snapshot.revision),
    renderCapture: () => undefined,
    renderVisual: () => undefined
  });

  assert.deepEqual(applied, [5]);
});
