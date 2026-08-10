import { TranslationService } from "./translation";
import type { TranslationSourceLanguage } from "./translation-protocol";
import type {
  CaptionOutputMode,
  CaptionOutputPayload,
  TranscriptSegment,
  TranscriptSnapshot,
  TranslationCatalogStatus
} from "./types";

const MAX_OVERLAY_SEGMENTS = 4;
const OVERLAY_CONTEXT_GAP_MICROS = 2_000_000;
const MAX_TRANSLATION_QUEUE = 32;
const MAX_TRANSCRIPT_BACKFILL = 100;

type TranslationState =
  | { phase: "ready"; text: string }
  | { phase: "failed"; message: string };

interface QueuedSegment {
  key: string;
  segment: TranscriptSegment;
}

export class CaptionOutputController {
  private transcript: TranscriptSnapshot = { revision: 0, committed: [] };
  private mode: CaptionOutputMode = "original";
  private catalog: TranslationCatalogStatus;
  private readonly translations = new Map<string, TranslationState>();
  private readonly queued = new Set<string>();
  private readonly pending = new Set<string>();
  private queue: QueuedSegment[] = [];
  private pumping = false;

  constructor(
    private readonly service: TranslationService,
    private readonly publishOutput: (payload: CaptionOutputPayload) => void | Promise<void>,
    private readonly reportError: (message: string) => void
  ) {
    this.catalog = service.snapshot();
    service.subscribe((catalog) => {
      for (const model of catalog.models) {
        const previousPhase = this.catalog.models.find(
          ({ modelId }) => modelId === model.modelId
        )?.phase;
        if (
          model.phase === "ready"
          && (previousPhase === "failed" || previousPhase === "corrupt")
        ) {
          for (const [key, translation] of this.translations) {
            if (key.startsWith(`${model.sourceLanguage}:`) && translation.phase === "failed") {
              this.translations.delete(key);
            }
          }
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

  setOutputMode(mode: CaptionOutputMode): void {
    this.mode = mode;
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
    this.scheduleTranslations();
    this.publish();
  }

  translationFor(segment: TranscriptSegment): TranslationState | undefined {
    return this.translations.get(segmentKey(segment));
  }

  isTranslationPending(segment: TranscriptSegment): boolean {
    const key = segmentKey(segment);
    return this.pending.has(key) || this.queued.has(key);
  }

  payload(): CaptionOutputPayload {
    const segments = recentCaptionSegments(this.transcript);
    return {
      mode: this.mode,
      originalCaption: segments.map(({ text }) => text).join("\n"),
      entries: segments.map((segment) => {
        const translation = this.translationFor(segment);
        return {
          key: segmentKey(segment),
          sourceLanguage: segment.sourceLanguage,
          original: segment.text,
          translation: translation?.phase === "ready" ? translation.text : undefined,
          isFinal: segment.isFinal
        };
      })
    };
  }

  private scheduleTranslations(): void {
    if (this.mode === "original") return;
    const candidates = this.transcript.committed
      .slice(-MAX_TRANSCRIPT_BACKFILL)
      .reverse();
    for (const segment of candidates) {
      const sourceLanguage = supportedSourceLanguage(segment.sourceLanguage);
      if (!sourceLanguage || !this.translationModelUsable(sourceLanguage)) continue;
      const key = segmentKey(segment);
      if (this.translations.has(key) || this.queued.has(key) || this.pending.has(key)) continue;
      this.queue.unshift({ key, segment });
      this.queued.add(key);
      if (this.queue.length > MAX_TRANSLATION_QUEUE) {
        const dropped = this.queue.shift();
        if (dropped) this.queued.delete(dropped.key);
      }
    }
    void this.pump();
  }

  private translationModelUsable(sourceLanguage: TranslationSourceLanguage): boolean {
    const phase = this.catalog.models.find((model) => model.sourceLanguage === sourceLanguage)?.phase;
    return phase === "ready" || phase === "loading";
  }

  private async pump(): Promise<void> {
    if (this.pumping) return;
    this.pumping = true;
    try {
      while (this.queue.length > 0 && this.mode !== "original") {
        const next = this.queue.pop();
        if (!next) break;
        this.queued.delete(next.key);
        const sourceLanguage = supportedSourceLanguage(next.segment.sourceLanguage);
        if (!sourceLanguage || !this.translationModelUsable(sourceLanguage)) continue;
        this.pending.add(next.key);
        this.publish();
        try {
          const text = await this.service.translate(sourceLanguage, next.segment.text);
          if (this.hasCommittedSegment(next.key)) {
            this.translations.set(next.key, { phase: "ready", text });
          }
        } catch (error) {
          const message = error instanceof Error ? error.message : String(error);
          if (this.hasCommittedSegment(next.key)) {
            this.translations.set(next.key, { phase: "failed", message });
            this.reportError(`English translation stopped: ${message}`);
          }
        } finally {
          this.pending.delete(next.key);
          this.publish();
        }
      }
    } finally {
      this.pumping = false;
    }
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

export function supportedSourceLanguage(language: string): TranslationSourceLanguage | undefined {
  return language === "es" || language === "ja" ? language : undefined;
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
