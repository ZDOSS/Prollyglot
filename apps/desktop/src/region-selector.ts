import "./styles.css";

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import { isTauri } from "./bridge";
import type { PixelRect } from "./types";

interface RegionSelectorRequest {
  displayId: string;
  width: number;
  height: number;
}

interface Point {
  x: number;
  y: number;
}

declare global {
  interface Window {
    __PROLLYGLOT_REGION_SELECTOR_PREVIEW__?: {
      show: (request: RegionSelectorRequest) => void;
      selected?: PixelRect;
    };
  }
}

const MINIMUM_REGION_WIDTH = 80;
const MINIMUM_REGION_HEIGHT = 60;

function required<T extends Element>(selector: string): T {
  const element = document.querySelector<T>(selector);
  if (!element) throw new Error(`missing region selector element: ${selector}`);
  return element;
}

const root = required<HTMLElement>("#region-selector-app");

root.innerHTML = `
  <section class="region-selector" aria-label="Select a screen region" tabindex="-1">
    <div class="region-selector-help">
      <strong>Drag around the text to translate</strong>
      <span>Press Esc to cancel</span>
    </div>
    <div id="region-selection-box" class="region-selection-box" hidden></div>
    <p id="region-selection-size" class="region-selection-size" aria-live="polite"></p>
    <button id="region-selection-cancel" class="region-selection-cancel" type="button">Cancel</button>
  </section>
`;

const surface = required<HTMLElement>(".region-selector");
const box = required<HTMLElement>("#region-selection-box");
const size = required<HTMLElement>("#region-selection-size");
const cancel = required<HTMLButtonElement>("#region-selection-cancel");

let request: RegionSelectorRequest | undefined;
let origin: Point | undefined;
let current: Point | undefined;
let pointerId: number | undefined;

function clampedPoint(event: PointerEvent): Point {
  return {
    x: Math.max(0, Math.min(window.innerWidth, event.clientX)),
    y: Math.max(0, Math.min(window.innerHeight, event.clientY))
  };
}

function cssRect(): PixelRect | undefined {
  if (!origin || !current) return undefined;
  const x = Math.min(origin.x, current.x);
  const y = Math.min(origin.y, current.y);
  return {
    x: Math.round(x),
    y: Math.round(y),
    width: Math.round(Math.abs(current.x - origin.x)),
    height: Math.round(Math.abs(current.y - origin.y))
  };
}

function physicalRect(rect: PixelRect): PixelRect {
  if (!request) return rect;
  const scaleX = request.width / Math.max(1, window.innerWidth);
  const scaleY = request.height / Math.max(1, window.innerHeight);
  return {
    x: Math.round(rect.x * scaleX),
    y: Math.round(rect.y * scaleY),
    width: Math.round(rect.width * scaleX),
    height: Math.round(rect.height * scaleY)
  };
}

function renderSelection(): void {
  const rect = cssRect();
  box.hidden = !rect;
  if (!rect) {
    size.textContent = "";
    return;
  }
  box.style.left = `${rect.x}px`;
  box.style.top = `${rect.y}px`;
  box.style.width = `${rect.width}px`;
  box.style.height = `${rect.height}px`;
  const physical = physicalRect(rect);
  size.textContent = `${physical.width} × ${physical.height} px`;
  size.style.left = `${Math.max(12, Math.min(window.innerWidth - 132, rect.x))}px`;
  size.style.top = `${Math.max(12, Math.min(window.innerHeight - 44, rect.y + rect.height + 10))}px`;
}

function reset(next?: RegionSelectorRequest): void {
  request = next;
  origin = undefined;
  current = undefined;
  pointerId = undefined;
  renderSelection();
  surface.focus();
}

async function cancelSelection(): Promise<void> {
  reset();
  if (!isTauri()) return;
  try {
    await invoke("cancel_visual_region_selection");
  } catch (error) {
    size.textContent = error instanceof Error ? error.message : String(error);
  }
}

surface.addEventListener("pointerdown", (event) => {
  if (!request || event.button !== 0) return;
  if (event.target instanceof Element && event.target.closest("button")) return;
  pointerId = event.pointerId;
  surface.setPointerCapture(event.pointerId);
  origin = clampedPoint(event);
  current = origin;
  renderSelection();
});

surface.addEventListener("pointermove", (event) => {
  if (pointerId !== event.pointerId || !origin) return;
  current = clampedPoint(event);
  renderSelection();
});

surface.addEventListener("pointercancel", () => {
  const activeRequest = request;
  reset(activeRequest);
});

surface.addEventListener("pointerup", async (event) => {
  if (pointerId !== event.pointerId || !request) return;
  current = clampedPoint(event);
  const selected = cssRect();
  const activeRequest = request;
  reset();
  if (!selected) return;
  const region = physicalRectForRequest(selected, activeRequest);
  if (region.width < MINIMUM_REGION_WIDTH || region.height < MINIMUM_REGION_HEIGHT) {
    request = activeRequest;
    size.textContent = `Select at least ${MINIMUM_REGION_WIDTH} × ${MINIMUM_REGION_HEIGHT} px`;
    return;
  }
  if (!isTauri()) {
    if (window.__PROLLYGLOT_REGION_SELECTOR_PREVIEW__) {
      window.__PROLLYGLOT_REGION_SELECTOR_PREVIEW__.selected = region;
    }
    return;
  }
  try {
    await invoke("complete_visual_region_selection", {
      displayId: activeRequest.displayId,
      region
    });
  } catch (error) {
    request = activeRequest;
    size.textContent = error instanceof Error ? error.message : String(error);
  }
});

function physicalRectForRequest(rect: PixelRect, activeRequest: RegionSelectorRequest): PixelRect {
  const scaleX = activeRequest.width / Math.max(1, window.innerWidth);
  const scaleY = activeRequest.height / Math.max(1, window.innerHeight);
  return {
    x: Math.round(rect.x * scaleX),
    y: Math.round(rect.y * scaleY),
    width: Math.round(rect.width * scaleX),
    height: Math.round(rect.height * scaleY)
  };
}

cancel.addEventListener("click", () => void cancelSelection());
window.addEventListener("keydown", (event) => {
  if (event.key !== "Escape") return;
  event.preventDefault();
  void cancelSelection();
});

if (isTauri()) {
  void listen<RegionSelectorRequest>("visual-region-selector-request", ({ payload }) => reset(payload));
} else {
  window.__PROLLYGLOT_REGION_SELECTOR_PREVIEW__ = { show: reset };
  reset({ displayId: "preview", width: window.innerWidth, height: window.innerHeight });
}
