import assert from "node:assert/strict";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";

// Uses only a new headless browser and fake capture/model services. No portal
// picker, native windows, desktop screenshots or model downloads are involved.
const module = process.env.PROLLYGLOT_PLAYWRIGHT_MODULE;
const { chromium } = await import(module ? pathToFileURL(resolve(module)).href : "playwright");
const base = process.argv[2] ?? "http://localhost:1420";
const browser = await chromium.launch({ headless: true, executablePath: process.env.PROLLYGLOT_CHROMIUM });
try {
  const page = await browser.newPage();
  const errors = [];
  page.on("pageerror", error => errors.push(error.message));
  await page.route("**/portal-ui-fixture.html", route => route.fulfill({
    contentType: "text/html",
    body: '<!doctype html><html lang="en"><head><title>Prollyglot portal UI fixture</title><link rel="stylesheet" href="/src/styles.css"></head><body><main class="main-window" style="height:auto;min-height:100vh"><section id="fixture"></section></main></body></html>'
  }));
  await page.goto(`${base}/portal-ui-fixture.html`);
  await page.evaluate(async () => {
    const { VisualPanel } = await import("/src/visual-panel.ts");
    const { TRANSLATION_MODELS } = await import("/src/translation-catalog.ts");
    const fixture = window.fixture = { starts: [], stops: 0 };
    fixture.panel = new VisualPanel({ sourceMode: "region", sourceLanguage: "zh", targetLanguage: "en", detectionMode: "focused", displayId: "stale-windows-id" }, () => {}, "qa");
    fixture.state = {
      capabilities: { windowsGraphicsCapture: false, portalScreenCast: true, systemPicker: false, desktopDuplicationExperiment: false },
      sources: { windows: [], displays: [] }, audioActive: false,
      models: { models: [{ modelId: "fixture", phase: "ready", displayName: "OCR fixture", languages: ["zh", "es"], totalBytes: 1 }] },
      translations: { models: TRANSLATION_MODELS.map(model => ({ ...model, phase: "ready" })) },
      status: { active: false, state: "stopped", framesReceived: 0, framesAnalyzed: 0, framesUnchanged: 0, replacedFrames: 0, visibleRegions: 0, overlayRegions: 0 }
    };
    fixture.actions = {
      refreshSources: async () => { throw new Error("Portal sources must not be enumerated"); },
      pickRegion: async () => { throw new Error("Portal regions are selected by the native preview after Start"); },
      installVisualModel: async () => {}, installTranslationModel: async () => {},
      openSettings: () => {}, report: message => { fixture.error = message; }, stopAudio: async () => {},
      start: async selection => {
        fixture.starts.push(selection);
        fixture.state.status = { ...fixture.state.status, active: true, state: "starting", message: "Choose a source in the desktop sharing picker." };
        fixture.render();
        await new Promise(resolve => { fixture.dismiss = resolve; });
      },
      stop: async () => {
        fixture.stops++;
        fixture.state.status = { ...fixture.state.status, active: false, state: "stopped" };
        fixture.dismiss();
        fixture.render();
      }
    };
    fixture.render = () => fixture.panel.render(document.querySelector("#fixture"), fixture.state, fixture.actions);
    fixture.render();
  });
  assert.equal(await page.locator("#qa-source").count(), 0);
  assert.equal(await page.getByRole("button", { name: "Refresh sources" }).count(), 0);
  assert.deepEqual(await page.locator("#qa-source-mode option").evaluateAll(options => options.map(option => option.value)), ["applicationWindow", "display", "region"]);
  assert.equal(await page.locator("#qa-source-mode").inputValue(), "region");
  for (const [mode, kind] of [["region", "portalRegion"], ["display", "portalDisplay"], ["applicationWindow", "portalWindow"]]) {
    await page.locator("#qa-source-mode").selectOption(mode);
    assert.equal(await page.locator(".visual-start-button").isDisabled(), false);
    await page.locator(".visual-start-button").click();
    await page.locator(".visual-stop-button").waitFor();
    assert.equal(await page.locator(".visual-stop-button").isDisabled(), false, "Stop must work while the picker is pending");
    assert.deepEqual(await page.evaluate(() => window.fixture.starts.at(-1)), { kind });
    await page.locator(".visual-stop-button").click();
    await page.locator(".visual-start-button").waitFor();
    await page.waitForFunction(() => !document.querySelector(".visual-start-button").disabled);
  }
  assert.equal(await page.evaluate(() => window.fixture.stops), 3);
  assert.equal(await page.evaluate(() => window.fixture.error), undefined);
  for (const width of [1280, 400]) {
    await page.setViewportSize({ width, height: 900 });
    assert.deepEqual(await page.locator("#fixture button, #fixture select").evaluateAll(elements => elements.filter(element => {
      const rect = element.getBoundingClientRect(); return rect.left < -1 || rect.right > innerWidth + 1;
    }).map(element => element.textContent)), [], `clipped controls at ${width}`);
  }
  await page.goto(`${base}/visual-overlay.html?reader`, { waitUntil: "networkidle" });
  for (const width of [560, 300]) {
    await page.setViewportSize({ width, height: 360 });
    await page.evaluate(width => {
      window.__PROLLYGLOT_VISUAL_OVERLAY_PREVIEW__.setPresentation({
        sessionId: 1, runtimeRevision: 1, presentationRevision: 1000 - width,
        sourceWidth: 3840, sourceHeight: 2160, sourceLanguage: "zh", targetLanguage: "en", scanning: false,
        regions: Array.from({ length: 6 }, (_, index) => ({ trackId: index + 1, textRevision: 1, original: "你好世界", translation: `Translation ${index + 1}: a long line that must wrap and stay reachable in a small reader.`, translationPending: false, bounds: { x: 3400, y: 2000, width: 400, height: 100 } }))
      });
    }, width);
    assert.equal(await page.locator(".visual-translation-label").count(), 6);
    assert.equal(await page.locator(".visual-reader-header button").isVisible(), true);
    assert(await page.locator("#visual-label-layer").evaluate(element => element.scrollHeight > element.clientHeight));
    assert(await page.locator(".visual-translation-label").first().evaluate(element => {
      const rect = element.getBoundingClientRect(); return rect.left >= 0 && rect.right <= innerWidth && rect.top < innerHeight;
    }));
    assert(!(await page.locator("#visual-label-layer").innerText()).includes("你好世界"), "source text must not be redrawn into the captured monitor");
    // Late results from an earlier session cannot restore old labels.
    await page.evaluate(() => window.__PROLLYGLOT_VISUAL_OVERLAY_PREVIEW__.setPresentation({ sessionId: 0, runtimeRevision: 0, presentationRevision: 9999, regions: [] }));
    assert.equal(await page.locator(".visual-translation-label").count(), 6);
  }
  await page.evaluate(() => {
    const frame = window.placementFixture = {
      sessionId: 2, runtimeRevision: 4, presentationRevision: 1,
      sourceWidth: 1000, sourceHeight: 200, sourceLanguage: "zh", targetLanguage: "en",
      scanning: false, anchored: true,
      regions: [{ trackId: 1, textRevision: 1, original: "你好", translation: "Hello",
        translationPending: false, bounds: { x: 400, y: 100, width: 160, height: 30 } }]
    };
    window.__PROLLYGLOT_VISUAL_OVERLAY_PREVIEW__.setPresentation(frame);
  });
  assert.equal(await page.locator(".visual-reader-header").count(), 0);
  assert.equal(await page.locator(".visual-translation-label").evaluate(el => getComputedStyle(el).position), "absolute");
  await page.evaluate(() => window.__PROLLYGLOT_VISUAL_OVERLAY_PREVIEW__.setPresentation({ ...window.placementFixture, anchored: false }));
  assert.equal(await page.locator(".visual-reader-header").count(), 1, "native fallback must apply without a new translation revision");
  await page.evaluate(() => window.__PROLLYGLOT_VISUAL_OVERLAY_PREVIEW__.setPresentation({ ...window.placementFixture, runtimeRevision: 3, anchored: true }));
  assert.equal(await page.locator(".visual-reader-header").count(), 1, "stale placement cannot restore an old anchor");
  assert.deepEqual(errors, []);
  console.log("Portal UI checks passed: explicit source kinds, cancellable picker UI, reader layout, stale results, and 300–1280 px controls.");
} finally {
  await browser.close();
}
