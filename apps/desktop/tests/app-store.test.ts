import assert from "node:assert/strict";
import test from "node:test";

import {
  AppStore,
  createInitialAppState,
  reduceAppState
} from "../src/app-store.ts";
import { RUNTIME_CONTRACT_VERSION } from "../src/generated/runtime.ts";
import type { RuntimeSnapshot } from "../src/types.ts";

function runtime(revision: number, lifecycle: RuntimeSnapshot["lifecycle"]): RuntimeSnapshot {
  const active = lifecycle !== "stopped";
  return {
    contractVersion: RUNTIME_CONTRACT_VERSION,
    revision,
    sessionId: active ? 7 : null,
    mode: active ? "audioCaptions" : null,
    source: active ? { id: "output", kind: "systemOutput", label: "Speakers" } : null,
    lifecycle,
    health: {
      level: "healthy",
      progress: lifecycle === "running" ? "live" : lifecycle === "starting" ? "preparingModel" : "idle",
      message: null
    },
    failure: null
  };
}

test("the application store rejects an older runtime after a newer event", () => {
  const store = new AppStore(createInitialAppState());
  const newest = store.dispatch({
    type: "runtime/received",
    snapshot: runtime(5, "running"),
    expectedContractVersion: RUNTIME_CONTRACT_VERSION
  });
  const stale = store.dispatch({
    type: "runtime/received",
    snapshot: runtime(4, "starting"),
    expectedContractVersion: RUNTIME_CONTRACT_VERSION
  });

  assert.equal(newest.runtime?.accepted, true);
  assert.equal(stale.changed, false);
  assert.equal(stale.runtime?.accepted, false);
  assert.equal(store.getState().runtime.snapshot?.revision, 5);
  assert.equal(store.getState().runtime.snapshot?.lifecycle, "running");
});

test("a contract mismatch is visible without replacing the accepted runtime", () => {
  let state = reduceAppState(createInitialAppState(), {
    type: "runtime/received",
    snapshot: runtime(3, "running"),
    expectedContractVersion: RUNTIME_CONTRACT_VERSION
  }).state;
  state = reduceAppState(state, {
    type: "runtime/received",
    snapshot: { ...runtime(4, "running"), contractVersion: 99 },
    expectedContractVersion: RUNTIME_CONTRACT_VERSION
  }).state;

  assert.equal(state.runtime.snapshot?.revision, 3);
  assert.equal(state.runtimeContractMismatch, 99);
});

test("transcript revisions cannot regress while other feature state remains independent", () => {
  const store = new AppStore(createInitialAppState());
  store.dispatch({
    type: "transcript/received",
    transcript: { revision: 8, committed: [] }
  });
  const stale = store.dispatch({
    type: "transcript/received",
    transcript: { revision: 7, committed: [] }
  });
  store.dispatch({
    type: "visual/status",
    status: {
      active: true,
      state: "capturing",
      framesReceived: 12,
      framesAnalyzed: 4,
      framesUnchanged: 5,
      replacedFrames: 0,
      visibleRegions: 2,
      overlayRegions: 1
    }
  });

  assert.equal(stale.changed, false);
  assert.equal(store.getState().transcript.revision, 8);
  assert.equal(store.getState().visualStatus.framesReceived, 12);
});

test("navigation, preferences, notices, and subscriptions have one owner", () => {
  const store = new AppStore(createInitialAppState());
  const actions: string[] = [];
  const unsubscribe = store.subscribe((_state, _previous, action) => actions.push(action.type));

  store.dispatch({ type: "navigation/view-mode", viewMode: "compact" });
  store.dispatch({ type: "navigation/destination", destination: "models" });
  store.dispatch({ type: "preferences/caption-mode", mode: "both" });
  store.dispatch({ type: "preferences/translation-target", language: "es" });
  store.dispatch({
    type: "notice/settings",
    notice: { message: "Installed", tone: "success" }
  });
  unsubscribe();
  store.dispatch({ type: "navigation/destination", destination: "captions" });

  assert.deepEqual(actions, [
    "navigation/view-mode",
    "navigation/destination",
    "preferences/caption-mode",
    "preferences/translation-target",
    "notice/settings"
  ]);
  assert.equal(store.getState().navigation.viewMode, "compact");
  assert.equal(store.getState().navigation.destination, "captions");
  assert.equal(store.getState().preferences.captionMode, "both");
  assert.equal(store.getState().preferences.translationTarget, "es");
  assert.deepEqual(store.getState().notices.settings, {
    message: "Installed",
    tone: "success"
  });
});
