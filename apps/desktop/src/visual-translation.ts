import { supportedTranslationLanguage } from "./language-catalog";
import { TranslationService } from "./translation";
import type {
  StableVisualTextRegion,
  VisualDetectionMode,
  VisualOutputPayload,
  VisualOutputRegion,
  VisualTextUpdate
} from "./types";

interface TrackedVisualRegion extends VisualOutputRegion {
  sourceLanguage: string;
  firstSeenAt: number;
  removalTimer?: number;
}

interface QueuedTranslation {
  generation: number;
  trackId: number;
  textRevision: number;
  text: string;
}

export class VisualTranslationController {
  private readonly regions = new Map<number, TrackedVisualRegion>();
  private queue: QueuedTranslation[] = [];
  private generation = 0;
  private translating = false;
  private scanning = false;
  private publishSerial = Promise.resolve();
  private sourceLanguage = "ja";
  private targetLanguage = "en";
  private detectionMode: VisualDetectionMode = "focused";
  private sourceWidth = 1;
  private sourceHeight = 1;

  constructor(
    private readonly translation: TranslationService,
    private readonly publish: (output: VisualOutputPayload) => Promise<void>,
    private readonly reportError: (message: string) => void
  ) {}

  begin(
    sourceLanguage: string,
    targetLanguage: string,
    detectionMode: VisualDetectionMode = "focused"
  ): void {
    if (!supportedTranslationLanguage(sourceLanguage) || !supportedTranslationLanguage(targetLanguage)) {
      throw new Error("Visual translation requires a supported source and target language.");
    }
    this.generation += 1;
    this.sourceLanguage = sourceLanguage;
    this.targetLanguage = targetLanguage;
    this.detectionMode = detectionMode;
    this.cancelRemovalTimers();
    this.regions.clear();
    this.queue = [];
    this.scanning = true;
    this.sourceWidth = 1;
    this.sourceHeight = 1;
    void this.render();
  }

  update(update: VisualTextUpdate): void {
    const now = Date.now();
    this.scanning = false;
    this.sourceWidth = Math.max(1, update.source.width);
    this.sourceHeight = Math.max(1, update.source.height);
    const visibleTrackIds = new Set(update.visible.map(({ trackId }) => trackId));
    for (const [trackId, region] of this.regions) {
      if (!visibleTrackIds.has(trackId)) this.retainOrRemove(region, now);
    }
    for (const region of update.visible) this.mergeVisible(region, now);
    this.refreshQueue();
    void this.render();
    void this.pump();
  }

  clear(): void {
    this.reset(false);
  }

  rescan(): void {
    this.reset(true);
  }

  private reset(scanning: boolean): void {
    this.generation += 1;
    this.cancelRemovalTimers();
    this.regions.clear();
    this.queue = [];
    this.scanning = scanning;
    void this.render();
  }

  private mergeVisible(region: StableVisualTextRegion, now: number): void {
    const previous = this.regions.get(region.trackId);
    const sameRevision = previous?.textRevision === region.textRevision
      && previous.original === region.text;
    for (const [trackId, candidate] of this.regions) {
      if (trackId !== region.trackId
        && candidate.retained
        && intersectionOverUnion(candidate.bounds, region.bounds) >= 0.45) {
        this.removeRegion(trackId);
      }
    }
    if (previous?.removalTimer !== undefined) window.clearTimeout(previous.removalTimer);
    this.regions.set(region.trackId, {
      trackId: region.trackId,
      textRevision: region.textRevision,
      original: region.text,
      translation: sameRevision ? previous.translation : undefined,
      translationPending: sameRevision ? previous.translationPending : true,
      retained: false,
      bounds: region.bounds,
      sourceLanguage: region.language ?? this.sourceLanguage,
      firstSeenAt: sameRevision ? previous.firstSeenAt : now
    });
  }

  private retainOrRemove(region: TrackedVisualRegion, now: number): void {
    if (region.retained) return;
    const visibleFor = now - region.firstSeenAt;
    if (visibleFor >= 12_000) {
      this.removeRegion(region.trackId);
      return;
    }
    region.retained = true;
    region.removalTimer = window.setTimeout(() => {
      const latest = this.regions.get(region.trackId);
      if (!latest?.retained || latest.textRevision !== region.textRevision) return;
      this.removeRegion(region.trackId);
      this.refreshQueue();
      void this.render();
    }, 8_000);
  }

  private removeRegion(trackId: number): void {
    const region = this.regions.get(trackId);
    if (region?.removalTimer !== undefined) window.clearTimeout(region.removalTimer);
    this.regions.delete(trackId);
  }

  private cancelRemovalTimers(): void {
    for (const region of this.regions.values()) {
      if (region.removalTimer !== undefined) window.clearTimeout(region.removalTimer);
    }
  }

  private refreshQueue(): void {
    const maximumPending = this.detectionMode === "focused" ? 6 : 12;
    this.queue = [...this.regions.values()]
      .filter(({ retained, translationPending }) => !retained && translationPending)
      .sort((left, right) => this.regionPriority(right) - this.regionPriority(left))
      .slice(0, maximumPending)
      .map((region) => ({
        generation: this.generation,
        trackId: region.trackId,
        textRevision: region.textRevision,
        text: region.original
      }));
  }

  private regionPriority(region: TrackedVisualRegion): number {
    const relativeHeight = region.bounds.height / this.sourceHeight;
    const relativeArea = (region.bounds.width * region.bounds.height)
      / (this.sourceWidth * this.sourceHeight);
    return relativeHeight * 8 + relativeArea * 2 + Math.min(region.original.length, 80) / 800;
  }

  private async pump(): Promise<void> {
    if (this.translating) return;
    this.translating = true;
    try {
      while (this.queue.length > 0) {
        const request = this.queue.shift();
        if (!request || request.generation !== this.generation) continue;
        const current = this.regions.get(request.trackId);
        if (!current
          || current.textRevision !== request.textRevision
          || !current.translationPending) continue;
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
      .map(({
        sourceLanguage: _sourceLanguage,
        firstSeenAt: _firstSeenAt,
        removalTimer: _removalTimer,
        ...region
      }) => region);
    const output: VisualOutputPayload = {
      sourceWidth: this.sourceWidth,
      sourceHeight: this.sourceHeight,
      sourceLanguage: this.sourceLanguage,
      targetLanguage: this.targetLanguage,
      scanning: this.scanning,
      regions
    };
    this.publishSerial = this.publishSerial
      .then(() => this.publish(output), () => this.publish(output))
      .catch((error) => {
        this.reportError(error instanceof Error ? error.message : String(error));
      });
    return this.publishSerial;
  }
}

function intersectionOverUnion(left: VisualOutputRegion["bounds"], right: VisualOutputRegion["bounds"]): number {
  const intersectionWidth = Math.max(
    0,
    Math.min(left.x + left.width, right.x + right.width) - Math.max(left.x, right.x)
  );
  const intersectionHeight = Math.max(
    0,
    Math.min(left.y + left.height, right.y + right.height) - Math.max(left.y, right.y)
  );
  const intersection = intersectionWidth * intersectionHeight;
  const union = left.width * left.height + right.width * right.height - intersection;
  return union > 0 ? intersection / union : 0;
}
