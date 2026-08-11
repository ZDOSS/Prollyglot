import {
  TRANSLATION_MODELS,
  translationModelsForRoute
} from "./translation-catalog";
import { languageLabel, type TranslationLanguage } from "./language-catalog";
import type {
  TranslationWorkerCommand,
  TranslationWorkerRequest,
  TranslationWorkerResponse
} from "./translation-protocol";
import type { TranslationCatalogStatus, TranslationModelStatus } from "./types";

type CatalogListener = (catalog: TranslationCatalogStatus) => void;

interface PendingRequest {
  resolve: (result: unknown) => void;
  reject: (error: Error) => void;
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

export class TranslationService {
  private worker?: Worker;
  private readonly mock: boolean;
  private readonly listeners = new Set<CatalogListener>();
  private readonly pending = new Map<number, PendingRequest>();
  private readonly preparing = new Map<string, Promise<void>>();
  private readonly preparedMockModels = new Set<string>();
  private restarting?: Promise<void>;
  private nextRequestId = 1;
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
    if (!mock) this.startWorker();
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
    await this.request({ type: "status" });
    return this.snapshot();
  }

  subscribe(listener: CatalogListener): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  async install(modelId: string): Promise<void> {
    if (this.mock) {
      this.preparedMockModels.delete(modelId);
      await this.mockInstall(modelId);
      return;
    }
    await this.request({ type: "install", modelId });
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
    await this.request({ type: "remove", modelId });
  }

  prepare(
    sourceLanguage: TranslationLanguage,
    targetLanguage: TranslationLanguage
  ): Promise<void> {
    const key = `${sourceLanguage}:${targetLanguage}`;
    const existing = this.preparing.get(key);
    if (existing) return existing;
    const operation = Promise.resolve().then(() =>
      this.prepareUncached(sourceLanguage, targetLanguage)
    );
    this.preparing.set(key, operation);
    void operation.finally(() => {
      if (this.preparing.get(key) === operation) this.preparing.delete(key);
    }).catch(() => undefined);
    return operation;
  }

  async translate(
    sourceLanguage: TranslationLanguage,
    targetLanguage: TranslationLanguage,
    text: string
  ): Promise<string> {
    if (this.mock) {
      await new Promise((resolve) => window.setTimeout(resolve, 260));
      return mockTranslation(sourceLanguage, targetLanguage, text);
    }
    const result = await this.request({
      type: "translate",
      sourceLanguage,
      targetLanguage,
      text
    });
    if (typeof result !== "string") throw new Error("The local translator returned an invalid result.");
    return result;
  }

  restart(reason = "The local translator was restarted after it stopped responding."): Promise<void> {
    if (this.mock) return Promise.resolve();
    if (this.restarting) return this.restarting;
    const operation = Promise.resolve().then(async () => {
      this.stopWorker(new Error(reason));
      this.startWorker();
      await this.requestNow({ type: "status" });
    });
    this.restarting = operation;
    void operation.finally(() => {
      if (this.restarting === operation) this.restarting = undefined;
    }).catch(() => undefined);
    return operation;
  }

  restartIfBusy(reason: string): Promise<void> {
    if (this.mock || (this.pending.size === 0 && this.preparing.size === 0)) {
      return Promise.resolve();
    }
    return this.restart(reason);
  }

  private async prepareUncached(
    sourceLanguage: TranslationLanguage,
    targetLanguage: TranslationLanguage
  ): Promise<void> {
    if (this.mock) {
      const status = this.routeStatus(sourceLanguage, targetLanguage);
      if (!status || status.phase !== "ready") {
        throw new Error(`${status?.displayName ?? "The selected translator"} is not installed and ready.`);
      }
      if (this.preparedMockModels.has(status.modelId)) return;
      this.updateMock(status.modelId, {
        phase: "loading",
        message: `Loading ${status.displayName} locally…`
      });
      await new Promise((resolve) => window.setTimeout(resolve, 180));
      this.preparedMockModels.add(status.modelId);
      this.updateMock(status.modelId, { phase: "ready", message: undefined });
      return;
    }
    await this.request({ type: "prepare", sourceLanguage, targetLanguage });
  }

  private async request(request: TranslationWorkerCommand): Promise<unknown> {
    if (this.restarting) await this.restarting;
    return this.requestNow(request);
  }

  private requestNow(request: TranslationWorkerCommand): Promise<unknown> {
    if (!this.worker) return Promise.reject(new Error("The local translation worker is unavailable."));
    const requestId = this.nextRequestId++;
    return new Promise((resolve, reject) => {
      this.pending.set(requestId, { resolve, reject });
      this.worker?.postMessage({ ...request, requestId } as TranslationWorkerRequest);
    });
  }

  private startWorker(): void {
    const worker = new Worker(new URL("./translation.worker.ts", import.meta.url), { type: "module" });
    this.worker = worker;
    worker.addEventListener("message", ({ data }: MessageEvent<TranslationWorkerResponse>) => {
      if (this.worker === worker) this.handleWorkerMessage(data);
    });
    worker.addEventListener("error", (event) => {
      if (this.worker !== worker) return;
      const error = new Error(event.message || "The local translation worker stopped unexpectedly.");
      this.stopWorker(error);
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

  private stopWorker(error: Error): void {
    const worker = this.worker;
    this.worker = undefined;
    worker?.terminate();
    for (const request of this.pending.values()) request.reject(error);
    this.pending.clear();
    this.preparing.clear();
  }

  private handleWorkerMessage(message: TranslationWorkerResponse): void {
    if (message.type === "catalog") {
      this.catalog = structuredClone(message.catalog);
      this.publish();
      return;
    }
    const request = this.pending.get(message.requestId);
    if (!request) return;
    this.pending.delete(message.requestId);
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
    await new Promise((resolve) => window.setTimeout(resolve, 220));
    this.updateMock(modelId, {
      phase: "downloading",
      downloadedBytes: Math.round(status.totalBytes * 0.58),
      message: "Downloading and verifying model weights…"
    });
    await new Promise((resolve) => window.setTimeout(resolve, 380));
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
