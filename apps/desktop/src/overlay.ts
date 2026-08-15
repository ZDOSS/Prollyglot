import "./styles.css";

import { listen } from "@tauri-apps/api/event";

import { isTauri } from "./bridge";
import { languageLabel } from "./language-catalog";
import { PresentationCursor, captionDisplayState } from "./presentation-state";
import {
  DEFAULT_OVERLAY_SETTINGS,
  RUNTIME_EVENTS,
  type CaptionPresentationEntry,
  type CaptionPresentationFrame,
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
const cursor = new PresentationCursor<CaptionPresentationFrame>();
let frame: CaptionPresentationFrame | undefined;
let overlaySettings: OverlaySettings = { ...DEFAULT_OVERLAY_SETTINGS };
let visibilityTimer: number | undefined;

declare global {
  interface Window {
    __PROLLYGLOT_OVERLAY_PREVIEW__?: {
      setPresentation: (frame: CaptionPresentationFrame) => void;
    };
  }
}

function captionLine(text: string, className: string, language: string): HTMLElement {
  const line = document.createElement("span");
  line.className = className;
  line.lang = language;
  line.textContent = text;
  return line;
}

function translationStatus(entry: CaptionPresentationEntry): string {
  const target = frame?.targetLanguage ? languageLabel(frame.targetLanguage) : "Translation";
  if (!entry.isFinal) return `${target} is catching up…`;
  if (entry.translationPending) return "Translating…";
  return `${target} unavailable`;
}

function markTranslationState(group: HTMLElement, entry: CaptionPresentationEntry): void {
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
  const output = frame;
  const mode = output?.mode ?? "original";
  captionText.dataset.mode = mode;
  const visibleEntries = (output?.entries ?? []).slice(-overlaySettings.maximumLines);
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
        output?.targetLanguage ?? ""
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
        output?.targetLanguage ?? ""
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
          output?.targetLanguage ?? ""
        ));
      } else {
        markTranslationState(group, entry);
        group.append(captionLine(
          translationStatus(entry),
          "caption-line caption-translation caption-translation-status",
          output?.targetLanguage ?? ""
        ));
      }
    }
    return group;
  });
  captionText.replaceChildren(...rendered);
  if (!surface.hidden) fitHistoryWithoutClipping();
}

function cancelVisibilityTimer(): void {
  if (visibilityTimer !== undefined) window.clearTimeout(visibilityTimer);
  visibilityTimer = undefined;
}

function applyVisibility(nowMs = Date.now()): void {
  cancelVisibilityTimer();
  const state = frame
    ? captionDisplayState(
        frame,
        overlaySettings.readingTimeSeconds,
        overlaySettings.fadeDurationMs,
        nowMs
      )
    : { phase: "hidden" as const };
  surface.classList.toggle("caption-fading", state.phase === "fading");
  surface.hidden = state.phase === "hidden" || captionText.childElementCount === 0;
  if (!surface.hidden) fitHistoryWithoutClipping();
  if ("nextAtMs" in state && state.nextAtMs !== undefined) {
    visibilityTimer = window.setTimeout(
      () => applyVisibility(),
      Math.max(0, state.nextAtMs - nowMs)
    );
  }
}

function handlePresentation(next: CaptionPresentationFrame): void {
  if (!cursor.accept(next)) return;
  frame = structuredClone(next);
  renderCaption();
  applyVisibility();
}

function applySettings(settings: OverlaySettings): void {
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
  applyVisibility();
}

function storedSettings(): OverlaySettings {
  try {
    const stored = localStorage.getItem("prollyglot.overlay");
    return stored
      ? { ...DEFAULT_OVERLAY_SETTINGS, ...(JSON.parse(stored) as Partial<OverlaySettings>) }
      : { ...DEFAULT_OVERLAY_SETTINGS };
  } catch {
    return { ...DEFAULT_OVERLAY_SETTINGS };
  }
}

applySettings(storedSettings());
if (!isTauri()) {
  handlePresentation({
    sessionId: 1,
    runtimeRevision: 1,
    presentationRevision: 1,
    phase: "holding",
    readableAtMs: Date.now(),
    mode: "both",
    targetLanguage: "en",
    entries: [{
      key: "preview",
      sourceLanguage: "ja",
      original: "今日は何をする予定ですか？",
      translation: "What are you planning to do today?",
      translationPending: false,
      isFinal: true
    }]
  });
  if (import.meta.env.DEV) {
    window.__PROLLYGLOT_OVERLAY_PREVIEW__ = { setPresentation: handlePresentation };
  }
}

if (isTauri()) {
  void listen<CaptionPresentationFrame>(
    RUNTIME_EVENTS.captionPresentation,
    ({ payload }) => handlePresentation(payload)
  );
  void listen<OverlaySettings>("overlay-settings", ({ payload }) => applySettings(payload));
}
