import assert from "node:assert/strict";
import test from "node:test";

import {
  initialWorkspaceNavigation,
  reduceWorkspaceNavigation
} from "../src/workspace-navigation.ts";

test("full navigation mounts each page once and never requests a compact dialog", () => {
  let state = initialWorkspaceNavigation("full");
  state = reduceWorkspaceNavigation(state, { type: "navigate", destination: "models" });
  state = reduceWorkspaceNavigation(state, { type: "navigate", destination: "captions" });
  state = reduceWorkspaceNavigation(state, { type: "navigate", destination: "models" });

  assert.equal(state.destination, "models");
  assert.equal(state.compactPanel, undefined);
  assert.deepEqual([...state.mountedPages].sort(), ["captions", "models"]);
});

test("compact destinations use a contained panel without mounting full pages", () => {
  let state = initialWorkspaceNavigation("compact");
  state = reduceWorkspaceNavigation(state, { type: "navigate", destination: "transcript" });

  assert.equal(state.destination, "transcript");
  assert.equal(state.compactPanel, "transcript");
  assert.deepEqual([...state.mountedPages], ["captions"]);

  state = reduceWorkspaceNavigation(state, { type: "navigate", destination: "captions" });
  assert.equal(state.compactPanel, undefined);
});

test("changing view mode closes secondary presentation while preserving mounted pages", () => {
  let state = initialWorkspaceNavigation("full");
  state = reduceWorkspaceNavigation(state, { type: "navigate", destination: "appearance" });
  state = reduceWorkspaceNavigation(state, { type: "view-mode", viewMode: "compact" });

  assert.equal(state.viewMode, "compact");
  assert.equal(state.destination, "captions");
  assert.equal(state.compactPanel, undefined);
  assert.equal(state.mountedPages.has("appearance"), true);
});
