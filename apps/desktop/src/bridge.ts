import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { LogicalSize } from "@tauri-apps/api/dpi";
import { emit, listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { SPOKEN_LANGUAGES } from "./language-catalog";
import { RUNTIME_COMMANDS, RUNTIME_CONTRACT_VERSION, RUNTIME_EVENTS } from "./types";

import type {
  CaptureSelection,
  CaptionPresentationFrame,
  CaptureStatus,
  ModelCatalogStatus,
  OverlaySettings,
  PixelRect,
  RuntimeBootstrap,
  RuntimeSnapshot,
  RuntimeStateEvent,
  ShowVisualRegionSelectorCommand,
  SourceSnapshot,
  StartCaptureCommand,
  StartVisualTranslationCommand,
  TranscriptSnapshot,
  TranslationStorageCatalog,
  UpdateCaptionPresentationCommand,
  UpdateVisualPresentationCommand,
  VisualCaptureCapabilities,
  VisualCaptureSelection,
  VisualDetectionMode,
  VisualModelCatalogStatus,
  VisualPresentationFrame,
  VisualRegionSelected,
  VisualRegionSelectorRequest,
  VisualSourceSnapshot,
  VisualStatus,
  VisualTextClear,
  VisualTextUpdate
} from "./types";

declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
    __PROLLYGLOT_PREVIEW__?: {
      setTranscript: (snapshot: TranscriptSnapshot) => void;
      visualPresentation?: VisualPresentationFrame;
    };
  }
}
export const isTauri = () => window.__TAURI_INTERNALS__ !== undefined;

const mockSnapshot: SourceSnapshot = {
  playbackDevices: [
    { id: "default", name: "Speakers (Realtek(R) Audio)", isDefault: true },
    { id: "headphones", name: "Headphones (USB Audio)", isDefault: false }
  ],
  applications: [
    { id: "process:4028", name: "Discord", processId: 4028, deviceIds: ["default"] },
    { id: "process:7716", name: "Firefox", processId: 7716, deviceIds: ["default"] }
  ]
};

