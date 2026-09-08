import "./styles.css";

import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";

import { isTauri } from "./bridge";
import { PresentationCursor } from "./presentation-state";
import {
  RUNTIME_EVENTS,
  type VisualPresentationFrame,
  type VisualOverlayLayout,
  type VisualPresentationRegion
} from "./types";

function required<T extends Element>(selector: string): T {
  const element = document.querySelector<T>(selector);
  if (!element) throw new Error(`missing visual overlay element: ${selector}`);
  return element;
}

const root = required<HTMLElement>("#visual-overlay-app");

root.innerHTML = `<div id="visual-label-layer" class="visual-label-layer" aria-live="polite"></div>`;
const layer = required<HTMLElement>("#visual-label-layer");

let output: VisualPresentationFrame = {
  sessionId: 0,
  runtimeRevision: 0,
  presentationRevision: 0,
  sourceWidth: 1,
  sourceHeight: 1,
  sourceLanguage: "",
  targetLanguage: "",
  scanning: false,
  regions: []
};
const cursor = new PresentationCursor<VisualPresentationFrame>();

declare global {
  interface Window {
    __PROLLYGLOT_VISUAL_OVERLAY_PREVIEW__?: {
      setPresentation: (frame: VisualPresentationFrame) => void;
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

function labelFor(region: VisualPresentationRegion): HTMLElement {
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

  const sourceWidth = (region.bounds.width / output.sourceWidth) * 100;
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
  positionLabels();
}

function positionLabels(): void {
  const width = layer.clientWidth;
  const height = layer.clientHeight;
  if (!width || !height) return;
  const margin = Math.min(8, width / 8, height / 8);
  const layout: VisualOverlayLayout = {
    sessionId: output.sessionId,
    presentationRevision: output.presentationRevision,
    labels: []
  };
  for (const region of output.regions) {
    const label = layer.querySelector<HTMLElement>(`[data-track-id="${region.trackId}"]`);
    if (!label) continue;
    const size = label.getBoundingClientRect();
    const sourceTop = region.bounds.y / output.sourceHeight * height;
    const sourceBottom = (region.bounds.y + region.bounds.height) / output.sourceHeight * height;
    const center = (region.bounds.x + region.bounds.width / 2) / output.sourceWidth * width;
    const above = sourceTop - size.height - 7;
    const below = sourceBottom + 7;
    const preferred = above >= margin || height - below < sourceTop ? above : below;
    const x = Math.max(margin, Math.min(width - size.width - margin, center - size.width / 2));
    const y = Math.max(margin, Math.min(height - size.height - margin, preferred));
    label.style.left = `${x}px`;
    label.style.top = `${y}px`;
    layout.labels.push({
      trackId: region.trackId,
      textRevision: region.textRevision,
      bounds: { x: x / width * output.sourceWidth, y: y / height * output.sourceHeight,
        width: size.width / width * output.sourceWidth,
        height: size.height / height * output.sourceHeight }
    });
  }
  const scanning = layer.querySelector<HTMLElement>(".visual-scanning-state");
  if (scanning) {
    const rect = scanning.getBoundingClientRect();
    layout.labels.push({ trackId: 0, textRevision: 0, bounds: {
      x: rect.x / width * output.sourceWidth, y: rect.y / height * output.sourceHeight,
      width: rect.width / width * output.sourceWidth, height: rect.height / height * output.sourceHeight
    } });
  }
  if (isTauri()) {
    void invoke("update_visual_overlay_layout", { layout }).catch((error) => {
      console.warn("Could not report visual label positions", error);
    });
  }
}

window.addEventListener("resize", positionLabels);
void document.fonts.ready.then(positionLabels);

function setPresentation(next: VisualPresentationFrame): void {
  if (!cursor.accept(next)) return;
  output = structuredClone(next);
  render();
}

if (isTauri()) {
  void listen<VisualPresentationFrame>(
    RUNTIME_EVENTS.visualPresentation,
    ({ payload }) => setPresentation(payload)
  );
} else {
  window.__PROLLYGLOT_VISUAL_OVERLAY_PREVIEW__ = { setPresentation };
}
