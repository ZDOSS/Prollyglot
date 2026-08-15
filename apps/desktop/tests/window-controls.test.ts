import assert from "node:assert/strict";
import test from "node:test";

import { titlebarOperation } from "../src/window-controls.ts";

test("titlebar gestures distinguish drag, maximize, and interactive controls", () => {
  assert.equal(titlebarOperation(0, 1, false), "drag");
  assert.equal(titlebarOperation(0, 2, false), "maximize");
  assert.equal(titlebarOperation(0, 1, true), undefined);
  assert.equal(titlebarOperation(2, 1, false), undefined);
});
