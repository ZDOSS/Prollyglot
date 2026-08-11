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
  label.title = region.original;

  const center = ((region.bounds.x + region.bounds.width / 2) / output.sourceWidth) * 100;
  const top = (region.bounds.y / output.sourceHeight) * 100;
  const bottom = ((region.bounds.y + region.bounds.height) / output.sourceHeight) * 100;
  const sourceWidth = (region.bounds.width / output.sourceWidth) * 100;
  const placeBelow = top < 14;
  label.dataset.placement = placeBelow ? "below" : "above";
  label.style.left = `${Math.max(8, Math.min(92, center))}%`;
  label.style.top = `${placeBelow ? bottom : top}%`;
  label.style.setProperty("--visual-source-width", `${Math.max(14, Math.min(72, sourceWidth))}vw`);

  label.append(line(region.original, "visual-source-copy", output.sourceLanguage));
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
  layer.replaceChildren(...output.regions.map(labelFor));
}

function setOutput(next: VisualOutputPayload): void {
  output = structuredClone(next);
  render();
}

function clear(): void {
  output = { ...output, regions: [] };
  render();
}

if (isTauri()) {
  void listen<VisualOutputPayload>("visual-overlay-output", ({ payload }) => setOutput(payload));
  void listen("visual-text-clear", clear);
} else {
  window.__PROLLYGLOT_VISUAL_OVERLAY_PREVIEW__ = { setOutput };
}
