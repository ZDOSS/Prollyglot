import {
  TRANSLATION_MODELS,
  translationModelsForRoute
} from "./translation-catalog";
import { languageLabel, type TranslationLanguage } from "./language-catalog";
import type {
  TranslationControlCommand,
  TranslationControlRequest,
  TranslationControlResponse,
  TranslationInferenceCommand,
  TranslationInferenceRequest,
  TranslationInferenceResponse
} from "./translation-protocol";
import {
  TranslationExecutorTerminatedError,
  TranslationScheduler,
  isExpectedTranslationCancellation,
  type TranslationExecutor,
  type TranslationJobRequest,
  type TranslationPreparation,
  type TranslationTelemetry,
  type TranslationWorkloadProfile
} from "./translation-scheduler";
import type { TranslationCatalogStatus, TranslationModelStatus } from "./types";

type CatalogListener = (catalog: TranslationCatalogStatus) => void;
type TelemetryListener = (telemetry: TranslationTelemetry) => void;

interface PendingRequest {
  resolve: (result: unknown) => void;
  reject: (error: Error) => void;
}

export interface SessionTranslationRequest {
  sourceRevision: number;
  workloadProfile: TranslationWorkloadProfile;
  sourceLanguage: TranslationLanguage;
  targetLanguage: TranslationLanguage;
  text: string;
  coalesceKey: string;
  onStarted?: () => void;
}

export function translationStatusForRoute(
  catalog: TranslationCatalogStatus,
  sourceLanguage: TranslationLanguage,
  targetLanguage: TranslationLanguage
): TranslationModelStatus | undefined {
  const candidates = translationModelsForRoute(sourceLanguage, targetLanguage);
  const statuses = candidates
    .map((candidate) => catalog.models.find(({ modelId }) => modelId === candidate.modelId))
    .filter((status): status is TranslationModelStatus => status !== undefined);
  return statuses.find(({ phase }) => phase === "ready" || phase === "loading") ?? statuses[0];
}

export class TranslationSession {
  private closed = false;

  constructor(
    private readonly service: TranslationService,
    readonly id: string
  ) {}

  prepare(
    sourceLanguage: TranslationLanguage,
    targetLanguage: TranslationLanguage
  ): Promise<void> {
    this.requireOpen();
    return this.service.prepareSession(this.id, sourceLanguage, targetLanguage);
  }

  translate(request: SessionTranslationRequest): Promise<string> {
    this.requireOpen();
    return this.service.translateInSession({ ...request, sessionId: this.id });
  }

  restart(reason: string): void {
    this.requireOpen();
    this.service.restartSession(this.id, reason);
  }

  cancelQueued(coalesceKey: string, reason: string): void {
    this.requireOpen();
    this.service.cancelQueued(this.id, coalesceKey, reason);
  }

  close(reason?: string): void {
    if (this.closed) return;
    this.closed = true;
    this.service.closeSession(this.id, reason);
  }

  private requireOpen(): void {
    if (this.closed) throw new Error("The translation session is closed.");
  }
}

export class TranslationService {
  private controlWorker?: Worker;
  private readonly mock: boolean;
  private readonly listeners = new Set<CatalogListener>();
  private readonly telemetryListeners = new Set<TelemetryListener>();
  private readonly controlPending = new Map<number, PendingRequest>();
  private readonly preparedMockModels = new Set<string>();
  private readonly scheduler: TranslationScheduler;
  private nextControlRequestId = 1;
  private nextSessionId = 1;
  private catalog: TranslationCatalogStatus;

  constructor(mock = false) {
    this.mock = mock;
    this.catalog = {
      models: TRANSLATION_MODELS.map((model) => ({
        phase: mock ? "notInstalled" : "checking",
        kind: model.kind,
        sourceLanguages: [...model.sourceLanguages],
        targetLanguages: [...model.targetLanguages],
        modelId: model.modelId,
        displayName: model.displayName,
        license: model.license,
        downloadedBytes: 0,
        totalBytes: model.totalBytes,
        message: mock ? undefined : "Checking local model files…"
      }))
    };
    this.scheduler = new TranslationScheduler(
      (sessionId) => this.createInferenceExecutor(sessionId),
      (telemetry) => this.publishTelemetry(telemetry)
    );
    if (!mock) this.startControlWorker();
  }

  snapshot(): TranslationCatalogStatus {
    return structuredClone(this.catalog);
  }

  routeStatus(
    sourceLanguage: TranslationLanguage,
    targetLanguage: TranslationLanguage
  ): TranslationModelStatus | undefined {
    return translationStatusForRoute(this.catalog, sourceLanguage, targetLanguage);
  }

  async initialize(): Promise<TranslationCatalogStatus> {
    if (this.mock) return this.snapshot();
    await this.controlRequest({ type: "status" });
    return this.snapshot();
  }

  subscribe(listener: CatalogListener): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  subscribeTelemetry(listener: TelemetryListener): () => void {
    this.telemetryListeners.add(listener);
    return () => this.telemetryListeners.delete(listener);
  }