let mockStatus: CaptureStatus = { state: "stopped", peak: 0, droppedFrames: 0 };
let mockRuntimeRevision = 0;
let mockNextSessionId = 1;
let mockActiveSessionId: number | null = null;
const mockRuntimeListeners = new Set<(snapshot: RuntimeSnapshot) => void>();
const mockStatusListeners = new Set<(status: CaptureStatus) => void>();
let mockTimer: number | undefined;
let mockStartTimer: number | undefined;
let mockCaptionTimers: number[] = [];
let mockModelCatalog: ModelCatalogStatus = {
  selectedModelId: "sherpa-zipformer-en-20m-2023-02-17",
  models: [
    {
      phase: "notInstalled",
      modelId: "sherpa-zipformer-en-20m-2023-02-17",
      displayName: "English Streaming Small",
      profile: "Fast",
      description: "Lowest download and CPU cost for responsive captions on ordinary PCs.",
      languages: ["en"],
      downloadedBytes: 0,
      totalBytes: 45_202_074
    },
    {
      phase: "notInstalled",
      modelId: "sherpa-zipformer-en-standard-2023-06-26",
      displayName: "English Streaming Standard",
      profile: "Balanced",
      description: "A larger streaming model with more capacity while remaining comfortably real-time in local tests.",
      languages: ["en"],
      downloadedBytes: 0,
      totalBytes: 73_440_167
    },
    {
      phase: "notInstalled",
      modelId: "sherpa-zipformer-en-gigaspeech-2023-06-21",
      displayName: "English Streaming Enhanced",
      profile: "Enhanced",
      description: "The broadest English option, trained on LibriSpeech and GigaSpeech for a better chance on varied speech.",
      languages: ["en"],
      downloadedBytes: 0,
      totalBytes: 190_180_941
    },
    {
      phase: "notInstalled",
      modelId: "sherpa-zipformer-zh-14m-2023-02-23",
      displayName: "Chinese Streaming Small",
      profile: "Chinese · Small",
      description: "A low-footprint 14M streaming model for responsive Mandarin captions.",
      languages: ["zh"],
      downloadedBytes: 0,
      totalBytes: 30_975_688
    },
    {
      phase: "notInstalled",
      modelId: "sherpa-zipformer-fr-2023-04-14",
      displayName: "French Streaming Compact",
      profile: "French · Compact",
      description: "A dedicated streaming French model with much lower resource use than Nemotron.",
      languages: ["fr"],
      downloadedBytes: 0,
      totalBytes: 129_012_566
    },
    {
      phase: "notInstalled",
      modelId: "sherpa-zipformer-ko-2024-06-16",
      displayName: "Korean Streaming Compact",
      profile: "Korean · Compact",
      description: "A dedicated streaming Korean model with lower resource use than Nemotron.",
      languages: ["ko"],
      downloadedBytes: 0,
      totalBytes: 140_919_603
    },
    {
      phase: "notInstalled",
      modelId: "sherpa-zipformer-bn-vosk-2026-02-09",
      displayName: "Bengali Streaming Compact",
      profile: "Bengali · Compact",
      description: "A dedicated streaming Bengali model for local, lower-resource captions.",
      languages: ["bn"],
      downloadedBytes: 0,
      totalBytes: 94_119_939
    },
    {
      phase: "notInstalled",
      modelId: "nemotron-3.5-asr-streaming-0.6b-560ms-int8-2026-06-11",
      displayName: "Nemotron 3.5 Streaming 0.6B",
      profile: "Multilingual",
      description: "A high-resource 600M-parameter CPU model covering 28 languages plus automatic detection. Expect about 1 GB of app memory; broad-coverage languages and automatic detection may be less accurate.",
      languages: [
        "auto",
        ...SPOKEN_LANGUAGES.filter(({ code }) => code !== "bn").map(({ code }) => code)
      ],
      downloadedBytes: 0,
      totalBytes: 682_215_356
    }
  ]
};
const mockModelListeners = new Set<(status: ModelCatalogStatus) => void>();
const mockVisualCapabilities: VisualCaptureCapabilities = {
  windowsGraphicsCapture: true,
  systemPicker: false,
  desktopDuplicationExperiment: false
};
const mockVisualSources: VisualSourceSnapshot = {
  windows: [
    {
      id: "window:firefox",
      kind: "applicationWindow",
      label: "Japanese News — Firefox",
      x: 80,
      y: 60,
      width: 1280,
      height: 720
    },
    {
      id: "window:game",
      kind: "applicationWindow",
      label: "Sample game",
      x: 180,
      y: 100,
      width: 1440,
      height: 900
    }
  ],
  displays: [
    {
      id: "display:primary",
      kind: "display",
      label: "Display 1 · Primary",
      x: 0,
      y: 0,
      width: 1920,
      height: 1080
    }
  ]
};
let mockVisualModelCatalog: VisualModelCatalogStatus = {
  models: [
    {
      phase: "notInstalled",
      modelId: "ppocrv6-small-multilingual",
      displayName: "PP-OCRv6 Small · Multilingual",
      profile: "Visual OCR · Balanced",
      description: "Detects and recognizes text already visible in applications, video, games, and display regions.",
      languages: SPOKEN_LANGUAGES.map(({ code }) => code),
      downloadedBytes: 0,
      totalBytes: 31_824_456
    }
  ]
};
const mockVisualModelListeners = new Set<(status: VisualModelCatalogStatus) => void>();
let mockVisualStatus: VisualStatus = {
  active: false,
  state: "stopped",
  framesReceived: 0,
  framesAnalyzed: 0,
  framesUnchanged: 0,
  replacedFrames: 0,
  visibleRegions: 0,
  overlayRegions: 0
};
const mockVisualStatusListeners = new Set<(status: VisualStatus) => void>();
const mockVisualTextListeners = new Set<(update: VisualTextUpdate) => void>();
const mockVisualClearListeners = new Set<(event: VisualTextClear) => void>();
let mockVisualTimers: number[] = [];
let mockTranscript: TranscriptSnapshot = { revision: 0, committed: [] };
const mockTranscriptListeners = new Set<(snapshot: TranscriptSnapshot) => void>();

