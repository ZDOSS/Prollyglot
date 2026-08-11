import {
  supportedTranslationLanguage,
  type TranslationLanguage
} from "./language-catalog";
import {
  TranslationService,
  translationStatusForRoute
} from "./translation";
import type {
  CaptionOutputMode,
  CaptionOutputPayload,
  TranscriptSegment,
  TranscriptSnapshot,
  TranslationCatalogStatus
} from "./types";

const MAX_OVERLAY_SEGMENTS = 4;
const OVERLAY_CONTEXT_GAP_MICROS = 5_000_000;
const MAX_TRANSLATION_QUEUE = 8;
const SLOW_TRANSLATION_MS = 2_000;
const TRANSLATION_DIAGNOSTIC_INTERVAL_MS = 15_000;
const LIVE_TRANSLATION_DELAY_MS = 420;
const LIVE_TRANSLATION_INTERVAL_MS = 900;
const LIVE_TRANSLATION_MIN_CHARACTERS = 4;

type TranslationState =
  | { phase: "ready"; text: string }
  | { phase: "failed"; message: string };

interface QueuedSegment {
  key: string;
  segment: TranscriptSegment;
  queuedAtMs: number;
}

interface LiveTranslation {
  utteranceKey: string;
  sourceText: string;
  targetLanguage: TranslationLanguage;
  text: string;
}

export class CaptionOutputController {
  private transcript: TranscriptSnapshot = { revision: 0, committed: [] };
  private mode: CaptionOutputMode = "original";
  private targetLanguage: TranslationLanguage = "en";
  private catalog: TranslationCatalogStatus;
  private readonly translations = new Map<string, TranslationState>();
  private readonly queued = new Set<string>();
  private readonly pending = new Set<string>();
  private queue: QueuedSegment[] = [];
  private pumping = false;
  private skippedStaleSegments = 0;
  private lastDiagnosticAtMs = 0;
  private reportedFirstTranslation = false;
  private liveCandidate?: TranscriptSegment;
  private liveTranslation?: LiveTranslation;
  private liveRequest?: { key: string; utteranceKey: string };
  private liveReadyAtMs = 0;
  private lastLiveStartedAtMs = 0;
  private liveTimer?: number;

  constructor(
    private readonly service: TranslationService,
    private readonly publishOutput: (payload: CaptionOutputPayload) => void | Promise<void>,
    private readonly reportError: (message: string) => void,
    private readonly reportDiagnostic: (message: string) => void = () => undefined
  ) {
    this.catalog = service.snapshot();
    service.subscribe((catalog) => {
      const recovered = catalog.models.some((model) => {
        const previousPhase = this.catalog.models.find(
          ({ modelId }) => modelId === model.modelId
        )?.phase;
        return model.phase === "ready"
          && (previousPhase === "failed" || previousPhase === "corrupt");
      });
      if (recovered) {
        for (const [key, translation] of this.translations) {
          if (translation.phase === "failed") this.translations.delete(key);
        }
      }
      this.catalog = catalog;
      this.scheduleTranslations();
      this.publish();
    });
  }

  outputMode(): CaptionOutputMode {
    return this.mode;
  }

  translationTarget(): TranslationLanguage {
    return this.targetLanguage;
  }

  setOutputMode(mode: CaptionOutputMode): void {
    this.mode = mode;
    if (mode === "original") this.clearLiveScheduling();
    this.scheduleTranslations();
    this.publish();
  }

  setTranslationTarget(targetLanguage: TranslationLanguage): void {
    if (this.targetLanguage === targetLanguage) return;
    this.targetLanguage = targetLanguage;
    this.translations.clear();
    this.queue = [];
    this.queued.clear();
    this.clearLiveScheduling();
    this.liveTranslation = undefined;
    this.scheduleTranslations();
    this.publish();
  }

