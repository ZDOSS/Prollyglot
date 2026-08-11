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
  queuedAt: number;
}

const FOCUSED_REGION_BUDGET = 6;
const ALL_TEXT_REGION_BUDGET = 12;
const COMPACT_TRANSLATION_DEADLINE_MS = 5_000;
const UNIVERSAL_TRANSLATION_DEADLINE_MS = 12_000;
const DIAGNOSTIC_INTERVAL_MS = 15_000;

class VisualTranslationTimeoutError extends Error {}

export class VisualTranslationController {
  private readonly regions = new Map<number, TrackedVisualRegion>();
  private queue: QueuedTranslation[] = [];
  private active?: QueuedTranslation;
  private generation = 0;
  private translating = false;
  private scanning = false;
  private publishSerial = Promise.resolve();
  private sourceLanguage = "ja";
  private targetLanguage = "en";
  private detectionMode: VisualDetectionMode = "focused";
  private sourceWidth = 1;
  private sourceHeight = 1;
  private lastDiagnosticAt = 0;
  private reportedFirstTranslation = false;

  constructor(
    private readonly translation: TranslationService,
    private readonly publish: (output: VisualOutputPayload) => Promise<void>,
    private readonly reportError: (message: string) => void,
    private readonly reportDiagnostic: (message: string) => void = () => undefined
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
    this.active = undefined;
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
    const requested = new Set(update.translationRequests.map(regionKey));
    const visibleTrackIds = new Set(update.visible.map(({ trackId }) => trackId));
    for (const [trackId, region] of this.regions) {
      if (!visibleTrackIds.has(trackId)) this.retainOrRemove(region, now);
    }
    for (const region of update.visible) {
      this.mergeVisible(region, now, requested.has(regionKey(region)));
    }
    this.pruneRegions();
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
    this.active = undefined;
    this.scanning = scanning;
    void this.render();
  }

  private mergeVisible(
    region: StableVisualTextRegion,
    now: number,
    translationRequested: boolean
  ): void {
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
      translationPending: sameRevision
        ? previous.translationPending
        : translationRequested || previous === undefined,
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
    const queuedAt = new Map(this.queue.map((request) => [regionKey(request), request.queuedAt]));
    const activeKey = this.active ? regionKey(this.active) : undefined;
    this.queue = [...this.regions.values()]
      .filter(({ retained, translationPending }) => !retained && translationPending)
      .filter((region) => regionKey(region) !== activeKey)
      .sort((left, right) => this.regionPriority(right) - this.regionPriority(left))
      .slice(0, this.regionBudget())
      .map((region) => ({
        generation: this.generation,
        trackId: region.trackId,
        textRevision: region.textRevision,
        text: region.original,
        queuedAt: queuedAt.get(regionKey(region)) ?? Date.now()
      }));
  }

  private pruneRegions(): void {
    const activeTrackId = this.active?.generation === this.generation
      ? this.active.trackId
      : undefined;
    const ranked = [...this.regions.values()]
      .sort((left, right) => this.regionPriority(right) - this.regionPriority(left));
    const keep = new Set(
      ranked.slice(0, this.regionBudget()).map(({ trackId }) => trackId)
    );
    if (activeTrackId !== undefined
      && this.regions.has(activeTrackId)
      && !keep.has(activeTrackId)) {
      const lowestKept = [...ranked]
        .reverse()
        .find(({ trackId }) => keep.has(trackId));
      if (lowestKept) keep.delete(lowestKept.trackId);
      keep.add(activeTrackId);
    }
    for (const trackId of this.regions.keys()) {
      if (!keep.has(trackId)) this.removeRegion(trackId);
    }
  }

  private regionBudget(): number {
    return this.detectionMode === "focused"
      ? FOCUSED_REGION_BUDGET
      : ALL_TEXT_REGION_BUDGET;
  }