const publishMockStatus = () => {
  for (const listener of mockStatusListeners) listener(mockStatus);
};

const mockRuntimeSnapshot = (): RuntimeSnapshot => {
  const visualOwnsRuntime = mockVisualStatus.state !== "stopped";
  const audioOwnsRuntime = !visualOwnsRuntime && mockStatus.state !== "stopped";
  const mode = visualOwnsRuntime
    ? "visualTranslation"
    : audioOwnsRuntime
      ? "audioCaptions"
      : null;
  const state = visualOwnsRuntime ? mockVisualStatus.state : mockStatus.state;
  const lifecycle = state === "capturing" ? "running" : state;
  const message = visualOwnsRuntime ? mockVisualStatus.message : mockStatus.message;
  return {
    contractVersion: RUNTIME_CONTRACT_VERSION,
    revision: mockRuntimeRevision,
    sessionId: mode ? mockActiveSessionId : null,
    mode,
    source: mode === "visualTranslation"
      ? { id: "preview-visual", kind: "applicationWindow", label: mockVisualStatus.sourceLabel ?? "Preview window" }
      : mode === "audioCaptions"
        ? { id: "default-output", kind: "systemOutput", label: "System default" }
        : null,
    lifecycle,
    health: {
      level: state === "waiting" ? "recovering" : state === "failed" ? "degraded" : "healthy",
      progress: state === "capturing"
        ? "live"
        : state === "waiting"
          ? "waitingForSource"
          : state === "starting"
            ? "preparingModel"
            : state === "stopped"
              ? "idle"
              : state,
      message: message ?? null
    },
    failure: null
  };
};

const publishMockRuntime = () => {
  mockRuntimeRevision += 1;
  const snapshot = mockRuntimeSnapshot();
  for (const listener of mockRuntimeListeners) listener(structuredClone(snapshot));
};

const publishMockModel = () => {
  for (const listener of mockModelListeners) listener(structuredClone(mockModelCatalog));
};

const publishMockVisualModel = () => {
  for (const listener of mockVisualModelListeners) {
    listener(structuredClone(mockVisualModelCatalog));
  }
};

const publishMockVisualStatus = () => {
  for (const listener of mockVisualStatusListeners) listener(structuredClone(mockVisualStatus));
};

const publishMockVisualText = (update: VisualTextUpdate) => {
  for (const listener of mockVisualTextListeners) listener(structuredClone(update));
};

const mockModel = (modelId: string) => {
  const model = mockModelCatalog.models.find(({ modelId: candidate }) => candidate === modelId);
  if (!model) throw new Error("The selected speech model is unavailable.");
  return model;
};

const publishMockTranscript = () => {
  for (const listener of mockTranscriptListeners) listener(structuredClone(mockTranscript));
};

/** Supplies deterministic transcript state to the browser-only development preview. */
export function setMockTranscriptForPreview(snapshot: TranscriptSnapshot): void {
  if (isTauri()) return;
  mockTranscript = structuredClone(snapshot);
  publishMockTranscript();
}

if (!isTauri() && import.meta.env.DEV) {
  window.__PROLLYGLOT_PREVIEW__ = { setTranscript: setMockTranscriptForPreview };
}

export async function sourceSnapshot(): Promise<SourceSnapshot> {
  if (!isTauri()) return structuredClone(mockSnapshot);
  return invoke<SourceSnapshot>(RUNTIME_COMMANDS.sourceSnapshot);
}

