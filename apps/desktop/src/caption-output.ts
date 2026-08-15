import {
  supportedTranslationLanguage,
  type TranslationLanguage
} from "./language-catalog";
import {
  TranslationService,
  TranslationSession,
  isExpectedTranslationCancellation,
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
const MAX_RECENT_FINAL_TRANSLATIONS = 8;
const LIVE_TRANSLATION_DELAY_MS = 420;
const LIVE_TRANSLATION_INTERVAL_MS = 900;
const LIVE_TRANSLATION_MIN_CHARACTERS = 4;

type TranslationState =
  | { phase: "ready"; text: string }
  | { phase: "failed"; message: string };

interface PendingFinal {
  sessionId: string;
  targetLanguage: TranslationLanguage;
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
  private readonly pending = new Map<string, PendingFinal>();
  private translationActive = true;
  private session?: TranslationSession;
  private liveCandidate?: TranscriptSegment;
  private liveTranslation?: LiveTranslation;
  private liveRequest?: { key: string; utteranceKey: string; sessionId: string };
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

  prepare(sourceLanguage: TranslationLanguage): Promise<void> {
    if (!this.translationEnabled()
      || sourceLanguage === this.targetLanguage
      || !this.translationModelUsable(sourceLanguage)) {
      return Promise.resolve();
    }
    return this.ensureSession().prepare(sourceLanguage, this.targetLanguage);
  }

  setTranslationActive(active: boolean): void {
    if (this.translationActive === active) return;
    this.translationActive = active;
    if (!active) this.closeSession("Audio-caption translation paused.");
    else this.scheduleTranslations();
    this.publish();
  }

  setOutputMode(mode: CaptionOutputMode): void {
    if (this.mode === mode) return;
    this.mode = mode;
    if (mode === "original") {
      this.closeSession("Translated caption output was disabled.");
    } else {
      this.scheduleTranslations();
    }
    this.publish();
  }

  setTranslationTarget(targetLanguage: TranslationLanguage): void {
    if (this.targetLanguage === targetLanguage) return;
    this.targetLanguage = targetLanguage;
    this.translations.clear();
    this.liveTranslation = undefined;
    this.closeSession("The caption translation language changed.");
    this.scheduleTranslations();
    this.publish();
  }

  updateTranscript(transcript: TranscriptSnapshot): void {
    this.transcript = transcript;
    const currentKeys = new Set(transcript.committed.map(segmentKey));
    for (const key of this.translations.keys()) {
      if (!currentKeys.has(key)) this.translations.delete(key);
    }
    for (const key of this.pending.keys()) {
      if (!currentKeys.has(key)) this.pending.delete(key);
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
        this.translationEnabled()
        && usable
        && provisional.text.trim().length >= LIVE_TRANSLATION_MIN_CHARACTERS
        && !alreadyTranslated
        && this.liveRequest?.key !== segmentKey(provisional)
      ) {
        const replacingSameUtterance = this.liveCandidate !== undefined
          && utteranceKey(this.liveCandidate) === utteranceKey(provisional);
        this.liveCandidate = provisional;
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
    if (!this.translationActive) return false;
    const key = segmentKey(segment);
    if (segment.isFinal) return this.pending.has(key);
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
    if (!this.translationEnabled()) return;
    const session = this.ensureSession();
    for (const segment of this.transcript.committed.slice(-MAX_RECENT_FINAL_TRANSLATIONS)) {
      const sourceLanguage = supportedSourceLanguage(segment.sourceLanguage);
      const key = segmentKey(segment);
      if (
        !sourceLanguage
        || sourceLanguage === this.targetLanguage
        || !this.translationModelUsable(sourceLanguage)
        || this.translations.has(key)
        || this.pending.has(key)
      ) continue;
      this.pending.set(key, { sessionId: session.id, targetLanguage: this.targetLanguage });
      void this.translateFinal(session, segment, sourceLanguage);
    }
    this.armLiveTimer();
  }

  private async translateFinal(
    session: TranslationSession,
    segment: TranscriptSegment,
    sourceLanguage: TranslationLanguage
  ): Promise<void> {
    const key = segmentKey(segment);
    const targetLanguage = this.targetLanguage;
    const startedAtMs = Date.now();
    try {
      const text = await session.translate({
        sourceRevision: this.transcript.revision,
        workloadProfile: "captionFinal",
        sourceLanguage,
        targetLanguage,
        text: segment.text,
        coalesceKey: `caption-final:${key}`
      });
      if (this.currentPending(key, session.id, targetLanguage) && this.hasCommittedSegment(key)) {
        this.translations.set(key, { phase: "ready", text });
      }
    } catch (error) {
      if (!isExpectedTranslationCancellation(error)
        && this.currentPending(key, session.id, targetLanguage)
        && this.hasCommittedSegment(key)) {
        const message = error instanceof Error ? error.message : String(error);
        this.translations.set(key, { phase: "failed", message });
        this.reportError(`Translation stopped: ${message}`);
      }
    } finally {
      if (this.currentPending(key, session.id, targetLanguage)) this.pending.delete(key);
      this.reportDiagnostic(
        `${sourceLanguage} to ${targetLanguage} final caption settled in ${Date.now() - startedAtMs} ms.`
      );
      this.publish();
    }
  }

  private armLiveTimer(): void {
    if (!this.liveCandidate || this.liveTimer !== undefined || !this.translationEnabled()) return;
    const delay = Math.max(0, this.liveReadyAtMs - Date.now());
    this.liveTimer = window.setTimeout(() => {
      this.liveTimer = undefined;
      const candidate = this.liveCandidate;
      this.liveCandidate = undefined;
      if (candidate) void this.translateLive(candidate);
    }, delay);
  }

  private async translateLive(segment: TranscriptSegment): Promise<void> {
    const sourceLanguage = supportedSourceLanguage(segment.sourceLanguage);
    if (
      !sourceLanguage
      || sourceLanguage === this.targetLanguage
      || !this.translationModelUsable(sourceLanguage)
      || !this.translationEnabled()
    ) return;
    const session = this.ensureSession();
    const requestKey = segmentKey(segment);
    const requestUtteranceKey = utteranceKey(segment);
    const targetLanguage = this.targetLanguage;
    this.liveRequest = { key: requestKey, utteranceKey: requestUtteranceKey, sessionId: session.id };
    this.lastLiveStartedAtMs = Date.now();
    this.publish();
    try {
      const text = await session.translate({
        sourceRevision: this.transcript.revision,
        workloadProfile: "captionLive",
        sourceLanguage,
        targetLanguage,
        text: segment.text,
        coalesceKey: `caption-live:${requestUtteranceKey}`
      });
      const current = this.transcript.provisional;
      if (
        this.translationEnabled()
        && this.session?.id === session.id
        && current
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
      if (!isExpectedTranslationCancellation(error)) {
        const message = error instanceof Error ? error.message : String(error);
        this.reportDiagnostic(`Live translation attempt stopped: ${message}`);
      }
    } finally {
      if (this.liveRequest?.key === requestKey && this.liveRequest.sessionId === session.id) {
        this.liveRequest = undefined;
      }
      const current = this.transcript.provisional;
      if (
        current
        && this.translationEnabled()
        && this.session?.id === session.id
        && this.targetLanguage === targetLanguage
        && utteranceKey(current) === requestUtteranceKey
        && current.text !== segment.text
      ) {
        this.liveCandidate = current;
        this.liveReadyAtMs = Math.max(
          Date.now() + 120,
          this.lastLiveStartedAtMs + LIVE_TRANSLATION_INTERVAL_MS
        );
        this.armLiveTimer();
      }
      this.publish();
    }
  }

  private currentPending(
    key: string,
    sessionId: string,
    targetLanguage: TranslationLanguage
  ): boolean {
    const pending = this.pending.get(key);
    return pending?.sessionId === sessionId
      && pending.targetLanguage === targetLanguage
      && this.session?.id === sessionId
      && this.targetLanguage === targetLanguage;
  }

  private ensureSession(): TranslationSession {
    if (this.session && !this.session.isActive()) this.session = undefined;
    this.session ??= this.service.openSession("captions");
    return this.session;
  }

  private closeSession(reason: string): void {
    this.cancelLiveTimer();
    this.liveCandidate = undefined;
    this.liveRequest = undefined;
    this.pending.clear();
    this.session?.close(reason);
    this.session = undefined;
  }

  private cancelLiveTimer(): void {
    if (this.liveTimer !== undefined) window.clearTimeout(this.liveTimer);
    this.liveTimer = undefined;
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
    return this.translationActive && this.mode !== "original";
  }

  private publish(): void {
    void this.publishOutput(this.payload());
  }

  private hasCommittedSegment(key: string): boolean {
    return this.transcript.committed.some((segment) => segmentKey(segment) === key);
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
