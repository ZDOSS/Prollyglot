import "./styles.css";

import {
  closeAppearance,
  reportFrontendDiagnostic,
  startWindowDrag,
  updateOverlaySettings,
  windowAction
} from "./bridge";
import { icons } from "./icons";
import { DEFAULT_OVERLAY_SETTINGS, type OverlaySettings } from "./types";

const root = document.querySelector<HTMLElement>("#appearance-app");
if (!root) throw new Error("missing appearance root");

root.innerHTML = `
  <section class="app-window appearance-window" aria-label="Caption appearance">
    <header class="titlebar compact-titlebar">
      <div class="brand compact-brand">
        <img class="brand-mark compact-mark" src="/branding/prollyglot-mark.png" alt="" />
        <span class="brand-name compact-name">Prollyglot</span>
      </div>
      <div class="window-controls" aria-label="Window controls">
        <button class="window-control" type="button" data-window-action="minimize" aria-label="Minimize">${icons.minimize}</button>
        <button class="window-control" type="button" data-window-action="maximize" aria-label="Maximize">${icons.maximize}</button>
        <button class="window-control close" type="button" data-window-action="close" aria-label="Close">${icons.close}</button>
      </div>
    </header>

    <div class="appearance-layout">
      <section class="appearance-controls" aria-labelledby="appearance-title">
        <h1 id="appearance-title">Appearance</h1>
        <h2>Caption style</h2>

        <label class="setting-row">
          <span>Font</span>
          <span class="compact-select-wrap">
            <select id="font-family">
              <option value='"Segoe UI Variable", "Segoe UI", sans-serif'>Segoe UI</option>
              <option value='Inter, "Segoe UI", sans-serif'>Inter</option>
              <option value='Arial, sans-serif'>Arial</option>
            </select>${icons.chevronDown}
          </span>
        </label>

        <label class="setting-row">
          <span>Size</span>
          <span class="compact-select-wrap">
            <select id="font-size">
              <option value="28">28 px</option>
              <option value="36">36 px</option>
              <option value="44">44 px</option>
              <option value="56">56 px</option>
            </select>${icons.chevronDown}
          </span>
        </label>

        <label class="setting-row">
          <span>Original color</span>
          <input id="text-color" class="color-control" type="color" value="#f4f6f5" aria-label="Original caption color" />
        </label>

        <label class="setting-row">
          <span>English color</span>
          <input id="translated-text-color" class="color-control" type="color" value="#86e3b0" aria-label="English translation color" />
        </label>

        <label class="setting-row">
          <span>Original + English layout</span>
          <span class="compact-select-wrap">
            <select id="bilingual-layout">
              <option value="stacked">Stacked</option>
              <option value="sideBySide">Side by side</option>
            </select>${icons.chevronDown}
          </span>
        </label>
        <p class="appearance-help">This controls the preview layout only. Turn translation on with <strong>Caption output → Original + English</strong> in the main window.</p>

        <label class="range-setting">
          <span class="range-label"><span>Background opacity</span><output id="opacity-output">75%</output></span>
          <input id="background-opacity" type="range" min="0" max="100" step="5" value="75" aria-label="Background opacity" />
        </label>

        <label class="setting-row">
          <span>Width</span>
          <span class="compact-select-wrap">
            <select id="overlay-width">
              <option value="520">520 px</option>
              <option value="720">720 px</option>
              <option value="920">920 px</option>
            </select>${icons.chevronDown}
          </span>
        </label>

        <label class="setting-row">
          <span>Caption history</span>
          <span class="compact-select-wrap">
            <select id="maximum-lines">
              <option value="1">Current only</option>
              <option value="2">1 previous line</option>
              <option value="3">2 previous lines</option>
              <option value="4">3 previous lines</option>
            </select>${icons.chevronDown}
          </span>
        </label>
        <p class="appearance-help">Recent finalized captions fade above the current line. Long current captions may use the extra wrapping space first.</p>

        <label class="setting-row">
          <span>Position</span>
          <span class="compact-select-wrap">
            <select id="overlay-position">
              <option value="bottomCenter">Bottom center</option>
              <option value="topCenter">Top center</option>
              <option value="bottomLeft">Bottom left</option>
              <option value="bottomRight">Bottom right</option>
            </select>${icons.chevronDown}
          </span>
        </label>

        <label class="setting-row toggle-row">
          <span>Click-through</span>
          <input id="click-through" class="toggle-input" type="checkbox" checked aria-label="Click-through" />
          <span class="toggle-visual" aria-hidden="true"><span></span></span>
        </label>
      </section>

      <section class="preview-canvas" aria-label="Original and English caption appearance preview">
        <div class="preview-desktop" id="preview-desktop">
          <div class="preview-caption" id="preview-caption">
            <span class="preview-caption-entry">
              <span class="preview-caption-original" lang="ja">昨日から雨が続いています。</span>
              <span class="preview-caption-translation" lang="en">It has been raining since yesterday.</span>
            </span>
            <span class="preview-caption-entry">
              <span class="preview-caption-original" lang="ja">午後には晴れる見込みです。</span>
              <span class="preview-caption-translation" lang="en">It should clear this afternoon.</span>
            </span>
            <span class="preview-caption-entry">
              <span class="preview-caption-original" lang="ja">電車は通常どおり運行しています。</span>
              <span class="preview-caption-translation" lang="en">Trains are running normally.</span>
            </span>
            <span class="preview-caption-entry">
              <span class="preview-caption-original" lang="ja">今日は何をする予定ですか？</span>
              <span class="preview-caption-translation" lang="en">What are you planning to do today?</span>
            </span>
          </div>
          <div class="preview-taskbar" aria-hidden="true"><span class="windows-mark">⊞</span><span class="taskbar-spacer"></span><span>10:28 AM</span></div>
        </div>
      </section>
    </div>

    <footer class="appearance-actions">
      <button id="reset-appearance" class="secondary-button" type="button">Reset</button>
      <button id="done-appearance" class="primary-button compact-primary" type="button">Done</button>
    </footer>
  </section>
`;