  updateTranscript(transcript: TranscriptSnapshot): void {
    this.transcript = transcript;
    const currentKeys = new Set(transcript.committed.map(segmentKey));
    for (const key of this.translations.keys()) {
      if (!currentKeys.has(key)) this.translations.delete(key);
    }
    this.queue = this.queue.filter(({ key }) => currentKeys.has(key));
    for (const key of this.queued) {
      if (!currentKeys.has(key)) this.queued.delete(key);
    }

    const provisional = transcript.provisional;
    if (!provisional) {
      this.liveCandidate = undefined;
      this.cancelLiveTimer();
    } else {
      const sourceLanguage = supportedSourceLanguage(provisional.sourceLanguage);
      const usable = sourceLanguage
        && sourceLanguage !== this.targetLanguage
        && this.translationModelUsable(sourceLanguage);
      const alreadyTranslated = this.liveTranslation?.utteranceKey === utteranceKey(provisional)
        && this.liveTranslation.targetLanguage === this.targetLanguage
        && this.liveTranslation.sourceText === provisional.text;
      if (
        this.mode !== "original"
        && usable
        && provisional.text.trim().length >= LIVE_TRANSLATION_MIN_CHARACTERS
        && !alreadyTranslated
        && this.liveRequest?.key !== segmentKey(provisional)
      ) {
        const replacingSameUtterance = this.liveCandidate !== undefined
          && utteranceKey(this.liveCandidate) === utteranceKey(provisional);
        this.liveCandidate = provisional;
        // Keep the first launch time while partial text is changing. Replacing
        // the candidate should coalesce the newest words, not debounce until a
        // speaker finally pauses.
        if (!replacingSameUtterance) {
          this.liveReadyAtMs = Math.max(
            Date.now() + LIVE_TRANSLATION_DELAY_MS,
            this.lastLiveStartedAtMs + LIVE_TRANSLATION_INTERVAL_MS
          );
        }
        this.armLiveTimer();
      }
    }

    for (const segment of transcript.committed) {
      if (
        this.liveTranslation?.utteranceKey === utteranceKey(segment)
        && this.liveTranslation.targetLanguage === this.targetLanguage
        && this.liveTranslation.sourceText === segment.text
      ) {
        this.translations.set(segmentKey(segment), {
          phase: "ready",
          text: this.liveTranslation.text
        });
      }
    }
    this.scheduleTranslations();
    this.publish();
  }

  translationFor(segment: TranscriptSegment): TranslationState | undefined {
    if (segment.isFinal) return this.translations.get(segmentKey(segment));
    if (
      this.liveTranslation?.utteranceKey === utteranceKey(segment)
      && this.liveTranslation.targetLanguage === this.targetLanguage
    ) {
      return { phase: "ready", text: this.liveTranslation.text };
    }
    return undefined;
  }

  isTranslationPending(segment: TranscriptSegment): boolean {
    const key = segmentKey(segment);
    if (segment.isFinal) return this.pending.has(key) || this.queued.has(key);
    return this.liveRequest?.utteranceKey === utteranceKey(segment)
      || (this.liveCandidate !== undefined
        && utteranceKey(this.liveCandidate) === utteranceKey(segment));
  }

  payload(): CaptionOutputPayload {
    const segments = recentCaptionSegments(this.transcript);
    return {
      mode: this.mode,
      targetLanguage: this.mode === "original" ? undefined : this.targetLanguage,
      originalCaption: segments.map(({ text }) => text).join("\n"),
      entries: segments.map((segment) => {
        const translation = this.translationFor(segment);
        return {
          key: segmentKey(segment),
          sourceLanguage: segment.sourceLanguage,
          original: segment.text,
          translation: translation?.phase === "ready" ? translation.text : undefined,
          translationPending: this.isTranslationPending(segment),
          isFinal: segment.isFinal
        };
      })
    };
  }