export async function startCapture(selection: CaptureSelection, language: string): Promise<void> {
  if (isTauri()) {
    await invoke(
      RUNTIME_COMMANDS.startCapture,
      { selection, language } satisfies StartCaptureCommand
    );
    return;
  }

  mockActiveSessionId = mockNextSessionId++;
  mockStatus = { state: "starting", peak: 0, droppedFrames: 0 };
  mockTranscript = { revision: mockTranscript.revision + 1, committed: [] };
  publishMockTranscript();
  publishMockStatus();
  publishMockRuntime();
  mockStartTimer = window.setTimeout(() => {
    mockStartTimer = undefined;
    mockStatus = { state: "capturing", peak: 0.18, droppedFrames: 0 };
    publishMockStatus();
    publishMockRuntime();
    mockTimer = window.setInterval(() => {
      mockStatus = { ...mockStatus, peak: 0.08 + Math.random() * 0.72 };
      publishMockStatus();
    }, 180);
    mockCaptionTimers = [
      window.setTimeout(() => {
        mockTranscript = {
          revision: mockTranscript.revision + 1,
          provisional: {
            utteranceId: 0,
            startMicros: 0,
            endMicros: 900_000,
            sourceLanguage: language,
            text: "We should be there",
            isFinal: false
          },
          committed: []
        };
        publishMockTranscript();
      }, 900),
      window.setTimeout(() => {
        mockTranscript = {
          revision: mockTranscript.revision + 1,
          committed: [
            {
              utteranceId: 0,
              startMicros: 0,
              endMicros: 1_800_000,
              sourceLanguage: language,
              text: "We should be there in about ten minutes.",
              isFinal: true
            }
          ]
        };
        publishMockTranscript();
      }, 1_800)
    ];
  }, 420);
}

export async function stopCapture(): Promise<void> {
  if (isTauri()) {
    await invoke(RUNTIME_COMMANDS.stopCapture);
    return;
  }

  if (mockTimer !== undefined) window.clearInterval(mockTimer);
  if (mockStartTimer !== undefined) window.clearTimeout(mockStartTimer);
  for (const timer of mockCaptionTimers) window.clearTimeout(timer);
  mockCaptionTimers = [];
  mockTimer = undefined;
  mockStartTimer = undefined;
  mockStatus = { ...mockStatus, state: "stopping", peak: 0 };
  publishMockStatus();
  publishMockRuntime();
  mockStatus = { state: "stopped", peak: 0, droppedFrames: 0 };
  mockActiveSessionId = null;
  publishMockStatus();
  publishMockRuntime();
}

export async function modelStatus(): Promise<ModelCatalogStatus> {
  if (!isTauri()) return structuredClone(mockModelCatalog);
  return invoke<ModelCatalogStatus>("model_status");
}

export async function selectSpeechModel(modelId: string): Promise<void> {
  if (isTauri()) {
    await invoke("select_speech_model", { modelId });
    return;
  }
  mockModel(modelId);
  mockModelCatalog.selectedModelId = modelId;
  publishMockModel();
}

export async function installSpeechModel(modelId: string): Promise<void> {
  if (isTauri()) {
    await invoke("install_speech_model", { modelId });
    return;
  }
  const model = mockModel(modelId);
  model.phase = "downloading";
  model.downloadedBytes = 0;
  model.message = "Downloading encoder…";
  publishMockModel();
  window.setTimeout(() => {
    const downloading = mockModel(modelId);
    downloading.phase = "downloading";
    downloading.downloadedBytes = Math.round(downloading.totalBytes * 0.62);
    downloading.message = "Downloading encoder…";
    publishMockModel();
  }, 450);
  window.setTimeout(() => {
    const downloaded = mockModel(modelId);
    downloaded.phase = "ready";
    downloaded.downloadedBytes = downloaded.totalBytes;
    downloaded.message = undefined;
    publishMockModel();
  }, 1_050);
}

export async function removeSpeechModel(modelId: string): Promise<void> {
  if (isTauri()) {
    await invoke("remove_speech_model", { modelId });
    return;
  }
  const model = mockModel(modelId);
  model.phase = "notInstalled";
  model.downloadedBytes = 0;
  model.message = undefined;
  publishMockModel();
}