  openSession(kind: "captions" | "visual"): TranslationSession {
    const sessionId = `${kind}:${this.nextSessionId++}`;
    this.scheduler.startSession(sessionId);
    return new TranslationSession(this, sessionId);
  }

  async install(modelId: string): Promise<void> {
    if (this.mock) {
      this.preparedMockModels.delete(modelId);
      await this.mockInstall(modelId);
      return;
    }
    await this.controlRequest({ type: "install", modelId });
  }

  async remove(modelId: string): Promise<void> {
    if (this.mock) {
      this.preparedMockModels.delete(modelId);
      this.updateMock(modelId, {
        phase: "notInstalled",
        downloadedBytes: 0,
        message: undefined
      });
      return;
    }
    await this.controlRequest({ type: "remove", modelId });
  }

  prepareSession(
    sessionId: string,
    sourceLanguage: TranslationLanguage,
    targetLanguage: TranslationLanguage
  ): Promise<void> {
    return this.scheduler.prepare(sessionId, sourceLanguage, targetLanguage);
  }

  translateInSession(request: TranslationJobRequest): Promise<string> {
    return this.scheduler.submit(request);
  }

  restartSession(sessionId: string, reason: string): void {
    this.scheduler.restartSession(sessionId, reason);
  }

  cancelQueued(sessionId: string, coalesceKey: string, reason: string): void {
    this.scheduler.cancelQueued(sessionId, coalesceKey, reason);
  }

  closeSession(sessionId: string, reason?: string): void {
    this.scheduler.stopSession(sessionId, reason);
  }

  private createInferenceExecutor(sessionId: string): TranslationExecutor {
    if (this.mock) {
      return new MockTranslationExecutor(
        () => this.catalog,
        this.preparedMockModels,
        sessionId
      );
    }
    return new WorkerTranslationExecutor(sessionId);
  }

  private controlRequest(request: TranslationControlCommand): Promise<unknown> {
    if (!this.controlWorker) {
      return Promise.reject(new Error("Translation model control is unavailable."));
    }
    const requestId = this.nextControlRequestId++;
    return new Promise((resolve, reject) => {
      this.controlPending.set(requestId, { resolve, reject });
      this.controlWorker?.postMessage({ ...request, requestId } as TranslationControlRequest);
    });
  }

  private startControlWorker(): void {
    const worker = new Worker(new URL("./translation.worker.ts", import.meta.url), {
      type: "module"
    });
    this.controlWorker = worker;
    worker.addEventListener("message", ({ data }: MessageEvent<TranslationControlResponse>) => {
      if (this.controlWorker === worker) this.handleControlMessage(data);
    });
    worker.addEventListener("error", (event) => {
      if (this.controlWorker !== worker) return;
      const error = new Error(event.message || "Translation model control stopped unexpectedly.");
      this.stopControlWorker(error);
      this.catalog = {
        models: this.catalog.models.map((model) => ({
          ...model,
          phase: "failed",
          message: error.message
        }))
      };
      this.publish();
    });
  }

  private stopControlWorker(error: Error): void {
    const worker = this.controlWorker;
    this.controlWorker = undefined;
    worker?.terminate();
    for (const request of this.controlPending.values()) request.reject(error);
    this.controlPending.clear();
  }

  private handleControlMessage(message: TranslationControlResponse): void {
    if (message.type === "catalog") {
      this.catalog = structuredClone(message.catalog);
      this.publish();
      return;
    }
    const request = this.controlPending.get(message.requestId);
    if (!request) return;
    this.controlPending.delete(message.requestId);
    if (message.ok) request.resolve(message.result);
    else request.reject(new Error(message.error));
  }

  private async mockInstall(modelId: string): Promise<void> {
    const status = this.requiredMock(modelId);
    this.updateMock(modelId, {
      phase: "downloading",
      downloadedBytes: 0,
      message: "Downloading and verifying local translation model…"
    });
    await new Promise((resolve) => globalThis.setTimeout(resolve, 220));
    this.updateMock(modelId, {
      phase: "downloading",
      downloadedBytes: Math.round(status.totalBytes * 0.58),
      message: "Downloading and verifying model weights…"
    });
    await new Promise((resolve) => globalThis.setTimeout(resolve, 380));
    this.updateMock(modelId, {
      phase: "ready",
      downloadedBytes: status.totalBytes,
      message: undefined
    });
  }

  private requiredMock(modelId: string): TranslationModelStatus {
    const status = this.catalog.models.find((model) => model.modelId === modelId);
    if (!status) throw new Error(`Unknown local translation model ${modelId}.`);
    return status;
  }

  private updateMock(modelId: string, patch: Partial<TranslationModelStatus>): void {
    Object.assign(this.requiredMock(modelId), patch);
    this.publish();
  }

  private publish(): void {
    const snapshot = this.snapshot();
    for (const listener of this.listeners) listener(snapshot);
  }

  private publishTelemetry(telemetry: TranslationTelemetry): void {
    for (const listener of this.telemetryListeners) listener(telemetry);
  }
}

class WorkerTranslationExecutor implements TranslationExecutor {
  private worker?: Worker;
  private nextRequestId = 1;
  private readonly pending = new Map<number, PendingRequest>();

