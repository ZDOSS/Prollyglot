import { supportedTranslationLanguage, type TranslationLanguage } from "./language-catalog";
import { LatestPublisher } from "./latest-publisher";
import {
  TranslationService,
  TranslationSession,
  isExpectedTranslationCancellation
} from "./translation";
import type {
  StableVisualTextRegion,
  VisualDetectionMode,
  VisualPresentationFrame,
  VisualPresentationRegion,
  VisualTextUpdate
} from "./types";

interface TrackedVisualRegion extends VisualPresentationRegion {
  sourceLanguage: string;
  firstSeenAt: number;
  removalTimer?: number;
}

export interface VisualPresentationEpoch {
  sessionId: number;
  runtimeRevision: number;
}

interface ActiveRequest {
  generation: number;
  sessionId: string;
  trackId: number;
  textRevision: number;
}

const FOCUSED_REGION_BUDGET = 6;
const ALL_TEXT_REGION_BUDGET = 12;

export class VisualTranslationController {
  private readonly regions = new Map<number, TrackedVisualRegion>();
  private readonly requests = new Map<string, ActiveRequest>();
  private generation = 0;
  private session?: TranslationSession;
  private scanning = false;
  private sourceLanguage: TranslationLanguage = "ja";
  private targetLanguage: TranslationLanguage = "en";
  private detectionMode: VisualDetectionMode = "focused";
  private sourceWidth = 1;
  private sourceHeight = 1;
  private activeRequestKey?: string;
  private presentationEpoch?: VisualPresentationEpoch;
  private presentationRevision = 0;
  private readonly outputPublisher: LatestPublisher<VisualPresentationFrame>;

  constructor(
    private readonly translation: TranslationService,
    publish: (frame: VisualPresentationFrame) => Promise<void>,
    private readonly reportError: (message: string) => void,
    private readonly reportDiagnostic: (message: string) => void = () => undefined
  ) {
    this.outputPublisher = new LatestPublisher(publish, (error) => {
      this.reportError(error instanceof Error ? error.message : String(error));
    });
  }

  setPresentationEpoch(epoch: VisualPresentationEpoch | undefined): void {
    if (!epoch) {
      this.presentationEpoch = undefined;
      return;
    }
    if (this.presentationEpoch?.sessionId !== epoch.sessionId) {
      this.presentationRevision = 0;
    }
    this.presentationEpoch = { ...epoch };
    this.render();
  }

  begin(
    sourceLanguage: string,
    targetLanguage: string,
    detectionMode: VisualDetectionMode = "focused"
  ): void {
    const supportedSource = supportedTranslationLanguage(sourceLanguage);
    const supportedTarget = supportedTranslationLanguage(targetLanguage);
    if (!supportedSource || !supportedTarget) {
      throw new Error("Visual translation requires a supported source and target language.");
    }
    this.closeSession("A newer visual translation session started.");
    this.presentationEpoch = undefined;
    this.presentationRevision = 0;
    this.generation += 1;
    this.sourceLanguage = supportedSource;
    this.targetLanguage = supportedTarget;
    this.detectionMode = detectionMode;
    this.cancelRemovalTimers();
    this.regions.clear();
    this.requests.clear();
    this.activeRequestKey = undefined;
    this.scanning = true;
    this.sourceWidth = 1;
    this.sourceHeight = 1;
    this.openSession();
    this.render();
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
    this.scheduleTranslations(update.runtimeRevision);
    this.render();
  }

  clear(): void {
    this.reset(false, true);
  }

  rescan(): void {
    this.reset(true, false);
  }