export async function onModelStatus(
  callback: (status: ModelCatalogStatus) => void
): Promise<UnlistenFn> {
  if (isTauri()) return listen<ModelCatalogStatus>("model-status", ({ payload }) => callback(payload));
  mockModelListeners.add(callback);
  return () => mockModelListeners.delete(callback);
}

export async function visualCapabilities(): Promise<VisualCaptureCapabilities> {
  if (!isTauri()) return structuredClone(mockVisualCapabilities);
  return invoke<VisualCaptureCapabilities>(RUNTIME_COMMANDS.visualCapabilities);
}

export async function visualSourceSnapshot(): Promise<VisualSourceSnapshot> {
  if (!isTauri()) return structuredClone(mockVisualSources);
  return invoke<VisualSourceSnapshot>(RUNTIME_COMMANDS.visualSourceSnapshot);
}

export async function pickVisualRegion(displayId: string): Promise<PixelRect | undefined> {
  if (!isTauri()) {
    const display = mockVisualSources.displays.find(({ id }) => id === displayId);
    if (!display) throw new Error("The selected display is unavailable.");
    return {
      x: Math.round(display.width * 0.12),
      y: Math.round(display.height * 0.18),
      width: Math.round(display.width * 0.72),
      height: Math.round(display.height * 0.64)
    };
  }

  return new Promise<PixelRect | undefined>((resolve, reject) => {
    let settled = false;
    const unlisten: UnlistenFn[] = [];
    const finish = (region?: PixelRect, error?: unknown) => {
      if (settled) return;
      settled = true;
      for (const stopListening of unlisten) stopListening();
      if (error) reject(error);
      else resolve(region);
    };

    void Promise.all([
      listen<VisualRegionSelected>(RUNTIME_EVENTS.visualRegionSelected, ({ payload }) => {
        if (payload.displayId === displayId) finish(payload.region);
      }),
      listen(RUNTIME_EVENTS.visualRegionSelectionCancelled, () => finish())
    ]).then(async (listeners) => {
      unlisten.push(...listeners);
      const request = await invoke<VisualRegionSelectorRequest>(
        RUNTIME_COMMANDS.showVisualRegionSelector,
        { displayId } satisfies ShowVisualRegionSelectorCommand
      );
      await emit(RUNTIME_EVENTS.visualRegionSelectorRequest, request);
    }).catch((error) => finish(undefined, error));
  });
}

export async function visualModelStatus(): Promise<VisualModelCatalogStatus> {
  if (!isTauri()) return structuredClone(mockVisualModelCatalog);
  return invoke<VisualModelCatalogStatus>("visual_model_status");
}

export async function translationStorageStatus(): Promise<TranslationStorageCatalog> {
  if (!isTauri()) return { models: [] };
  return invoke<TranslationStorageCatalog>("translation_model_status");
}

export async function installTranslationModel(storageId: string): Promise<void> {
  if (!isTauri()) throw new Error("Native translation storage requires the desktop app.");
  await invoke("install_translation_model", { storageId });
}

export async function removeTranslationModel(storageId: string): Promise<void> {
  if (!isTauri()) return;
  await invoke("remove_translation_model", { storageId });
}

export async function onTranslationStorageStatus(
  callback: (status: TranslationStorageCatalog) => void
): Promise<UnlistenFn> {
  if (!isTauri()) return () => undefined;
  return listen<TranslationStorageCatalog>("translation-model-status", ({ payload }) => {
    callback(payload);
  });
}

export function translationModelBaseUrl(): string | undefined {
  if (!isTauri()) return undefined;
  return convertFileSrc("translation", "prollyglot-model").replace(/\/$/u, "");
}

