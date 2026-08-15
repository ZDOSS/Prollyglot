import type {
  CaptionPresentationFrame,
  CaptureSelection,
  CaptureStatus,
  ModelCatalogStatus,
  OverlaySettings,
  PixelRect,
  RuntimeBootstrap,
  RuntimeSnapshot,
  SourceSnapshot,
  TranscriptSnapshot,
  TranslationStorageCatalog,
  VisualCaptureCapabilities,
  VisualCaptureSelection,
  VisualDetectionMode,
  VisualModelCatalogStatus,
  VisualPresentationFrame,
  VisualSourceSnapshot,
  VisualStatus,
  VisualTextClear,
  VisualTextUpdate
} from "./types";

export type Unsubscribe = () => void;
export type WindowAction = "minimize" | "maximize" | "close";
export type WindowLayout = "full" | "compact";

/**
 * Complete host boundary used by desktop feature code.
 *
 * The native and browser-preview implementations both satisfy this contract;
 * feature modules never branch between Tauri commands and preview fixtures.
 */
export interface DesktopBridge {
  readonly kind: "native" | "preview";

  sourceSnapshot(): Promise<SourceSnapshot>;
  startCapture(selection: CaptureSelection, language: string): Promise<void>;
  stopCapture(): Promise<void>;
  captureStatus(): Promise<CaptureStatus>;
  onCaptureStatus(callback: (status: CaptureStatus) => void): Promise<Unsubscribe>;

  runtimeBootstrap(): Promise<RuntimeBootstrap>;
  onRuntimeState(callback: (snapshot: RuntimeSnapshot) => void): Promise<Unsubscribe>;

  modelStatus(): Promise<ModelCatalogStatus>;
  selectSpeechModel(modelId: string): Promise<void>;
  installSpeechModel(modelId: string): Promise<void>;
  removeSpeechModel(modelId: string): Promise<void>;
  onModelStatus(callback: (status: ModelCatalogStatus) => void): Promise<Unsubscribe>;

  transcriptSnapshot(): Promise<TranscriptSnapshot>;
  clearTranscript(): Promise<void>;
  onTranscriptUpdate(callback: (snapshot: TranscriptSnapshot) => void): Promise<Unsubscribe>;

  visualCapabilities(): Promise<VisualCaptureCapabilities>;
  visualSourceSnapshot(): Promise<VisualSourceSnapshot>;
  pickVisualRegion(displayId: string): Promise<PixelRect | undefined>;
  visualStatus(): Promise<VisualStatus>;
  onVisualStatus(callback: (status: VisualStatus) => void): Promise<Unsubscribe>;
  visualModelStatus(): Promise<VisualModelCatalogStatus>;
  installVisualModel(modelId: string): Promise<void>;
  removeVisualModel(modelId: string): Promise<void>;
  onVisualModelStatus(
    callback: (status: VisualModelCatalogStatus) => void
  ): Promise<Unsubscribe>;
  startVisualTranslation(
    selection: VisualCaptureSelection,
    sourceLanguage: string,
    targetLanguage: string,
    detectionMode: VisualDetectionMode
  ): Promise<void>;
  stopVisualTranslation(): Promise<void>;
  onVisualTextUpdate(callback: (update: VisualTextUpdate) => void): Promise<Unsubscribe>;
  onVisualTextClear(callback: (event: VisualTextClear) => void): Promise<Unsubscribe>;

  translationStorageStatus(): Promise<TranslationStorageCatalog>;
  installTranslationModel(storageId: string): Promise<void>;
  removeTranslationModel(storageId: string): Promise<void>;
  onTranslationStorageStatus(
    callback: (status: TranslationStorageCatalog) => void
  ): Promise<Unsubscribe>;
  translationModelBaseUrl(): string | undefined;

  updateCaptionPresentation(frame: CaptionPresentationFrame): Promise<void>;
  updateVisualPresentation(frame: VisualPresentationFrame): Promise<void>;
  updateOverlaySettings(settings: OverlaySettings): Promise<void>;

  showAppearance(): Promise<void>;
  closeAppearance(): Promise<void>;
  windowAction(action: WindowAction): Promise<void>;
  startWindowDrag(): Promise<void>;
  setWindowLayout(layout: WindowLayout): Promise<void>;

  reportFrontendDiagnostic(
    scope: string,
    message: string,
    level?: "error" | "info"
  ): Promise<void>;
}
