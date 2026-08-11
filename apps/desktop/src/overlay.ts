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
const TRANSLATION_MAX_WAIT_MS = 30_000;
const TRANSLATION_RESULT_HOLD_MS = 6_000;
let rawCaption = "";
let captionActive = false;
let output: CaptionOutputPayload = { mode: "original", originalCaption: "", entries: [] };
let overlaySettings: OverlaySettings = { ...DEFAULT_OVERLAY_SETTINGS };
let deferredClear = false;
let clearTimer: number | undefined;

declare global {
  interface Window {
    __PROLLYGLOT_OVERLAY_PREVIEW__?: {
      setCaption: (caption: string) => void;
      setOutput: (payload: CaptionOutputPayload) => void;
    };
  }
}

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

function translationStatus(entry: CaptionOutputEntry): string {
  if (!entry.isFinal) return "English after pause…";
  if (entry.translationPending) return "Translating…";
  return "English unavailable";
}

function markTranslationState(group: HTMLElement, entry: CaptionOutputEntry): void {
  group.classList.add(
    !entry.isFinal
      ? "translation-waiting"
      : entry.translationPending ? "translation-pending" : "translation-unavailable"
  );
}

function fitHistoryWithoutClipping(): void {
  const contentOverflows = () => {
    const first = captionText.firstElementChild?.getBoundingClientRect();
    const last = captionText.lastElementChild?.getBoundingClientRect();
    if (!first || !last) return false;
    const bounds = captionText.getBoundingClientRect();
    return first.top < bounds.top - 1
      || last.bottom > bounds.bottom + 1
      || captionText.scrollHeight > captionText.clientHeight + 1;
  };
  while (captionText.children.length > 1 && contentOverflows()) {
    captionText.firstElementChild?.remove();
  }
  const remaining = [...captionText.querySelectorAll<HTMLElement>(":scope > .caption-entry")];
  remaining.forEach((entry, index) => {
    const historyDepth = remaining.length - index - 1;
    entry.dataset.historyDepth = String(historyDepth);
    entry.classList.toggle("caption-history", historyDepth > 0);
  });
}

function renderCaption(): void {
  const entries = output.originalCaption === rawCaption
    ? output.entries
    : fallbackEntries(rawCaption);
  const mode = output.originalCaption === rawCaption ? output.mode : "original";
  captionText.dataset.mode = mode;
  const visibleEntries = entries.slice(-overlaySettings.maximumLines);
  const rendered = visibleEntries.map((entry, index) => {
    const group = document.createElement("span");
    group.className = "caption-entry";
    const historyDepth = visibleEntries.length - index - 1;
    group.dataset.historyDepth = String(historyDepth);
    if (historyDepth > 0) group.classList.add("caption-history");
    if (mode === "original") {
      group.append(captionLine(
        entry.original,
        "caption-line caption-original",
        entry.sourceLanguage === "auto" ? "" : entry.sourceLanguage
      ));
    } else if (mode === "english" && entry.translation) {
      group.append(captionLine(entry.translation, "caption-line caption-translation", "en"));
    } else if (mode === "english") {
      markTranslationState(group, entry);
      group.append(captionLine(
        entry.original,
        "caption-line caption-original caption-fallback",
        entry.sourceLanguage === "auto" ? "" : entry.sourceLanguage
      ));
      group.append(captionLine(
        translationStatus(entry),
        "caption-line caption-translation caption-translation-status",
        "en"
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
        markTranslationState(group, entry);
        group.append(captionLine(
          translationStatus(entry),
          "caption-line caption-translation caption-translation-status",
          "en"
        ));
      }
    }
    return group;
  });
  captionText.replaceChildren(...rendered);
  surface.hidden = !captionActive || rendered.length === 0;
  if (!surface.hidden) fitHistoryWithoutClipping();
}

function cancelCaptionClear(): void {
  if (clearTimer !== undefined) window.clearTimeout(clearTimer);
  clearTimer = undefined;
}

function clearCaption(): void {
  cancelCaptionClear();
  deferredClear = false;
  rawCaption = "";
  captionActive = false;
  renderCaption();
}

function matchingTranslationPending(): boolean {
  return output.originalCaption === rawCaption
    && output.entries.some((entry) => entry.translationPending);
}

function handleRawCaption(caption: string): void {
  if (caption.trim()) {
    cancelCaptionClear();
    deferredClear = false;
    rawCaption = caption;
    captionActive = true;
    renderCaption();
    return;
  }

  if (captionActive && rawCaption.trim() && matchingTranslationPending()) {
    deferredClear = true;
    cancelCaptionClear();
    clearTimer = window.setTimeout(clearCaption, TRANSLATION_MAX_WAIT_MS);
    return;
  }
  clearCaption();
}

function handleCaptionOutput(payload: CaptionOutputPayload): void {
  output = payload;
  renderCaption();
  if (!deferredClear) return;
  if (output.originalCaption !== rawCaption) {
    clearCaption();
    return;
  }
  if (output.entries.some((entry) => entry.translationPending)) return;
  deferredClear = false;
  cancelCaptionClear();
  clearTimer = window.setTimeout(clearCaption, TRANSLATION_RESULT_HOLD_MS);
}

function applySettings(settings: OverlaySettings) {
  overlaySettings = { ...settings };
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
  renderCaption();
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
  handleRawCaption("今日は何をする予定ですか？");
  handleCaptionOutput({
    mode: "both",
    originalCaption: rawCaption,
    entries: [{
      key: "preview",
      sourceLanguage: "ja",
      original: rawCaption,
      translation: "What are you planning to do today?",
      isFinal: true
    }]
  });
  if (import.meta.env.DEV) {
    window.__PROLLYGLOT_OVERLAY_PREVIEW__ = {
      setCaption: handleRawCaption,
      setOutput: handleCaptionOutput
    };
  }
}

if (isTauri()) {
  void listen<string>("overlay-caption", ({ payload }) => {
    handleRawCaption(payload);
  });
  void listen<CaptionOutputPayload>("caption-output", ({ payload }) => {
    handleCaptionOutput(payload);
  });
  void listen<OverlaySettings>("overlay-settings", ({ payload }) => applySettings(payload));
}