export async function installVisualModel(modelId: string): Promise<void> {
  if (isTauri()) {
    await invoke("install_visual_model", { modelId });
    return;
  }
  const model = mockVisualModelCatalog.models.find((candidate) => candidate.modelId === modelId);
  if (!model) throw new Error("The selected visual recognition model is unavailable.");
  model.phase = "downloading";
  model.downloadedBytes = 0;
  model.message = "Downloading visual text detector…";
  publishMockVisualModel();
  window.setTimeout(() => {
    model.downloadedBytes = Math.round(model.totalBytes * 0.55);
    model.message = "Downloading multilingual text recognizer…";
    publishMockVisualModel();
  }, 320);
  window.setTimeout(() => {
    model.phase = "ready";
    model.downloadedBytes = model.totalBytes;
    model.message = undefined;
    publishMockVisualModel();
  }, 760);
}

export async function removeVisualModel(modelId: string): Promise<void> {
  if (isTauri()) {
    await invoke("remove_visual_model", { modelId });
    return;
  }
  const model = mockVisualModelCatalog.models.find((candidate) => candidate.modelId === modelId);
  if (!model) throw new Error("The selected visual recognition model is unavailable.");
  model.phase = "notInstalled";
  model.downloadedBytes = 0;
  model.message = undefined;
  publishMockVisualModel();
}

export async function onVisualModelStatus(
  callback: (status: VisualModelCatalogStatus) => void
): Promise<UnlistenFn> {
  if (isTauri()) {
    return listen<VisualModelCatalogStatus>("visual-model-status", ({ payload }) => callback(payload));
  }
  mockVisualModelListeners.add(callback);
  return () => mockVisualModelListeners.delete(callback);
}

export async function visualStatus(): Promise<VisualStatus> {
  if (!isTauri()) return structuredClone(mockVisualStatus);
  return invoke<VisualStatus>(RUNTIME_COMMANDS.visualStatus);
}

export async function startVisualTranslation(
  selection: VisualCaptureSelection,
  sourceLanguage: string,
  targetLanguage: string,
  detectionMode: VisualDetectionMode
): Promise<void> {
  if (isTauri()) {
    await invoke(
      RUNTIME_COMMANDS.startVisualTranslation,
      {
        selection,
        sourceLanguage,
        targetLanguage,
        detectionMode
      } satisfies StartVisualTranslationCommand
    );
    return;
  }
  void detectionMode;
  const model = mockVisualModelCatalog.models[0];
  if (!model || model.phase !== "ready") {
    throw new Error("Install PP-OCRv6 Small in Settings before starting visual translation.");
  }
  const selectedSource = selection.kind === "applicationWindow"
    ? mockVisualSources.windows.find(({ id }) => id === selection.sourceId)
    : mockVisualSources.displays.find(({ id }) =>
      id === (selection.kind === "display" ? selection.sourceId : selection.displayId));
  if (!selectedSource) throw new Error("The selected visual source is unavailable.");
  const sessionId = mockNextSessionId++;
  mockActiveSessionId = sessionId;
  const geometry = selection.kind === "region"
    ? {
        label: `${selectedSource.label} · Region`,
        x: selectedSource.x + selection.region.x,
        y: selectedSource.y + selection.region.y,
        width: selection.region.width,
        height: selection.region.height
      }
    : {
        label: selectedSource.label,
        x: selectedSource.x,
        y: selectedSource.y,
        width: selectedSource.width,
        height: selectedSource.height
      };
  mockVisualStatus = {
    active: true,
    state: "starting",
    sourceLabel: geometry.label,
    framesReceived: 0,
    framesAnalyzed: 0,
    framesUnchanged: 0,
    replacedFrames: 0,
    visibleRegions: 0,
    overlayRegions: 0,
    message: "Loading local visual text recognition…"
  };
  publishMockVisualStatus();
  publishMockRuntime();
  mockVisualTimers = [
    window.setTimeout(() => {
      mockVisualStatus = {
        ...mockVisualStatus,
        state: "capturing",
        message: `Watching the live source, recognizing ${sourceLanguage} text, and translating to ${targetLanguage}.`
      };
      publishMockVisualStatus();
      publishMockRuntime();
    }, 300),
    window.setTimeout(() => {
      const region = {
        trackId: 1,
        textRevision: 1,
        text: "今朝、新しい計画が発表されました。",
        confidence: 0.94,
        language: sourceLanguage,
        bounds: { x: 210, y: 520, width: 680, height: 58 }
      };
      mockVisualStatus = {
        ...mockVisualStatus,
        framesReceived: 4,
        framesAnalyzed: 2,
        framesUnchanged: 1,
        visibleRegions: 1
      };
      publishMockVisualStatus();
      publishMockVisualText({
        sessionId,
        runtimeRevision: mockRuntimeRevision,
        source: geometry,
        visible: [region],
        translationRequests: [region],
        removedTrackIds: []
      });
    }, 900)
  ];
}