  private reset(scanning: boolean, closeSession: boolean): void {
    this.generation += 1;
    this.cancelRemovalTimers();
    for (const region of this.regions.values()) this.cancelQueued(region.trackId);
    this.regions.clear();
    this.requests.clear();
    this.activeRequestKey = undefined;
    this.scanning = scanning;
    if (closeSession) {
      this.closeSession("Visual translation stopped.");
    } else if (scanning) {
      this.closeSession("The visual source revision changed.");
      this.openSession();
    }
    this.render();
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
    if (!sameRevision) this.cancelQueued(region.trackId);
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
      this.render();
    }, 8_000);
  }

  private removeRegion(trackId: number): void {
    const region = this.regions.get(trackId);
    if (region?.removalTimer !== undefined) window.clearTimeout(region.removalTimer);
    this.cancelQueued(trackId);
    this.regions.delete(trackId);
  }

  private cancelQueued(trackId: number): void {
    if (!this.session) return;
    try {
      this.session.cancelQueued(
        visualCoalesceKey(trackId),
        "The recognized visual text is no longer current."
      );
    } catch {
      // Session replacement already provides the same cancellation boundary.
    }
  }

  private cancelRemovalTimers(): void {
    for (const region of this.regions.values()) {
      if (region.removalTimer !== undefined) window.clearTimeout(region.removalTimer);
    }
  }

  private scheduleTranslations(sourceRevision: number): void {
    const session = this.session;
    if (!session) return;
    const ranked = [...this.regions.values()]
      .filter(({ retained, translationPending }) => !retained && translationPending)
      .sort((left, right) => this.regionPriority(right) - this.regionPriority(left))
      .slice(0, this.regionBudget());
    for (const region of ranked) {
      const requestKey = regionKey(region);
      const current = this.requests.get(requestKey);
      if (current?.sessionId === session.id && current.generation === this.generation) continue;
      const sourceLanguage = supportedTranslationLanguage(region.sourceLanguage)
        ?? this.sourceLanguage;
      const request: ActiveRequest = {
        generation: this.generation,
        sessionId: session.id,
        trackId: region.trackId,
        textRevision: region.textRevision
      };
      this.requests.set(requestKey, request);
      void this.translateRegion(
        session,
        request,
        sourceRevision,
        sourceLanguage,
        region.original
      );
    }
  }

  private async translateRegion(
    session: TranslationSession,
    request: ActiveRequest,
    sourceRevision: number,
    sourceLanguage: TranslationLanguage,
    text: string
  ): Promise<void> {
    const requestKey = regionKey(request);
    const startedAt = Date.now();
    try {
      const translated = await session.translate({
        sourceRevision,
        workloadProfile: this.translation.routeStatus(sourceLanguage, this.targetLanguage)?.kind
          === "manyToMany" ? "visualUniversal" : "visualCompact",
        sourceLanguage,
        targetLanguage: this.targetLanguage,
        text,
        coalesceKey: visualCoalesceKey(request.trackId),
        onStarted: () => {
          if (!this.isCurrent(request)) return;
          this.activeRequestKey = requestKey;
          this.render();
        }
      });
      if (!this.isCurrent(request)) return;
      const latest = this.regions.get(request.trackId);
      if (!latest || latest.textRevision !== request.textRevision) return;
      latest.translation = translated;
      latest.translationPending = false;
    } catch (error) {
      if (!this.isCurrent(request)) return;
      const latest = this.regions.get(request.trackId);
      if (latest?.textRevision === request.textRevision) latest.translationPending = false;
      if (!isExpectedTranslationCancellation(error)) {
        this.reportError(error instanceof Error ? error.message : String(error));
      }
    } finally {
      if (this.requests.get(requestKey) === request) this.requests.delete(requestKey);
      if (this.activeRequestKey === requestKey) this.activeRequestKey = undefined;
      this.reportDiagnostic(
        `${sourceLanguage} to ${this.targetLanguage} visual translation settled in `
        + `${Date.now() - startedAt} ms.`
      );
      this.render();
    }
  }

  private isCurrent(request: ActiveRequest): boolean {
    return request.generation === this.generation
      && request.sessionId === this.session?.id;
  }

  private pruneRegions(): void {
    const ranked = [...this.regions.values()]
      .sort((left, right) => this.regionPriority(right) - this.regionPriority(left));
    const keep = new Set(
      ranked.slice(0, this.regionBudget()).map(({ trackId }) => trackId)
    );
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

  private render(): void {
    const epoch = this.presentationEpoch;
    if (!epoch) return;
    const regions = [...this.regions.values()]
      .filter((region) => region.translation !== undefined
        || this.activeRequestKey === regionKey(region))
      .sort((left, right) => left.bounds.y - right.bounds.y || left.bounds.x - right.bounds.x)
      .map(({
        sourceLanguage: _sourceLanguage,
        firstSeenAt: _firstSeenAt,
        removalTimer: _removalTimer,
        ...region
      }) => region);
    this.presentationRevision += 1;
    this.outputPublisher.publish({
      ...epoch,
      presentationRevision: this.presentationRevision,
      sourceWidth: this.sourceWidth,
      sourceHeight: this.sourceHeight,
      sourceLanguage: this.sourceLanguage,
      targetLanguage: this.targetLanguage,
      scanning: this.scanning || (regions.length === 0 && this.requests.size > 0),
      regions
    });
  }

  private closeSession(reason: string): void {
    this.session?.close(reason);
    this.session = undefined;
  }

  private openSession(): void {
    const generation = this.generation;
    const session = this.translation.openSession("visual");
    this.session = session;
    void session.prepare(this.sourceLanguage, this.targetLanguage).catch((error) => {
      if (generation !== this.generation || this.session?.id !== session.id) return;
      if (!isExpectedTranslationCancellation(error)) {
        this.reportError(error instanceof Error ? error.message : String(error));
      }
    });
  }
}

function regionKey(region: Pick<TrackedVisualRegion, "trackId" | "textRevision">
  | Pick<StableVisualTextRegion, "trackId" | "textRevision">
  | Pick<ActiveRequest, "trackId" | "textRevision">): string {
  return `${region.trackId}:${region.textRevision}`;
}

function visualCoalesceKey(trackId: number): string {
  return `visual:${trackId}`;
}

function intersectionOverUnion(
  left: VisualPresentationRegion["bounds"],
  right: VisualPresentationRegion["bounds"]
): number {
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