  constructor(private readonly sessionId: string) {
    this.start();
  }

  async prepare(
    sourceLanguage: TranslationLanguage,
    targetLanguage: TranslationLanguage
  ): Promise<TranslationPreparation> {
    const result = await this.request({ type: "prepare", sourceLanguage, targetLanguage });
    if (!isTranslationPreparation(result)) {
      throw new Error("The local translator returned invalid preparation telemetry.");
    }
    return result;
  }

  async translate(
    sourceLanguage: TranslationLanguage,
    targetLanguage: TranslationLanguage,
    text: string
  ): Promise<string> {
    const result = await this.request({
      type: "translate",
      sourceLanguage,
      targetLanguage,
      text
    });
    if (typeof result !== "string") {
      throw new Error("The local translator returned an invalid result.");
    }
    return result;
  }

  terminate(reason: Error): void {
    const worker = this.worker;
    this.worker = undefined;
    worker?.terminate();
    for (const request of this.pending.values()) request.reject(reason);
    this.pending.clear();
  }

  private start(): void {
    const worker = new Worker(new URL("./translation-inference.worker.ts", import.meta.url), {
      type: "module",
      name: `prollyglot-translation-${this.sessionId}`
    });
    this.worker = worker;
    worker.addEventListener("message", ({ data }: MessageEvent<TranslationInferenceResponse>) => {
      if (this.worker !== worker) return;
      const request = this.pending.get(data.requestId);
      if (!request) return;
      this.pending.delete(data.requestId);
      if (data.ok) {
        request.resolve(data.type === "reply"
          ? data.result
          : { modelId: data.modelId, coldStartMs: data.coldStartMs });
      } else {
        request.reject(new Error(data.error));
      }
    });
    worker.addEventListener("error", (event) => {
      if (this.worker !== worker) return;
      this.terminate(new TranslationExecutorTerminatedError(
        event.message || "The local translation inference worker stopped unexpectedly."
      ));
    });
  }

  private request(request: TranslationInferenceCommand): Promise<unknown> {
    if (!this.worker) {
      return Promise.reject(new TranslationExecutorTerminatedError(
        "The local translation inference worker is unavailable."
      ));
    }
    const requestId = this.nextRequestId++;
    return new Promise((resolve, reject) => {
      this.pending.set(requestId, { resolve, reject });
      this.worker?.postMessage({ ...request, requestId } as TranslationInferenceRequest);
    });
  }
}

class MockTranslationExecutor implements TranslationExecutor {
  private terminated?: Error;

  constructor(
    private readonly catalog: () => TranslationCatalogStatus,
    private readonly preparedModels: Set<string>,
    readonly sessionId: string
  ) {}

  async prepare(
    sourceLanguage: TranslationLanguage,
    targetLanguage: TranslationLanguage
  ): Promise<TranslationPreparation> {
    this.requireActive();
    const status = translationStatusForRoute(this.catalog(), sourceLanguage, targetLanguage);
    if (!status || status.phase !== "ready") {
      throw new Error(`${status?.displayName ?? "The selected translator"} is not installed and ready.`);
    }
    if (this.preparedModels.has(status.modelId)) {
      return { modelId: status.modelId, coldStartMs: 0 };
    }
    const startedAt = Date.now();
    await new Promise((resolve) => globalThis.setTimeout(resolve, 180));
    this.requireActive();
    this.preparedModels.add(status.modelId);
    return { modelId: status.modelId, coldStartMs: Date.now() - startedAt };
  }

  async translate(
    sourceLanguage: TranslationLanguage,
    targetLanguage: TranslationLanguage,
    text: string
  ): Promise<string> {
    this.requireActive();
    await new Promise((resolve) => globalThis.setTimeout(resolve, 260));
    this.requireActive();
    return mockTranslation(sourceLanguage, targetLanguage, text);
  }

  terminate(reason: Error): void {
    this.terminated = reason;
  }

  private requireActive(): void {
    if (this.terminated) throw this.terminated;
  }
}

function mockTranslation(
  sourceLanguage: TranslationLanguage,
  targetLanguage: TranslationLanguage,
  text: string
): string {
  const known: Record<string, string> = {
    "ja:en:今朝、新しい計画が発表されました。": "A new plan was announced this morning.",
    "ja:en:今日は何をする予定ですか？": "What are you planning to do today?",
    "es:en:Las ventanas azules se abren sobre el jardín.": "Blue windows open above the garden."
  };
  const key = `${sourceLanguage}:${targetLanguage}:${text.trim()}`;
  return known[key]
    ?? `${languageLabel(sourceLanguage)} → ${languageLabel(targetLanguage)}: ${text.trim()}`;
}

function isTranslationPreparation(value: unknown): value is TranslationPreparation {
  if (!value || typeof value !== "object") return false;
  const candidate = value as Partial<TranslationPreparation>;
  return typeof candidate.modelId === "string"
    && typeof candidate.coldStartMs === "number";
}

export { isExpectedTranslationCancellation };
export type { TranslationTelemetry, TranslationWorkloadProfile };
