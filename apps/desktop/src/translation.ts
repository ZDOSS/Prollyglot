import { TRANSLATION_MODELS } from "./translation-catalog";
import type {
  TranslationWorkerCommand,
  TranslationSourceLanguage,
  TranslationWorkerRequest,
  TranslationWorkerResponse
} from "./translation-protocol";
import type { TranslationCatalogStatus, TranslationModelStatus } from "./types";

type CatalogListener = (catalog: TranslationCatalogStatus) => void;

interface PendingRequest {
  resolve: (result: unknown) => void;
  reject: (error: Error) => void;
}

export class TranslationService {
  private readonly worker?: Worker;
  private readonly mock: boolean;
  private readonly listeners = new Set<CatalogListener>();
  private readonly pending = new Map<number, PendingRequest>();
  private nextRequestId = 1;
  private catalog: TranslationCatalogStatus;

  constructor(mock = false) {
    this.mock = mock;
    this.catalog = {
      models: TRANSLATION_MODELS.map((model) => ({
        phase: mock ? "notInstalled" : "checking",
        sourceLanguage: model.sourceLanguage,
        targetLanguage: model.targetLanguage,
        modelId: model.modelId,
        displayName: model.displayName,
        downloadedBytes: 0,
        totalBytes: model.totalBytes,
        message: mock ? undefined : "Checking local model files…"
      }))
    };
    if (!mock) {
      this.worker = new Worker(new URL("./translation.worker.ts", import.meta.url), { type: "module" });
      this.worker.addEventListener("message", ({ data }: MessageEvent<TranslationWorkerResponse>) => {
        this.handleWorkerMessage(data);
      });
      this.worker.addEventListener("error", (event) => {
        const error = new Error(event.message || "The local translation worker stopped unexpectedly.");
        for (const request of this.pending.values()) request.reject(error);
        this.pending.clear();
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
  }

  snapshot(): TranslationCatalogStatus {
    return structuredClone(this.catalog);
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

  async install(sourceLanguage: TranslationSourceLanguage): Promise<void> {
    if (this.mock) {
      await this.mockInstall(sourceLanguage);
      return;
    }
    await this.request({ type: "install", sourceLanguage });
  }

  async remove(sourceLanguage: TranslationSourceLanguage): Promise<void> {
    if (this.mock) {
      this.updateMock(sourceLanguage, {
        phase: "notInstalled",
        downloadedBytes: 0,
        message: undefined
      });
      return;
    }
    await this.request({ type: "remove", sourceLanguage });
  }

  async translate(sourceLanguage: TranslationSourceLanguage, text: string): Promise<string> {
    if (this.mock) {
      await new Promise((resolve) => window.setTimeout(resolve, 260));
      return mockTranslation(sourceLanguage, text);
    }
    const result = await this.request({ type: "translate", sourceLanguage, text });
    if (typeof result !== "string") throw new Error("The local translator returned an invalid result.");
    return result;
  }

  private request(request: TranslationWorkerCommand): Promise<unknown> {
    if (!this.worker) return Promise.reject(new Error("The local translation worker is unavailable."));
    const requestId = this.nextRequestId++;
    return new Promise((resolve, reject) => {
      this.pending.set(requestId, { resolve, reject });
      this.worker?.postMessage({ ...request, requestId } as TranslationWorkerRequest);
    });
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

  private async mockInstall(sourceLanguage: TranslationSourceLanguage): Promise<void> {
    const status = this.requiredMock(sourceLanguage);
    this.updateMock(sourceLanguage, {
      phase: "downloading",
      downloadedBytes: 0,
      message: "Downloading and verifying local translation model…"
    });
    await new Promise((resolve) => window.setTimeout(resolve, 220));
    this.updateMock(sourceLanguage, {
      phase: "downloading",
      downloadedBytes: Math.round(status.totalBytes * 0.58),
      message: "Downloading and verifying model weights…"
    });
    await new Promise((resolve) => window.setTimeout(resolve, 380));
    this.updateMock(sourceLanguage, {
      phase: "ready",
      downloadedBytes: status.totalBytes,
      message: undefined
    });
  }

  private requiredMock(sourceLanguage: TranslationSourceLanguage): TranslationModelStatus {
    const status = this.catalog.models.find((model) => model.sourceLanguage === sourceLanguage);
    if (!status) throw new Error(`No English translation model supports ${sourceLanguage}.`);
    return status;
  }

  private updateMock(
    sourceLanguage: TranslationSourceLanguage,
    patch: Partial<TranslationModelStatus>
  ): void {
    Object.assign(this.requiredMock(sourceLanguage), patch);
    this.publish();
  }

  private publish(): void {
    const snapshot = this.snapshot();
    for (const listener of this.listeners) listener(snapshot);
  }
}

function mockTranslation(sourceLanguage: TranslationSourceLanguage, text: string): string {
  const known: Record<string, string> = {
    "今朝、新しい計画が発表されました。": "A new plan was announced this morning.",
    "今日は何をする予定ですか？": "What are you planning to do today?",
    "Las ventanas azules se abren sobre el jardín.": "Blue windows open above the garden."
  };
  return known[text.trim()] ?? `${sourceLanguage === "ja" ? "Japanese" : "Spanish"} → English: ${text.trim()}`;
}
