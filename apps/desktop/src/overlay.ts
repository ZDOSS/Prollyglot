import "./styles.css";

import { listen } from "@tauri-apps/api/event";

import { isTauri } from "./bridge";
import { languageLabel } from "./language-catalog";
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
let rawCaption = "";
let captionActive = false;
let output: CaptionOutputPayload = { mode: "original", originalCaption: "", entries: [] };
let overlaySettings: OverlaySettings = { ...DEFAULT_OVERLAY_SETTINGS };
let clearRequestedAtMs: number | undefined;
let lastReadableUpdateAtMs = 0;
let readableSignature = "";
let clearTimer: number | undefined;
let fadeTimer: number | undefined;

declare global {
  interface Window {
    __PROLLYGLOT_OVERLAY_PREVIEW__?: {
      setCaption: (caption: string) => void;
      setOutput: (payload: CaptionOutputPayload) => void;
    };
  }
}

function fallbackEntries(caption: string): CaptionOutputEntry[] {
  const sourceLanguage = output.entries.at(-1)?.sourceLanguage ?? "auto";
  return caption
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean)
    .map((original, index) => ({
      key: `fallback:${index}`,
      sourceLanguage,
      original,
      translationPending: output.mode !== "original",
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
  const target = output.targetLanguage ? languageLabel(output.targetLanguage) : "Translation";
  if (!entry.isFinal) return `${target} is catching up…`;
  if (entry.translationPending) return "Translating…";
  return `${target} unavailable`;
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
  // Raw overlay events arrive directly from the transcription worker while
  // structured bilingual output takes a short trip through the control
  // window. Keep the last complete structured frame during that gap instead
  // of collapsing to a full-size original-only layout for one paint.
  const entries = !rawCaption.trim() || output.originalCaption === rawCaption
    ? output.entries
    : output.entries.length > 0 ? output.entries : fallbackEntries(rawCaption);
  const mode = output.mode;
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
    } else if (mode === "translated" && entry.translation) {
      group.append(captionLine(
        entry.translation,
        "caption-line caption-translation",
        output.targetLanguage ?? ""
      ));
    } else if (mode === "translated") {
      markTranslationState(group, entry);
      group.append(captionLine(
        entry.original,
        "caption-line caption-original caption-fallback",
        entry.sourceLanguage === "auto" ? "" : entry.sourceLanguage
      ));
      group.append(captionLine(
        translationStatus(entry),
        "caption-line caption-translation caption-translation-status",
        output.targetLanguage ?? ""
      ));
    } else {
      group.append(captionLine(
        entry.original,
        "caption-line caption-original",
        entry.sourceLanguage === "auto" ? "" : entry.sourceLanguage
      ));
      if (entry.translation) {
        group.append(captionLine(
          entry.translation,
          "caption-line caption-translation",
          output.targetLanguage ?? ""
        ));
      } else {
        markTranslationState(group, entry);
        group.append(captionLine(
          translationStatus(entry),
          "caption-line caption-translation caption-translation-status",
          output.targetLanguage ?? ""
        ));
      }
    }
    return group;
  });
  captionText.replaceChildren(...rendered);
  surface.hidden = !captionActive || rendered.length === 0;
  if (!surface.hidden) fitHistoryWithoutClipping();
}

function cancelScheduledClear(): void {
  if (clearTimer !== undefined) window.clearTimeout(clearTimer);
  if (fadeTimer !== undefined) window.clearTimeout(fadeTimer);
  clearTimer = undefined;
  fadeTimer = undefined;
  surface.classList.remove("caption-fading");
}

function clearCaption(): void {
  cancelScheduledClear();
  clearRequestedAtMs = undefined;
  rawCaption = "";
  captionActive = false;
  renderCaption();
}

function matchingTranslationPending(): boolean {
  return output.mode !== "original"
    && output.entries.some((entry) => entry.translationPending);
}

function beginCaptionFade(): void {
  if (clearRequestedAtMs === undefined) return;
  if (overlaySettings.fadeDurationMs <= 0) {
    clearCaption();
    return;
  }
  surface.classList.add("caption-fading");
  fadeTimer = window.setTimeout(clearCaption, overlaySettings.fadeDurationMs);
}

function scheduleCaptionClear(): void {
  if (clearRequestedAtMs === undefined) return;
  cancelScheduledClear();
  const now = Date.now();
  const translationDeadline = clearRequestedAtMs + TRANSLATION_MAX_WAIT_MS;
  if (matchingTranslationPending() && now < translationDeadline) {
    clearTimer = window.setTimeout(scheduleCaptionClear, translationDeadline - now);
    return;
  }

  const readableAt = lastReadableUpdateAtMs || clearRequestedAtMs;
  const readingDeadline = Math.max(
    clearRequestedAtMs,
    readableAt + overlaySettings.readingTimeSeconds * 1_000
  );
  const remaining = readingDeadline - now;
  if (remaining > 0) {
    clearTimer = window.setTimeout(beginCaptionFade, remaining);
  } else {
    beginCaptionFade();
  }
}

function handleRawCaption(caption: string): void {
  if (caption.trim()) {
    cancelScheduledClear();
    clearRequestedAtMs = undefined;
    rawCaption = caption;
    captionActive = true;
    renderCaption();
    return;
  }

  rawCaption = "";
  if (!captionActive || output.entries.length === 0) {
    clearCaption();
    return;
  }
  clearRequestedAtMs ??= Date.now();
  renderCaption();
  scheduleCaptionClear();
}

function handleCaptionOutput(payload: CaptionOutputPayload): void {
  const nextSignature = JSON.stringify([
    payload.mode,
    payload.targetLanguage,
    payload.entries.map((entry) => [
      entry.key,
      entry.original,
      entry.translation,
      entry.translationPending,
      entry.isFinal
    ])
  ]);
  if (nextSignature !== readableSignature && payload.entries.length > 0) {
    readableSignature = nextSignature;
    lastReadableUpdateAtMs = Date.now();
  }
  output = payload;
  renderCaption();
  if (clearRequestedAtMs === undefined) return;
  if (!output.originalCaption.trim() || output.entries.length === 0) {
    clearCaption();
    return;
  }
  scheduleCaptionClear();
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
  surface.style.setProperty("--caption-fade-duration", `${settings.fadeDurationMs}ms`);
  surface.dataset.clickThrough = String(settings.clickThrough);
  surface.dataset.bilingualLayout = settings.bilingualLayout;
  requireElement<HTMLElement>("#overlay-app").dataset.position = settings.position;
  renderCaption();
  if (clearRequestedAtMs !== undefined) scheduleCaptionClear();
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
