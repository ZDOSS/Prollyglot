import { supportedTranslationLanguage } from "./language-catalog";
import { TranslationService } from "./translation";
import type {
  StableVisualTextRegion,
  VisualOutputPayload,
  VisualOutputRegion,
  VisualTextUpdate
} from "./types";

interface TrackedVisualRegion extends VisualOutputRegion {
  sourceLanguage: string;
}

interface QueuedTranslation {
  generation: number;
  trackId: number;
  textRevision: number;
  text: string;
}

export class VisualTranslationController {
  private readonly regions = new Map<number, TrackedVisualRegion>();
  private readonly queue: QueuedTranslation[] = [];
  private generation = 0;
  private translating = false;
  private sourceLanguage = "ja";
  private targetLanguage = "en";
  private sourceWidth = 1;
  private sourceHeight = 1;

  constructor(
    private readonly translation: TranslationService,
    private readonly publish: (output: VisualOutputPayload) => Promise<void>,
    private readonly reportError: (message: string) => void
  ) {}

  begin(sourceLanguage: string, targetLanguage: string): void {
    if (!supportedTranslationLanguage(sourceLanguage) || !supportedTranslationLanguage(targetLanguage)) {
      throw new Error("Visual translation requires a supported source and target language.");
    }
    this.generation += 1;
    this.sourceLanguage = sourceLanguage;
    this.targetLanguage = targetLanguage;
    this.regions.clear();
    this.queue.length = 0;
    this.sourceWidth = 1;
    this.sourceHeight = 1;
    void this.render();
  }

  update(update: VisualTextUpdate): void {
    this.sourceWidth = Math.max(1, update.source.width);
    this.sourceHeight = Math.max(1, update.source.height);
    const visibleTrackIds = new Set(update.visible.map(({ trackId }) => trackId));
    for (const trackId of this.regions.keys()) {
      if (!visibleTrackIds.has(trackId)) this.regions.delete(trackId);
    }
    for (const region of update.visible) this.mergeVisible(region);
    for (const region of update.translationRequests) this.enqueue(region);
    void this.render();
    void this.pump();
  }

  clear(): void {
    this.generation += 1;
    this.regions.clear();
    this.queue.length = 0;
    void this.render();
  }

  private mergeVisible(region: StableVisualTextRegion): void {
    const previous = this.regions.get(region.trackId);
    const sameRevision = previous?.textRevision === region.textRevision
      && previous.original === region.text;
    this.regions.set(region.trackId, {
      trackId: region.trackId,
      textRevision: region.textRevision,
      original: region.text,
      translation: sameRevision ? previous.translation : undefined,
      translationPending: sameRevision ? previous.translationPending : true,
      bounds: region.bounds,
      sourceLanguage: region.language ?? this.sourceLanguage
    });
  }

  private enqueue(region: StableVisualTextRegion): void {
    const current = this.regions.get(region.trackId);
    if (!current || current.textRevision !== region.textRevision) return;
    for (let index = this.queue.length - 1; index >= 0; index -= 1) {
      if (this.queue[index]?.trackId === region.trackId) this.queue.splice(index, 1);
    }
    this.queue.push({
      generation: this.generation,
      trackId: region.trackId,
      textRevision: region.textRevision,
      text: region.text
    });
    if (this.queue.length > 24) this.queue.splice(0, this.queue.length - 24);
  }

  private async pump(): Promise<void> {
    if (this.translating) return;
    this.translating = true;
    try {
      while (this.queue.length > 0) {
        const request = this.queue.pop();
        if (!request || request.generation !== this.generation) continue;
        const current = this.regions.get(request.trackId);
        if (!current || current.textRevision !== request.textRevision) continue;
        const sourceLanguage = supportedTranslationLanguage(current.sourceLanguage)
          ?? supportedTranslationLanguage(this.sourceLanguage);
        const targetLanguage = supportedTranslationLanguage(this.targetLanguage);
        if (!sourceLanguage || !targetLanguage) continue;
        try {
          await this.translation.prepare(sourceLanguage, targetLanguage);
          const translated = await this.translation.translate(
            sourceLanguage,
            targetLanguage,
            request.text
          );
          if (request.generation !== this.generation) continue;
          const latest = this.regions.get(request.trackId);
          if (!latest || latest.textRevision !== request.textRevision) continue;
          latest.translation = translated;
          latest.translationPending = false;
          await this.render();
        } catch (error) {
          if (request.generation !== this.generation) continue;
          const latest = this.regions.get(request.trackId);
          if (latest?.textRevision === request.textRevision) {
            latest.translationPending = false;
            await this.render();
          }
          this.reportError(error instanceof Error ? error.message : String(error));
        }
      }
    } finally {
      this.translating = false;
      if (this.queue.length > 0) void this.pump();
    }
  }

  private render(): Promise<void> {
    const regions = [...this.regions.values()]
      .sort((left, right) => left.bounds.y - right.bounds.y || left.bounds.x - right.bounds.x)
      .map(({ sourceLanguage: _sourceLanguage, ...region }) => region);
    return this.publish({
      sourceWidth: this.sourceWidth,
      sourceHeight: this.sourceHeight,
      sourceLanguage: this.sourceLanguage,
      targetLanguage: this.targetLanguage,
      regions
    });
  }
}
