import "./styles.css";

import { listen } from "@tauri-apps/api/event";

import { isTauri } from "./bridge";
import { DEFAULT_OVERLAY_SETTINGS, type OverlaySettings } from "./types";

const root = document.querySelector<HTMLElement>("#overlay-app");
if (!root) throw new Error("missing overlay root");

root.innerHTML = `
  <div id="caption-surface" class="caption-surface" role="status" aria-live="polite" data-tauri-drag-region hidden>
    <span id="caption-text" class="caption-text"></span>
  </div>
`;

function requireElement<T extends Element>(selector: string): T {
  const element = document.querySelector<T>(selector);
  if (!element) throw new Error(`missing element: ${selector}`);
  return element;
}

const surface = requireElement<HTMLElement>("#caption-surface");
const captionText = requireElement<HTMLElement>("#caption-text");

function renderCaption(caption: string) {
  const lines = caption
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean);
  captionText.replaceChildren(
    ...lines.map((line) => {
      const element = document.createElement("span");
      element.className = "caption-line";
      element.textContent = line;
      return element;
    })
  );
  surface.hidden = lines.length === 0;
}

function applySettings(settings: OverlaySettings) {
  surface.style.fontFamily = settings.fontFamily;
  surface.style.fontSize = `${settings.fontSize}px`;
  surface.style.color = settings.textColor;
  surface.style.backgroundColor = `rgba(11, 15, 18, ${settings.backgroundOpacity})`;
  surface.style.maxWidth = `${settings.width}px`;
  surface.style.setProperty("--maximum-lines", String(settings.maximumLines));
  surface.dataset.clickThrough = String(settings.clickThrough);
}

function storedSettings(): OverlaySettings {
  try {
    const stored = localStorage.getItem("prollyglot.overlay");
    return stored ? { ...DEFAULT_OVERLAY_SETTINGS, ...(JSON.parse(stored) as Partial<OverlaySettings>) } : DEFAULT_OVERLAY_SETTINGS;
  } catch {
    return DEFAULT_OVERLAY_SETTINGS;
  }
}

applySettings(storedSettings());
if (!isTauri()) renderCaption("We should be there in about ten minutes.");

if (isTauri()) {
  void listen<string>("overlay-caption", ({ payload }) => {
    renderCaption(payload);
  });
  void listen<OverlaySettings>("overlay-settings", ({ payload }) => applySettings(payload));
}
