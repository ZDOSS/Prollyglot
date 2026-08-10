import { invoke } from "@tauri-apps/api/core";
import { emit, listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";

import type {
  CaptureSelection,
  CaptionOutputPayload,
  CaptureStatus,
  ModelCatalogStatus,
  OverlaySettings,
  SourceSnapshot,
  TranscriptSnapshot
} from "./types";

declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
    __PROLLYGLOT_PREVIEW__?: {
      setTranscript: (snapshot: TranscriptSnapshot) => void;
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
      modelId: "nemotron-3.5-asr-streaming-0.6b-560ms-int8-2026-06-11",
      displayName: "Nemotron 3.5 Streaming 0.6B",
      profile: "Multilingual",
      description: "A high-resource 600M-parameter CPU trial for English, Spanish, Japanese, or automatic detection. Expect about 1 GB of app memory; Japanese and automatic detection are experimental.",
      languages: ["auto", "en", "es", "ja"],
      downloadedBytes: 0,
      totalBytes: 682_215_356
    }
  ]
};
const mockModelListeners = new Set<(status: ModelCatalogStatus) => void>();
let mockTranscript: TranscriptSnapshot = { revision: 0, committed: [] };
const mockTranscriptListeners = new Set<(snapshot: TranscriptSnapshot) => void>();

const publishMockStatus = () => {
  for (const listener of mockStatusListeners) listener(mockStatus);
};

const publishMockModel = () => {
  for (const listener of mockModelListeners) listener(structuredClone(mockModelCatalog));
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
  return invoke<SourceSnapshot>("source_snapshot");
}

export async function startCapture(selection: CaptureSelection, language: string): Promise<void> {
  if (isTauri()) {
    await invoke("start_capture", { selection, language });
    return;
  }

  mockStatus = { state: "starting", peak: 0, droppedFrames: 0 };
  mockTranscript = { revision: mockTranscript.revision + 1, committed: [] };
  publishMockTranscript();
  publishMockStatus();
  mockStartTimer = window.setTimeout(() => {
    mockStartTimer = undefined;
    mockStatus = { state: "capturing", peak: 0.18, droppedFrames: 0 };
    publishMockStatus();
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
    await invoke("stop_capture");
    return;
  }

  if (mockTimer !== undefined) window.clearInterval(mockTimer);
  if (mockStartTimer !== undefined) window.clearTimeout(mockStartTimer);
  for (const timer of mockCaptionTimers) window.clearTimeout(timer);
  mockCaptionTimers = [];
  mockTimer = undefined;
  mockStartTimer = undefined;
  mockStatus = { state: "stopped", peak: 0, droppedFrames: 0 };
  publishMockStatus();
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
  return invoke<CaptureStatus>("capture_status");
}

export async function onCaptureStatus(
  callback: (status: CaptureStatus) => void
): Promise<UnlistenFn> {
  if (isTauri()) return listen<CaptureStatus>("capture-status", ({ payload }) => callback(payload));
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

export async function updateCaptionOutput(output: CaptionOutputPayload): Promise<void> {
  if (isTauri()) await emit("caption-output", output);
}

export async function reportFrontendDiagnostic(scope: string, message: string): Promise<void> {
  console.error(`[${scope}] ${message}`);
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
