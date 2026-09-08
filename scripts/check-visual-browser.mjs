import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";

// Reuse a developer's Playwright installation; no browser dependency ships.
const playwright = process.env.PROLLYGLOT_PLAYWRIGHT_MODULE;
const { chromium } = await import(playwright ? pathToFileURL(resolve(playwright)).href : "playwright");
const base = process.argv[2] ?? "http://localhost:1420";
const artifacts = resolve(process.argv[3] ?? "target/visual-evaluation");
await mkdir(artifacts, { recursive: true });
const browser = await chromium.launch({ headless: true, executablePath: process.env.PROLLYGLOT_CHROMIUM });
try {
  const page = await browser.newPage();
  const errors = [];
  page.on("pageerror", error => errors.push(error.message));
  await page.goto(base, { waitUntil: "networkidle" });
  await page.getByRole("button", { name: "Screen translation", exact: true }).click();
  for (const width of [1280, 420, 400]) {
    await page.setViewportSize({ width, height: 800 });
    const clipped = await page.evaluate(() => [...document.querySelectorAll('.main-window button, .main-window select, .workspace-heading')]
      .filter(e => e.getClientRects().length && !e.closest('[hidden]'))
      .filter(e => { const r = e.getBoundingClientRect(); return r.left < -1 || r.right > innerWidth + 1; })
      .map(e => e.textContent));
    assert.deepEqual(clipped, [], `clipped controls at ${width}px`);
    await page.getByRole("button", { name: "Screen translation", exact: true }).click();
    await page.screenshot({ path: resolve(artifacts, `setup-${width}.png`) });
  }
  const sourceOptions = await page.locator('select[id$="source-language"] option').evaluateAll(options => options.map(o => o.value));
  assert(sourceOptions.includes("zh") && sourceOptions.includes("es"));
  assert(!sourceOptions.includes("ar") && !sourceOptions.includes("ko"));
  await page.goto(`${base}/visual-overlay.html`, { waitUntil: "networkidle" });
  let revision = 0;
  for (const width of [1280, 420, 240]) {
    await page.setViewportSize({ width, height: 720 });
    for (const [x, y] of [[0, 0], [1800, 0], [0, 1000], [1800, 1000]]) {
      const box = await page.evaluate(({ x, y, revision }) => {
        window.__PROLLYGLOT_VISUAL_OVERLAY_PREVIEW__.setPresentation({
          sessionId: 1, runtimeRevision: 1, presentationRevision: revision,
          sourceWidth: 1920, sourceHeight: 1080, sourceLanguage: 'es', targetLanguage: 'en', scanning: false,
          regions: [{ trackId: 1, textRevision: 1, original: 'Buenos días',
            translation: 'This translated label must remain readable near every edge of the captured area.',
            translationPending: false, retained: false, bounds: { x, y, width: 120, height: 50 } }]
        });
        const r = document.querySelector('.visual-translation-label').getBoundingClientRect();
        return { x: r.x, y: r.y, right: r.right, bottom: r.bottom, width: innerWidth, height: innerHeight };
      }, { x, y, revision: ++revision });
      assert(box.x >= 0 && box.y >= 0 && box.right <= box.width && box.bottom <= box.height, JSON.stringify(box));
    }
    await page.screenshot({ path: resolve(artifacts, `overlay-${width}.png`) });
  }
  assert.deepEqual(errors, []);
  const fixture = new URL("../docs/testing/fixtures/visual-subtitles.html", import.meta.url);
  const manifest = {};
  async function saveFixture(name, crop = null) {
    const subtitle = page.locator('#subtitle');
    const expectedText = await subtitle.innerText();
    const bounds = await subtitle.boundingBox();
    if (crop) {
      const origin = await crop.boundingBox();
      bounds.x -= origin.x; bounds.y -= origin.y;
    }
    manifest[`${name}.png`] = { expectedText, bounds };
    await (crop ?? page).screenshot({ path: resolve(artifacts, `${name}.png`) });
  }
  for (const language of ["zh", "es"]) for (const width of [1920, 3840]) for (const size of [24, 48]) {
    await page.setViewportSize({ width, height: width * 9 / 16 });
    fixture.search = new URLSearchParams({ fixture: "1", language, size });
    await page.goto(fixture.href);
    await page.evaluate(() => document.fonts.ready);
    const name = `${language}-${width}-${size}`;
    await saveFixture(name);
    await saveFixture(`${name}-region`, page.locator('#subtitle-area'));
  }
  // Detection must work beyond the usual subtitle band and through tile seams.
  for (const language of ["zh", "es"]) {
    fixture.search = new URLSearchParams({ fixture: "1", language, size: "24" });
    await page.setViewportSize({ width: 3840, height: 2160 });
    await page.goto(fixture.href);
    for (const name of ['top-left', 'top-right', 'bottom-left', 'bottom-right', 'seam']) {
      await page.locator('#subtitle-area').evaluate((element, name) => {
        const text = document.querySelector('#subtitle').getBoundingClientRect();
        const x = name === 'seam' ? 960 - text.width / 2 : name.endsWith('right') ? innerWidth - text.width - 8 : 8;
        const y = name === 'seam' ? 544 - text.height / 2 : name.startsWith('bottom') ? innerHeight - text.height - 8 : 8;
        Object.assign(element.style, { width: 'max-content', height: 'max-content', left: `${x}px`, top: `${y}px`, bottom: 'auto', transform: 'none' });
      }, name);
      await saveFixture(`${language}-3840-24-${name}`);
    }
    for (const text of language === 'zh' ? ['猫'] : ['No', 'Sí', 'OK']) {
      await page.locator('#subtitle').evaluate((element, text) => { element.textContent = text; }, text);
      await saveFixture(`${language}-3840-24-short-${text}`);
    }
    await page.reload();
    // A fully textured background forces bounded overlapping detection instead
    // of allowing the sparse-contrast proposal to isolate the subtitle.
    await page.locator('body').evaluate(element => {
      element.style.background = 'repeating-linear-gradient(35deg, #16383c 0px, #305058 14px, #16383c 28px)';
    });
    await saveFixture(`${language}-3840-24-textured`);
  }
  await writeFile(resolve(artifacts, 'manifest.json'), JSON.stringify(manifest, null, 2) + '\n');
  console.log(`Visual browser checks passed. Chinese/Spanish OCR fixtures: ${artifacts}`);
} finally { await browser.close(); }