export async function stopVisualTranslation(): Promise<void> {
  if (isTauri()) {
    await invoke(RUNTIME_COMMANDS.stopVisualTranslation);
    return;
  }
  for (const timer of mockVisualTimers) window.clearTimeout(timer);
  mockVisualTimers = [];
  mockVisualStatus = {
    ...mockVisualStatus,
    active: true,
    state: "stopping",
    message: "Cancelling recognition and stopping screen capture…"
  };
  publishMockVisualStatus();
  publishMockRuntime();
  const sessionId = mockActiveSessionId;
  if (sessionId !== null) {
    const clear = { sessionId, runtimeRevision: mockRuntimeRevision };
    for (const listener of mockVisualClearListeners) listener(clear);
  }
  mockVisualStatus = {
    active: false,
    state: "stopped",
    framesReceived: 0,
    framesAnalyzed: 0,
    framesUnchanged: 0,
    replacedFrames: 0,
    visibleRegions: 0,
    overlayRegions: 0
  };
  mockActiveSessionId = null;
  publishMockVisualStatus();
  publishMockRuntime();
}

export async function onVisualStatus(
  callback: (status: VisualStatus) => void
): Promise<UnlistenFn> {
  if (isTauri()) {
    return listen<VisualStatus>(RUNTIME_EVENTS.visualStatus, ({ payload }) => callback(payload));
  }
  mockVisualStatusListeners.add(callback);
  return () => mockVisualStatusListeners.delete(callback);
}

export async function onVisualTextUpdate(
  callback: (update: VisualTextUpdate) => void
): Promise<UnlistenFn> {
  if (isTauri()) {
    return listen<VisualTextUpdate>(RUNTIME_EVENTS.visualText, ({ payload }) => callback(payload));
  }
  mockVisualTextListeners.add(callback);
  return () => mockVisualTextListeners.delete(callback);
}

export async function onVisualTextClear(
  callback: (event: VisualTextClear) => void
): Promise<UnlistenFn> {
  if (isTauri()) {
    return listen<VisualTextClear>(RUNTIME_EVENTS.visualClear, ({ payload }) => callback(payload));
  }
  mockVisualClearListeners.add(callback);
  return () => mockVisualClearListeners.delete(callback);
}

export async function updateVisualPresentation(frame: VisualPresentationFrame): Promise<void> {
  if (window.__PROLLYGLOT_PREVIEW__) {
    window.__PROLLYGLOT_PREVIEW__.visualPresentation = structuredClone(frame);
  }
  if (isTauri()) {
    await invoke<boolean>(
      RUNTIME_COMMANDS.updateVisualPresentation,
      { frame } satisfies UpdateVisualPresentationCommand
    );
    return;
  }
  if (mockVisualStatus.active && mockVisualStatus.state === "capturing") {
    mockVisualStatus = {
      ...mockVisualStatus,
      overlayRegions: frame.regions.length
    };
    publishMockVisualStatus();
  }
}

export async function transcriptSnapshot(): Promise<TranscriptSnapshot> {
  if (!isTauri()) return structuredClone(mockTranscript);
  return invoke<TranscriptSnapshot>("transcript_snapshot");
}

