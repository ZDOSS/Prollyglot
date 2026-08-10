import "./styles.css";

import { listen } from "@tauri-apps/api/event";

import { isTauri } from "./bridge";
import {
  DEFAULT_OVERLAY_SETTINGS,
  type CaptionOutputEntry,
  type CaptionOutputPayload,
  type OverlaySettings
} from "./types";

const root = document.querySelector<HTMLElement>("#overlay-app");
if (!root) throw new Error("missing overlay root");

root.innerHTML = `
  <div id="caption-surface" class="caption-surface" role="status" aria-live="polite" data-tauri-drag-region hidden>
    <div id="caption-text" class="caption-text"></div>
  </div>
`;

function requireElement<T extends Element>(selector: string): T {
  const element = document.querySelector<T>(selector);
  if (!element) throw new Error(`missing element: ${selector}`);
  return element;
}

const surface = requireElement<HTMLElement>("#caption-surface");
const captionText = requireElement<HTMLElement>("#caption-text");
let rawCaption = "";
let captionActive = false;
let output: CaptionOutputPayload = { mode: "original", originalCaption: "", entries: [] };

function fallbackEntries(caption: string): CaptionOutputEntry[] {
  return caption
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean)
    .map((original, index) => ({
      key: `fallback:${index}`,
      sourceLanguage: "auto",
      original,
      isFinal: false
    }));
}

function captionLine(text: string, className: string, language: string): HTMLElement {
  const line = document.createElement("span");
  line.className = className;
  line.lang = language;
  line.textContent = text;
  return line;
}

function renderCaption(): void {
  const entries = output.originalCaption === rawCaption
    ? output.entries
    : fallbackEntries(rawCaption);
  const mode = output.originalCaption === rawCaption ? output.mode : "original";
  captionText.dataset.mode = mode;
  const rendered = entries.map((entry) => {
    const group = document.createElement("span");
    group.className = "caption-entry";
    if (mode === "original") {
      group.append(captionLine(
        entry.original,
        "caption-line caption-original",
        entry.sourceLanguage === "auto" ? "" : entry.sourceLanguage
      ));
    } else if (mode === "english" && entry.translation) {
      group.append(captionLine(entry.translation, "caption-line caption-translation", "en"));
    } else if (mode === "english") {
      group.classList.add("translation-pending");
      group.append(captionLine(
        entry.original,
        "caption-line caption-original caption-fallback",
        entry.sourceLanguage === "auto" ? "" : entry.sourceLanguage
      ));
    } else {
      group.append(captionLine(
        entry.original,
        "caption-line caption-original",
        entry.sourceLanguage === "auto" ? "" : entry.sourceLanguage
      ));
      if (entry.translation) {
        group.append(captionLine(entry.translation, "caption-line caption-translation", "en"));
      } else {
        group.classList.add("translation-pending");
      }
    }
    return group;
  });
  captionText.replaceChildren(...rendered);
  surface.hidden = !captionActive || rendered.length === 0;
}

function applySettings(settings: OverlaySettings) {
  surface.style.fontFamily = settings.fontFamily;
  surface.style.fontSize = `${settings.fontSize}px`;
  surface.style.setProperty("--source-caption-color", settings.textColor);
  surface.style.setProperty("--translated-caption-color", settings.translatedTextColor);
  surface.style.backgroundColor = `rgba(11, 15, 18, ${settings.backgroundOpacity})`;
  surface.style.maxWidth = `${settings.width}px`;
  surface.style.setProperty("--maximum-lines", String(settings.maximumLines));
  surface.dataset.clickThrough = String(settings.clickThrough);
  surface.dataset.bilingualLayout = settings.bilingualLayout;
  requireElement<HTMLElement>("#overlay-app").dataset.position = settings.position;
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
if (!isTauri()) {
  rawCaption = "今日は何をする予定ですか？";
  captionActive = true;
  output = {
    mode: "both",
    originalCaption: rawCaption,
    entries: [{
      key: "preview",
      sourceLanguage: "ja",
      original: rawCaption,
      translation: "What are you planning to do today?",
      isFinal: true
    }]
  };
  renderCaption();
}

if (isTauri()) {
  void listen<string>("overlay-caption", ({ payload }) => {
    rawCaption = payload;
    captionActive = payload.trim().length > 0;
    renderCaption();
  });
  void listen<CaptionOutputPayload>("caption-output", ({ payload }) => {
    output = payload;
    renderCaption();
  });
  void listen<OverlaySettings>("overlay-settings", ({ payload }) => applySettings(payload));
}
