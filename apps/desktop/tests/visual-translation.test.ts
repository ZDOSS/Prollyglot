import assert from "node:assert/strict";
import test from "node:test";
import { VisualTranslationController } from "../src/visual-translation.ts";
import type { VisualTextUpdate, VisualPresentationFrame } from "../src/types.ts";

Object.defineProperty(globalThis, "window", { value: globalThis, configurable: true });

const region = { trackId: 1, textRevision: 1, text: "你好世界", confidence: 0.99,
  language: "zh", bounds: { x: 100, y: 200, width: 200, height: 40 } };
function update(visible = true): VisualTextUpdate {
  return { sessionId: 1, runtimeRevision: 1, frameAgeMs: 0,
    source: { label: "fixture", x: 0, y: 0, width: 1920, height: 1080 },
    visible: visible ? [region] : [], translationRequests: visible ? [region] : [], removedTrackIds: visible ? [] : [1] };
}
async function settle() { for (let i = 0; i < 30; i++) await Promise.resolve(); }
function fixture(translate: () => Promise<string>) {
  const frames: VisualPresentationFrame[] = [];
  const controller = new VisualTranslationController({
    openSession: () => ({ id: "visual:1", prepare: async () => {}, translate,
      cancelQueued: () => {}, close: () => {} }), routeStatus: () => undefined
  }, async frame => { frames.push(frame); }, () => {});
  controller.begin("zh", "en");
  controller.setPresentationEpoch({ sessionId: 1, runtimeRevision: 1 });
  return { controller, frames };
}

test("a translation arriving after its source disappears is never shown", async () => {
  let resolve!: (text: string) => void;
  const { controller, frames } = fixture(() => new Promise(done => { resolve = done; }));
  controller.update(update());
  controller.update(update(false));
  resolve("Hello world");
  await settle();
  assert.equal(frames.some(frame => frame.regions.some(region => region.translation)), false);
  controller.clear();
});

test("transient failure retries unchanged source text with a bounded backoff", async t => {
  t.mock.timers.enable({ apis: ["setTimeout", "Date"] });
  let calls = 0;
  const { controller, frames } = fixture(async () => {
    calls++;
    if (calls === 1) throw new Error("temporary failure");
    return "Hello world";
  });
  controller.update(update());
  await settle();
  assert.equal(calls, 1);
  controller.update(update());
  t.mock.timers.tick(500);
  await settle();
  assert.equal(calls, 2);
  assert.equal(frames.at(-1)?.regions[0]?.translation, "Hello world");
  controller.clear();
});

test("permanent failures stop after three attempts and old OCR does not start work", async t => {
  t.mock.timers.enable({ apis: ["setTimeout", "Date"] });
  let calls = 0;
  const { controller } = fixture(async () => { calls++; throw new Error("failure"); });
  controller.update({ ...update(), frameAgeMs: 4_000 });
  await settle();
  assert.equal(calls, 0);
  controller.update(update());
  await settle();
  for (const delay of [500, 1_000, 2_000]) {
    t.mock.timers.tick(delay);
    controller.update(update());
    await settle();
  }
  assert.equal(calls, 3);
  controller.clear();
});