const preview = requireElement<HTMLElement>("#preview-caption");
const opacityOutput = requireElement<HTMLOutputElement>("#opacity-output");

function requireElement<T extends Element>(selector: string): T {
  const element = document.querySelector<T>(selector);
  if (!element) throw new Error(`missing element: ${selector}`);
  return element;
}

function readStoredSettings(): OverlaySettings {
  const stored = localStorage.getItem("prollyglot.overlay");
  if (!stored) return { ...DEFAULT_OVERLAY_SETTINGS };
  try {
    return { ...DEFAULT_OVERLAY_SETTINGS, ...(JSON.parse(stored) as Partial<OverlaySettings>) };
  } catch {
    return { ...DEFAULT_OVERLAY_SETTINGS };
  }
}

function readSettings(): OverlaySettings {
  return {
    fontFamily: requireElement<HTMLSelectElement>("#font-family").value,
    fontSize: Number(requireElement<HTMLSelectElement>("#font-size").value),
    textColor: requireElement<HTMLInputElement>("#text-color").value,
    translatedTextColor: requireElement<HTMLInputElement>("#translated-text-color").value,
    bilingualLayout: requireElement<HTMLSelectElement>("#bilingual-layout").value as OverlaySettings["bilingualLayout"],
    backgroundOpacity: Number(requireElement<HTMLInputElement>("#background-opacity").value) / 100,
    width: Number(requireElement<HTMLSelectElement>("#overlay-width").value),
    maximumLines: Number(requireElement<HTMLSelectElement>("#maximum-lines").value),
    position: requireElement<HTMLSelectElement>("#overlay-position").value as OverlaySettings["position"],
    clickThrough: requireElement<HTMLInputElement>("#click-through").checked
  };
}

function writeSettings(settings: OverlaySettings) {
  requireElement<HTMLSelectElement>("#font-family").value = settings.fontFamily;
  requireElement<HTMLSelectElement>("#font-size").value = String(settings.fontSize);
  requireElement<HTMLInputElement>("#text-color").value = settings.textColor;
  requireElement<HTMLInputElement>("#translated-text-color").value = settings.translatedTextColor;
  requireElement<HTMLSelectElement>("#bilingual-layout").value = settings.bilingualLayout;
  requireElement<HTMLInputElement>("#background-opacity").value = String(settings.backgroundOpacity * 100);
  requireElement<HTMLSelectElement>("#overlay-width").value = String(settings.width);
  requireElement<HTMLSelectElement>("#maximum-lines").value = String(settings.maximumLines);
  requireElement<HTMLSelectElement>("#overlay-position").value = settings.position;
  requireElement<HTMLInputElement>("#click-through").checked = settings.clickThrough;
  renderPreview(settings);
}

