import { icons } from "./icons.ts";
import { languageLabel } from "./language-catalog.ts";
import type {
  CaptionOutputMode,
  TranscriptSegment,
  TranscriptSnapshot
} from "./types.ts";

const BOTTOM_THRESHOLD = 48;

type TranslationState =
  | { phase: "ready"; text: string }
  | { phase: "failed"; message: string };

export interface TranscriptPresentation {
  outputMode: () => CaptionOutputMode;
  translationTarget: () => string;
  translationFor: (segment: TranscriptSegment) => TranslationState | undefined;
  isTranslationPending: (segment: TranscriptSegment) => boolean;
}

export interface TranscriptPanelActions {
  clear: () => Promise<void>;
  reportError: (message: string) => void;
  setFollowLatest: (follow: boolean) => void;
}

export interface TranscriptPanelOptions {
  forceLatest?: boolean;
  followLatest: boolean;
}

export interface TranscriptScrollInput {
  forceLatest: boolean;
  hasPreviousList: boolean;
  followLatest: boolean;
  distanceFromBottom: number;
}

export function shouldFollowTranscriptLatest(input: TranscriptScrollInput): boolean {
  return input.forceLatest
    || !input.hasPreviousList
    || input.followLatest
    || input.distanceFromBottom <= BOTTOM_THRESHOLD;
}

export function formatTranscriptTimestamp(micros: number): string {
  const seconds = Math.max(0, Math.floor(micros / 1_000_000));
  const minutes = Math.floor(seconds / 60);
  return `${String(minutes).padStart(2, "0")}:${String(seconds % 60).padStart(2, "0")}`;
}

export class TranscriptPanel {
  private readonly previewContent: HTMLElement;
  private readonly presentation: TranscriptPresentation;
  private readonly actions: TranscriptPanelActions;

  constructor(
    previewContent: HTMLElement,
    presentation: TranscriptPresentation,
    actions: TranscriptPanelActions
  ) {
    this.previewContent = previewContent;
    this.presentation = presentation;
    this.actions = actions;
  }

  renderPreview(transcript: TranscriptSnapshot): void {
    this.previewContent.replaceChildren();
    const segments = transcript.committed.slice(-6);
    if (transcript.provisional) segments.push(transcript.provisional);
    if (segments.length === 0) {
      const empty = document.createElement("div");
      empty.className = "session-preview-empty";
      empty.innerHTML = `${icons.transcript}<strong>Waiting for captions</strong><span>The newest finalized and provisional text will stay visible here.</span>`;
      this.previewContent.append(empty);
      return;
    }

    const list = document.createElement("ol");
    list.className = "session-preview-list";
    list.setAttribute("aria-label", "Latest captions");
    for (const segment of segments) {
      const item = document.createElement("li");
      item.className = `transcript-segment${segment.isFinal ? "" : " provisional"}`;
      const timestamp = document.createElement("time");
      timestamp.textContent = segment.isFinal
        ? formatTranscriptTimestamp(segment.startMicros)
        : "Live";
      item.append(timestamp);
      this.appendCaption(item, segment);
      list.append(item);
    }
    this.previewContent.append(list);
    requestAnimationFrame(() => {
      this.previewContent.scrollTop = this.previewContent.scrollHeight;
    });
  }