  private regionPriority(region: TrackedVisualRegion): number {
    const relativeHeight = region.bounds.height / this.sourceHeight;
    const relativeArea = (region.bounds.width * region.bounds.height)
      / (this.sourceWidth * this.sourceHeight);
    const textLength = [...region.original].length;
    const shortTextSignal = Math.min(textLength, 48) / 800;
    const longTextCost = Math.max(0, textLength - 96) / 600;
    return relativeHeight * 8 + relativeArea * 2 + shortTextSignal - longTextCost;
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
        } catch (error) {
          if (request.generation === this.generation) {
            const latest = this.regions.get(request.trackId);
            if (latest?.textRevision === request.textRevision) {
              latest.translationPending = false;
            }
            this.reportError(error instanceof Error ? error.message : String(error));
            this.refreshQueue();
            await this.render();
          }
          continue;
        }
        this.active = request;
        await this.render();
        const startedAt = Date.now();
        let timedOut = false;
        try {
          const translated = await this.translateWithDeadline(
            sourceLanguage,
            targetLanguage,
            request
          );
          if (request.generation !== this.generation) continue;
          const latest = this.regions.get(request.trackId);
          if (!latest || latest.textRevision !== request.textRevision) continue;
          latest.translation = translated;
          latest.translationPending = false;
        } catch (error) {
          timedOut = error instanceof VisualTranslationTimeoutError;
          if (request.generation !== this.generation) continue;
          const latest = this.regions.get(request.trackId);
          if (latest?.textRevision === request.textRevision) {
            latest.translationPending = false;
          }
          this.reportError(error instanceof Error ? error.message : String(error));
        } finally {
          if (this.active && requestKey(this.active) === requestKey(request)) {
            this.active = undefined;
          }
          this.reportTiming(
            sourceLanguage,
            targetLanguage,
            startedAt - request.queuedAt,
            Date.now() - startedAt,
            timedOut
          );
          this.refreshQueue();
          await this.render();
        }
      }
    } finally {
      this.translating = false;
      if (this.queue.length > 0) void this.pump();
    }
  }

  private async translateWithDeadline(
    sourceLanguage: NonNullable<ReturnType<typeof supportedTranslationLanguage>>,
    targetLanguage: NonNullable<ReturnType<typeof supportedTranslationLanguage>>,
    request: QueuedTranslation
  ): Promise<string> {
    const universal = this.translation.routeStatus(sourceLanguage, targetLanguage)?.kind === "manyToMany";
    const deadlineMs = universal
      ? UNIVERSAL_TRANSLATION_DEADLINE_MS
      : COMPACT_TRANSLATION_DEADLINE_MS;
    let timer: number | undefined;
    const timeout = new Promise<never>((_resolve, reject) => {
      timer = window.setTimeout(() => {
        reject(new VisualTranslationTimeoutError(
          `Visual translation did not finish within ${deadlineMs / 1_000} seconds; the local translator was restarted.`
        ));
      }, deadlineMs);
    });
    try {
      return await Promise.race([
        this.translation.translate(sourceLanguage, targetLanguage, request.text),
        timeout
      ]);
    } catch (error) {
      if (error instanceof VisualTranslationTimeoutError
        && request.generation === this.generation) {
        await this.translation.restart(error.message);
      }
      throw error;
    } finally {
      if (timer !== undefined) window.clearTimeout(timer);
    }
  }

  private reportTiming(
    sourceLanguage: string,
    targetLanguage: string,
    queueWaitMs: number,
    inferenceMs: number,
    timedOut: boolean
  ): void {
    const now = Date.now();
    const slow = timedOut || queueWaitMs >= 2_000 || inferenceMs >= 2_000;
    if (this.reportedFirstTranslation
      && (!slow || now - this.lastDiagnosticAt < DIAGNOSTIC_INTERVAL_MS)) return;
    this.reportedFirstTranslation = true;
    this.lastDiagnosticAt = now;
    this.reportDiagnostic(
      `${sourceLanguage} to ${targetLanguage} visual translation ${timedOut ? "timed out" : "completed"} `
      + `in ${inferenceMs} ms after ${queueWaitMs} ms queued; ${this.queue.length} region(s) remain queued.`
    );
  }

  private render(): Promise<void> {
    const regions = [...this.regions.values()]
      .filter((region) => region.translation !== undefined || this.isActive(region))
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

  private isActive(region: TrackedVisualRegion): boolean {
    return this.active?.generation === this.generation
      && this.active.trackId === region.trackId
      && this.active.textRevision === region.textRevision;
  }
}

function regionKey(region: Pick<TrackedVisualRegion, "trackId" | "textRevision">
  | Pick<StableVisualTextRegion, "trackId" | "textRevision">): string {
  return `${region.trackId}:${region.textRevision}`;
}

function requestKey(request: Pick<QueuedTranslation, "generation" | "trackId" | "textRevision">): string {
  return `${request.generation}:${request.trackId}:${request.textRevision}`;
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