export async function clearTranscript(): Promise<void> {
  if (isTauri()) {
    await invoke("clear_transcript");
    return;
  }
  mockTranscript = { revision: mockTranscript.revision + 1, committed: [] };
  publishMockTranscript();
}

export async function onTranscriptUpdate(
  callback: (snapshot: TranscriptSnapshot) => void
): Promise<UnlistenFn> {
  if (isTauri()) {
    return listen<TranscriptSnapshot>("transcript-update", ({ payload }) => callback(payload));
  }
  mockTranscriptListeners.add(callback);
  return () => mockTranscriptListeners.delete(callback);
}

export async function captureStatus(): Promise<CaptureStatus> {
  if (!isTauri()) return mockStatus;
  return invoke<CaptureStatus>(RUNTIME_COMMANDS.captureStatus);
}

export async function runtimeBootstrap(): Promise<RuntimeBootstrap> {
  if (!isTauri()) return { snapshot: structuredClone(mockRuntimeSnapshot()) };
  return invoke<RuntimeBootstrap>(RUNTIME_COMMANDS.bootstrap);
}

export async function onRuntimeState(
  callback: (snapshot: RuntimeSnapshot) => void
): Promise<UnlistenFn> {
  if (isTauri()) {
    return listen<RuntimeStateEvent>(RUNTIME_EVENTS.state, ({ payload }) => {
      callback(payload.snapshot);
    });
  }
  mockRuntimeListeners.add(callback);
  return () => mockRuntimeListeners.delete(callback);
}

export async function onCaptureStatus(
  callback: (status: CaptureStatus) => void
): Promise<UnlistenFn> {
  if (isTauri()) {
    return listen<CaptureStatus>(RUNTIME_EVENTS.captureStatus, ({ payload }) => callback(payload));
  }
  mockStatusListeners.add(callback);
  return () => mockStatusListeners.delete(callback);
}

export async function showAppearance(): Promise<void> {
  if (isTauri()) {
    await invoke("show_appearance_window");
  } else {
    window.location.href = "/appearance.html";
  }
}

export async function closeAppearance(): Promise<void> {
  if (isTauri()) {
    await invoke("close_appearance_window");
  } else {
    window.location.href = "/";
  }
}

export async function updateOverlaySettings(settings: OverlaySettings): Promise<void> {
  localStorage.setItem("prollyglot.overlay", JSON.stringify(settings));
  if (isTauri()) await invoke("update_overlay_settings", { settings });
}

export async function updateCaptionPresentation(frame: CaptionPresentationFrame): Promise<void> {
  if (isTauri()) {
    await invoke<boolean>(
      RUNTIME_COMMANDS.updateCaptionPresentation,
      { frame } satisfies UpdateCaptionPresentationCommand
    );
  }
}

export async function reportFrontendDiagnostic(
  scope: string,
  message: string,
  level: "error" | "info" = "error"
): Promise<void> {
  if (level === "info") console.info(`[${scope}] ${message}`);
  else console.error(`[${scope}] ${message}`);
  if (!isTauri()) return;
  try {
    await invoke("report_frontend_diagnostic", { scope, message });
  } catch (error) {
    console.error("Could not write the frontend diagnostic to the Prollyglot log.", error);
  }
}

export async function windowAction(action: "minimize" | "maximize" | "close"): Promise<void> {
  if (!isTauri()) return;
  const current = getCurrentWindow();
  if (action === "minimize") await current.minimize();
  if (action === "maximize") {
    if (await current.isMaximized()) await current.unmaximize();
    else await current.maximize();
  }
  if (action === "close") await current.close();
}

export async function startWindowDrag(): Promise<void> {
  if (!isTauri()) return;
  await getCurrentWindow().startDragging();
}

export async function setWindowLayout(layout: "full" | "compact"): Promise<void> {
  if (!isTauri()) return;
  const current = getCurrentWindow();
  if (await current.isMaximized()) await current.unmaximize();
  const size = layout === "full"
    ? new LogicalSize(1180, 760)
    : new LogicalSize(440, 640);
  await current.setSize(size);
}