  private scheduleTranslations(): void {
    if (this.mode === "original") return;
    const candidates = this.transcript.committed.slice(-MAX_TRANSLATION_QUEUE);
    for (const segment of candidates) {
      const sourceLanguage = supportedSourceLanguage(segment.sourceLanguage);
      if (
        !sourceLanguage
        || sourceLanguage === this.targetLanguage
        || !this.translationModelUsable(sourceLanguage)
      ) continue;
      const key = segmentKey(segment);
      if (this.translations.has(key) || this.queued.has(key) || this.pending.has(key)) continue;
      this.queue.push({ key, segment, queuedAtMs: Date.now() });
      this.queued.add(key);
      if (this.queue.length > MAX_TRANSLATION_QUEUE) {
        const dropped = this.queue.shift();
        if (dropped) {
          this.queued.delete(dropped.key);
          this.skippedStaleSegments += 1;
        }
      }
    }
    void this.pump();
  }

  private translationModelUsable(sourceLanguage: TranslationLanguage): boolean {
    const phase = translationStatusForRoute(
      this.catalog,
      sourceLanguage,
      this.targetLanguage
    )?.phase;
    return phase === "ready" || phase === "loading";
  }

  private translationEnabled(): boolean {
    return this.mode !== "original";
  }

  private async pump(): Promise<void> {
    if (this.pumping || !this.translationEnabled()) return;
    this.pumping = true;
    try {
      while (true) {
        if (!this.translationEnabled()) break;
        const next = this.queue.pop();
        if (next) {
          await this.translateCommitted(next);
          continue;
        }

        const live = this.takeReadyLiveCandidate();
        if (live) {
          await this.translateLive(live);
          continue;
        }
        this.armLiveTimer();
        break;
      }
    } finally {
      this.pumping = false;
    }
  }

  private async translateCommitted(next: QueuedSegment): Promise<void> {
    this.queued.delete(next.key);
    const sourceLanguage = supportedSourceLanguage(next.segment.sourceLanguage);
    if (
      !sourceLanguage
      || sourceLanguage === this.targetLanguage
      || !this.translationModelUsable(sourceLanguage)
    ) return;
    this.pending.add(next.key);
    this.publish();
    const startedAtMs = Date.now();
    const targetLanguage = this.targetLanguage;
    try {
      const text = await this.service.translate(
        sourceLanguage,
        targetLanguage,
        next.segment.text
      );
      if (this.targetLanguage === targetLanguage && this.hasCommittedSegment(next.key)) {
        this.translations.set(next.key, { phase: "ready", text });
      }
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      if (this.targetLanguage === targetLanguage && this.hasCommittedSegment(next.key)) {
        this.translations.set(next.key, { phase: "failed", message });
        this.reportError(`Translation stopped: ${message}`);
      }
    } finally {
      this.pending.delete(next.key);
      this.reportTranslationTiming(
        sourceLanguage,
        targetLanguage,
        startedAtMs - next.queuedAtMs,
        Date.now() - startedAtMs
      );
      this.scheduleTranslations();
      this.publish();
    }
  }

  private takeReadyLiveCandidate(): TranscriptSegment | undefined {
    if (!this.liveCandidate || Date.now() < this.liveReadyAtMs) return undefined;
    const candidate = this.liveCandidate;
    this.liveCandidate = undefined;
    return candidate;
  }

  private async translateLive(segment: TranscriptSegment): Promise<void> {
    const sourceLanguage = supportedSourceLanguage(segment.sourceLanguage);
    if (
      !sourceLanguage
      || sourceLanguage === this.targetLanguage
      || !this.translationModelUsable(sourceLanguage)
    ) return;
    const requestKey = segmentKey(segment);
    const requestUtteranceKey = utteranceKey(segment);
    const targetLanguage = this.targetLanguage;
    this.liveRequest = { key: requestKey, utteranceKey: requestUtteranceKey };
    this.lastLiveStartedAtMs = Date.now();
    this.publish();
    try {
      const text = await this.service.translate(
        sourceLanguage,
        targetLanguage,
        segment.text
      );
      const current = this.transcript.provisional;
      if (
        current
        && this.targetLanguage === targetLanguage
        && utteranceKey(current) === requestUtteranceKey
      ) {
        this.liveTranslation = {
          utteranceKey: requestUtteranceKey,
          sourceText: segment.text,
          targetLanguage,
          text
        };
      }
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      this.reportDiagnostic(`Live translation attempt stopped: ${message}`);
    } finally {
      if (this.liveRequest?.key === requestKey) this.liveRequest = undefined;
      const current = this.transcript.provisional;
      if (
        current
        && this.mode !== "original"
        && this.targetLanguage === targetLanguage
        && utteranceKey(current) === requestUtteranceKey
        && current.text !== segment.text
      ) {
        this.liveCandidate = current;
        this.liveReadyAtMs = Math.max(
          Date.now() + 120,
          this.lastLiveStartedAtMs + LIVE_TRANSLATION_INTERVAL_MS
        );
      }
      this.publish();
    }
  }

