import "./styles.css";

import {
  closeAppearance,
  hideOverlayPreview,
  updateOverlaySettings,
  windowAction
} from "./bridge";
import { icons } from "./icons";
import { DEFAULT_OVERLAY_SETTINGS, type OverlaySettings } from "./types";

const root = document.querySelector<HTMLElement>("#appearance-app");
if (!root) throw new Error("missing appearance root");

root.innerHTML = `
  <section class="app-window appearance-window" aria-label="Caption appearance">
    <header class="titlebar compact-titlebar" data-tauri-drag-region>
      <div class="brand compact-brand" data-tauri-drag-region>
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
          <span>Text color</span>
          <input id="text-color" class="color-control" type="color" value="#f4f6f5" aria-label="Text color" />
        </label>

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
          <span>Maximum lines</span>
          <span class="compact-select-wrap">
            <select id="maximum-lines">
              <option value="1">1</option>
              <option value="2">2</option>
              <option value="3">3</option>
            </select>${icons.chevronDown}
          </span>
        </label>

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

      <section class="preview-canvas" aria-label="Live caption preview">
        <div class="preview-desktop" id="preview-desktop">
          <div class="preview-caption" id="preview-caption">We should be there in about ten minutes.</div>
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
  preview.style.color = settings.textColor;
  preview.style.backgroundColor = `rgba(11, 15, 18, ${settings.backgroundOpacity})`;
  preview.style.maxWidth = `${Math.min(720, settings.width * 0.78)}px`;
  preview.style.setProperty("--maximum-lines", String(settings.maximumLines));

  const desktop = requireElement<HTMLElement>("#preview-desktop");
  desktop.dataset.position = settings.position;
  void updateOverlaySettings(settings);
}

for (const control of document.querySelectorAll<HTMLInputElement | HTMLSelectElement>(".appearance-controls input, .appearance-controls select")) {
  control.addEventListener("input", () => renderPreview(readSettings()));
}

for (const button of document.querySelectorAll<HTMLButtonElement>("[data-window-action]")) {
  button.addEventListener("click", () => {
    const action = button.dataset.windowAction;
    if (action === "close") {
      void hideOverlayPreview().then(closeAppearance);
    } else if (action === "minimize" || action === "maximize") {
      void windowAction(action);
    }
  });
}

requireElement<HTMLButtonElement>("#reset-appearance").addEventListener("click", () => writeSettings({ ...DEFAULT_OVERLAY_SETTINGS }));
requireElement<HTMLButtonElement>("#done-appearance").addEventListener("click", async () => {
  await updateOverlaySettings(readSettings());
  await hideOverlayPreview();
  await closeAppearance();
});

writeSettings(readStoredSettings());