function renderPreview(settings: OverlaySettings) {
  opacityOutput.value = `${Math.round(settings.backgroundOpacity * 100)}%`;
  preview.style.fontFamily = settings.fontFamily;
  preview.style.fontSize = `${Math.max(18, settings.fontSize * 0.72)}px`;
  preview.style.backgroundColor = `rgba(11, 15, 18, ${settings.backgroundOpacity})`;
  preview.style.maxWidth = `${Math.min(720, settings.width * 0.78)}px`;
  preview.style.setProperty("--maximum-lines", String(settings.maximumLines));
  preview.style.setProperty("--source-caption-color", settings.textColor);
  preview.style.setProperty("--translated-caption-color", settings.translatedTextColor);
  preview.dataset.bilingualLayout = settings.bilingualLayout;
  const previewEntries = [...preview.querySelectorAll<HTMLElement>(".preview-caption-entry")];
  const firstVisible = Math.max(0, previewEntries.length - settings.maximumLines);
  previewEntries.forEach((entry, index) => {
    entry.hidden = index < firstVisible;
    entry.dataset.historyDepth = String(previewEntries.length - index - 1);
  });

  const desktop = requireElement<HTMLElement>("#preview-desktop");
  desktop.dataset.position = settings.position;
  void updateOverlaySettings(settings);
}

for (const control of document.querySelectorAll<HTMLInputElement | HTMLSelectElement>(".appearance-controls input, .appearance-controls select")) {
  control.addEventListener("input", () => renderPreview(readSettings()));
}

let dismissing = false;

async function dismissAppearance() {
  if (dismissing) return;
  dismissing = true;
  const dismissButtons = document.querySelectorAll<HTMLButtonElement>("#done-appearance, [data-window-action='close']");
  for (const button of dismissButtons) {
    button.disabled = true;
  }

  try {
    await updateOverlaySettings(readSettings());
  } catch (error) {
    console.error("Could not save caption appearance before closing.", error);
  }

  try {
    await closeAppearance();
  } catch (error) {
    console.error("Could not hide Appearance; closing the window instead.", error);
    try {
      await windowAction("close");
    } catch (closeError) {
      const message = closeError instanceof Error ? closeError.message : String(closeError);
      console.error("Could not close Appearance.", closeError);
      void reportFrontendDiagnostic("window-control", `close appearance: ${message}`);
    }
  } finally {
    dismissing = false;
    for (const button of dismissButtons) {
      button.disabled = false;
    }
  }
}

for (const button of document.querySelectorAll<HTMLButtonElement>("[data-window-action]")) {
  button.addEventListener("click", () => {
    const action = button.dataset.windowAction;
    if (action === "close") {
      void dismissAppearance();
    } else if (action === "minimize" || action === "maximize") {
      void windowAction(action).catch((error) => {
        const message = error instanceof Error ? error.message : String(error);
        console.error(`Window ${action} failed.`, error);
        void reportFrontendDiagnostic("window-control", `${action}: ${message}`);
      });
    }
  });
}

requireElement<HTMLElement>(".titlebar").addEventListener("mousedown", (event) => {
  if (event.button !== 0) return;
  const target = event.target;
  if (target instanceof Element && target.closest("button, input, select, a")) return;
  const action = event.detail === 2 ? "maximize" : "drag";
  const operation = event.detail === 2 ? windowAction("maximize") : startWindowDrag();
  void operation.catch((error) => {
    const message = error instanceof Error ? error.message : String(error);
    console.error(`Window ${action} failed.`, error);
    void reportFrontendDiagnostic("window-control", `${action}: ${message}`);
  });
});

requireElement<HTMLButtonElement>("#reset-appearance").addEventListener("click", () => writeSettings({ ...DEFAULT_OVERLAY_SETTINGS }));
requireElement<HTMLButtonElement>("#done-appearance").addEventListener("click", () => void dismissAppearance());
document.addEventListener("keydown", (event) => {
  if (event.key === "Escape") void dismissAppearance();
});

writeSettings(readStoredSettings());