  render(
    content: HTMLElement,
    transcript: TranscriptSnapshot,
    options: TranscriptPanelOptions
  ): void {
    content.className = "transcript-content";
    const previousList = content.querySelector<HTMLOListElement>(".transcript-list");
    const previousScrollTop = previousList?.scrollTop ?? 0;
    const previousDistanceFromBottom = previousList
      ? previousList.scrollHeight - previousList.clientHeight - previousList.scrollTop
      : 0;
    const shouldFollowLatest = shouldFollowTranscriptLatest({
      forceLatest: options.forceLatest ?? false,
      hasPreviousList: Boolean(previousList),
      followLatest: options.followLatest,
      distanceFromBottom: previousDistanceFromBottom
    });
    content.replaceChildren();

    const toolbar = document.createElement("div");
    toolbar.className = "dialog-toolbar";
    const summary = document.createElement("span");
    summary.className = "dialog-summary";
    summary.textContent = `${transcript.committed.length} finalized ${transcript.committed.length === 1 ? "caption" : "captions"}`;
    const actions = document.createElement("div");
    actions.className = "dialog-toolbar-actions";
    const latest = document.createElement("button");
    latest.type = "button";
    latest.className = "text-button";
    latest.textContent = "Latest";
    latest.hidden = shouldFollowLatest;
    const clear = document.createElement("button");
    clear.type = "button";
    clear.className = "text-button";
    clear.textContent = "Clear";
    clear.disabled = transcript.committed.length === 0 && !transcript.provisional;
    clear.addEventListener("click", () => {
      void this.actions.clear().catch((error: unknown) => {
        this.actions.reportError(error instanceof Error ? error.message : String(error));
      });
    });
    actions.append(latest, clear);
    toolbar.append(summary, actions);
    content.append(toolbar);

    if (transcript.committed.length === 0 && !transcript.provisional) {
      this.actions.setFollowLatest(true);
      const empty = document.createElement("p");
      empty.className = "empty-copy";
      empty.textContent = "Finalized captions from this session will appear here.";
      content.append(empty);
      return;
    }

    const list = document.createElement("ol");
    list.className = "transcript-list";
    list.setAttribute("aria-label", "Session transcript");
    for (const segment of transcript.committed) {
      const item = document.createElement("li");
      item.className = "transcript-segment";
      const timestamp = document.createElement("time");
      timestamp.textContent = formatTranscriptTimestamp(segment.startMicros);
      item.append(timestamp);
      this.appendCaption(item, segment);
      list.append(item);
    }
    if (transcript.provisional) {
      const item = document.createElement("li");
      item.className = "transcript-segment provisional";
      const timestamp = document.createElement("time");
      timestamp.textContent = "Live";
      item.append(timestamp);
      this.appendCaption(item, transcript.provisional);
      list.append(item);
    }
    content.append(list);

    const updateFollowState = () => {
      const distanceFromBottom = list.scrollHeight - list.clientHeight - list.scrollTop;
      const follow = distanceFromBottom <= BOTTOM_THRESHOLD;
      this.actions.setFollowLatest(follow);
      latest.hidden = follow;
    };
    list.addEventListener("scroll", updateFollowState, { passive: true });
    latest.addEventListener("click", () => {
      this.actions.setFollowLatest(true);
      list.scrollTop = list.scrollHeight;
      latest.hidden = true;
    });

    requestAnimationFrame(() => {
      if (shouldFollowLatest) {
        this.actions.setFollowLatest(true);
        list.scrollTop = list.scrollHeight;
        latest.hidden = true;
      } else {
        this.actions.setFollowLatest(false);
        list.scrollTop = Math.min(previousScrollTop, list.scrollHeight - list.clientHeight);
        latest.hidden = false;
      }
    });
  }

  private appendCaption(item: HTMLElement, segment: TranscriptSegment): void {
    const copy = document.createElement("span");
    copy.className = "transcript-copy";
    const original = document.createElement("span");
    original.className = "transcript-text transcript-original";
    original.lang = segment.sourceLanguage === "auto" ? "" : segment.sourceLanguage;
    original.textContent = segment.text;
    const mode = this.presentation.outputMode();
    const translated = this.presentation.translationFor(segment);
    const targetLanguage = this.presentation.translationTarget();
    const targetLabel = languageLabel(targetLanguage);

    if (mode === "original") {
      copy.append(original);
      item.append(copy);
      return;
    }

    const translation = document.createElement("span");
    translation.className = "transcript-text transcript-translation";
    translation.lang = targetLanguage;
    if (translated?.phase === "ready") translation.textContent = translated.text;

    if (mode === "both") copy.append(original);
    if (translated?.phase === "ready") {
      copy.append(translation);
    } else {
      if (mode === "translated") {
        original.classList.add("translation-fallback");
        copy.append(original);
      }
      const note = document.createElement("span");
      note.className = "transcript-translation-state";
      note.textContent = translated?.phase === "failed"
        ? `${targetLabel} unavailable · showing original`
        : this.presentation.isTranslationPending(segment)
          ? `Translating to ${targetLabel}…`
          : `${targetLabel} translator is not ready`;
      copy.append(note);
    }
    item.append(copy);
  }
}
