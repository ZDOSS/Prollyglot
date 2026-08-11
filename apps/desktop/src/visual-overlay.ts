import "./styles.css";

import { listen } from "@tauri-apps/api/event";

import { isTauri } from "./bridge";
import type { VisualOutputPayload, VisualOutputRegion } from "./types";

function required<T extends Element>(selector: string): T {
  const element = document.querySelector<T>(selector);
  if (!element) throw new Error(`missing visual overlay element: ${selector}`);
  return element;
}

const root = required<HTMLElement>("#visual-overlay-app");

root.innerHTML = `<div id="visual-label-layer" class="visual-label-layer" aria-live="polite"></div>`;
const layer = required<HTMLElement>("#visual-label-layer");

let output: VisualOutputPayload = {
  sourceWidth: 1,
  sourceHeight: 1,
  sourceLanguage: "",
  targetLanguage: "",
  scanning: false,
  regions: []
};

declare global {
  interface Window {
    __PROLLYGLOT_VISUAL_OVERLAY_PREVIEW__?: {
      setOutput: (payload: VisualOutputPayload) => void;
    };
  }
}

function line(text: string, className: string, language: string): HTMLElement {
  const copy = document.createElement("span");
  copy.className = className;
  copy.lang = language;
  copy.textContent = text;
  return copy;
}

function labelFor(region: VisualOutputRegion): HTMLElement {
  const label = document.createElement("div");
  label.className = "visual-translation-label";
  label.dataset.trackId = String(region.trackId);
  label.dataset.pending = String(region.translationPending);
  label.dataset.retained = String(Boolean(region.retained));
  label.title = region.original;
  label.setAttribute(
    "aria-label",
    region.translation
      ? `${region.original}. Translation: ${region.translation}`
      : `${region.original}. Translation pending.`
  );

  const center = ((region.bounds.x + region.bounds.width / 2) / output.sourceWidth) * 100;
  const top = (region.bounds.y / output.sourceHeight) * 100;
  const bottom = ((region.bounds.y + region.bounds.height) / output.sourceHeight) * 100;
  const sourceWidth = (region.bounds.width / output.sourceWidth) * 100;
  const placeBelow = top < 14;
  label.dataset.placement = placeBelow ? "below" : "above";
  label.style.left = `${Math.max(8, Math.min(92, center))}%`;
  label.style.top = `${placeBelow ? bottom : top}%`;
  label.style.setProperty("--visual-source-width", `${Math.max(14, Math.min(72, sourceWidth))}vw`);

  if (region.translation) {
    label.append(line(region.translation, "visual-translated-copy", output.targetLanguage));
  } else {
    label.append(line(
      region.translationPending ? "Translating…" : "Translation unavailable",
      "visual-translation-state",
      output.targetLanguage
    ));
  }
  return label;
}

function render(): void {
  const labels = output.regions.map(labelFor);
  if (output.scanning && labels.length === 0) {
    const scanning = document.createElement("div");
    scanning.className = "visual-scanning-state";
    scanning.setAttribute("role", "status");
    scanning.textContent = "Scanning for text…";
    labels.push(scanning);
  }
  layer.replaceChildren(...labels);
}

function setOutput(next: VisualOutputPayload): void {
  output = structuredClone(next);
  render();
}

if (isTauri()) {
  void listen<VisualOutputPayload>("visual-overlay-output", ({ payload }) => setOutput(payload));
} else {
  window.__PROLLYGLOT_VISUAL_OVERLAY_PREVIEW__ = { setOutput };
}
