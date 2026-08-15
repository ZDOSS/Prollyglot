import assert from "node:assert/strict";
import test from "node:test";

import { planCaptionOutput, planTranslationTargets } from "../src/caption-form.ts";

test("automatic recognition keeps translation unavailable until segments report language", () => {
  const plan = planTranslationTargets("auto", "en");
  assert.equal(plan.disabled, true);
  assert.equal(plan.selected, "off");
});

test("translation targets preserve a valid preference and choose a safe fallback", () => {
  assert.equal(planTranslationTargets("ja", "es").selected, "es");
  assert.equal(planTranslationTargets("ja", "ja").selected, "en");
  assert.equal(planTranslationTargets("en", "en").selected, "off");
});

test("caption output exposes bilingual choices only for a valid route", () => {
  const enabled = planCaptionOutput("ja", "en", "both", "ready");
  const disabled = planCaptionOutput("auto", "en", "both", "ready");
  assert.deepEqual(enabled.options.map(({ value }) => value), ["original", "translated", "both"]);
  assert.equal(enabled.selected, "both");
  assert.equal(enabled.help.includes("starts from live partial speech"), true);
  assert.equal(disabled.disabled, true);
  assert.equal(disabled.selected, "original");
});