  private armLiveTimer(): void {
    if (!this.liveCandidate || this.liveTimer !== undefined || this.mode === "original") return;
    const delay = Math.max(0, this.liveReadyAtMs - Date.now());
    this.liveTimer = window.setTimeout(() => {
      this.liveTimer = undefined;
      void this.pump();
    }, delay);
  }

  private cancelLiveTimer(): void {
    if (this.liveTimer !== undefined) window.clearTimeout(this.liveTimer);
    this.liveTimer = undefined;
  }

  private clearLiveScheduling(): void {
    this.cancelLiveTimer();
    this.liveCandidate = undefined;
  }

  private publish(): void {
    void this.publishOutput(this.payload());
  }

  private hasCommittedSegment(key: string): boolean {
    return this.transcript.committed.some((segment) => segmentKey(segment) === key);
  }

  private reportTranslationTiming(
    sourceLanguage: TranslationLanguage,
    targetLanguage: TranslationLanguage,
    queueWaitMs: number,
    inferenceMs: number
  ): void {
    const now = Date.now();
    const slow = queueWaitMs >= SLOW_TRANSLATION_MS
      || inferenceMs >= SLOW_TRANSLATION_MS
      || this.skippedStaleSegments > 0;
    if (
      this.reportedFirstTranslation
      && (!slow || now - this.lastDiagnosticAtMs < TRANSLATION_DIAGNOSTIC_INTERVAL_MS)
    ) return;

    const skipped = this.skippedStaleSegments;
    this.skippedStaleSegments = 0;
    this.reportedFirstTranslation = true;
    this.lastDiagnosticAtMs = now;
    this.reportDiagnostic(
      `${sourceLanguage} to ${targetLanguage} completed in ${inferenceMs} ms after `
      + `${queueWaitMs} ms queued; ${this.queue.length} caption(s) queued; `
      + `${skipped} stale caption(s) skipped.`
    );
  }
}

export function segmentKey(segment: TranscriptSegment): string {
  return [
    segment.sourceLanguage,
    segment.utteranceId,
    segment.startMicros,
    segment.endMicros,
    segment.text
  ].join(":");
}

function utteranceKey(segment: TranscriptSegment): string {
  return [segment.sourceLanguage, segment.utteranceId, segment.startMicros].join(":");
}

export function supportedSourceLanguage(language: string): TranslationLanguage | undefined {
  return supportedTranslationLanguage(language);
}

export function recentCaptionSegments(snapshot: TranscriptSnapshot): TranscriptSegment[] {
  const recent: TranscriptSegment[] = [];
  let nextStart: number | undefined;
  if (snapshot.provisional?.text.trim()) {
    recent.push(snapshot.provisional);
    nextStart = snapshot.provisional.startMicros;
  }
  for (const committed of [...snapshot.committed].reverse()) {
    if (recent.length >= MAX_OVERLAY_SEGMENTS) break;
    if (nextStart !== undefined && nextStart - committed.endMicros > OVERLAY_CONTEXT_GAP_MICROS) break;
    recent.push(committed);
    nextStart = committed.startMicros;
  }
  return recent.reverse();
}
