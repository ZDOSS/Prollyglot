import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";

import type {
  CaptureSelection,
  CaptureStatus,
  ModelStatus,
  OverlaySettings,
  SourceSnapshot,
  TranscriptSnapshot
} from "./types";

declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
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
let mockModelStatus: ModelStatus = {
  phase: "notInstalled",
  modelId: "sherpa-zipformer-en-20m-2023-02-17",
  displayName: "English Streaming Small",
  downloadedBytes: 0,
  totalBytes: 45_202_074
};
const mockModelListeners = new Set<(status: ModelStatus) => void>();
let mockTranscript: TranscriptSnapshot = { revision: 0, committed: [] };
const mockTranscriptListeners = new Set<(snapshot: TranscriptSnapshot) => void>();

const publishMockStatus = () => {
  for (const listener of mockStatusListeners) listener(mockStatus);
};

const publishMockModel = () => {
  for (const listener of mockModelListeners) listener(mockModelStatus);
};

const publishMockTranscript = () => {
  for (const listener of mockTranscriptListeners) listener(structuredClone(mockTranscript));
};

export async function sourceSnapshot(): Promise<SourceSnapshot> {
  if (!isTauri()) return structuredClone(mockSnapshot);
  return invoke<SourceSnapshot>("source_snapshot");
}

export async function startCapture(selection: CaptureSelection): Promise<void> {
  if (isTauri()) {
    await invoke("start_capture", { selection });
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
            sourceLanguage: "en",
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
              sourceLanguage: "en",
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

export async function modelStatus(): Promise<ModelStatus> {
  if (!isTauri()) return structuredClone(mockModelStatus);
  return invoke<ModelStatus>("model_status");
}

export async function installEnglishModel(): Promise<void> {
  if (isTauri()) {
    await invoke("install_english_model");
    return;
  }
  mockModelStatus = {
    ...mockModelStatus,
    phase: "downloading",
    downloadedBytes: 0,
    message: "Downloading encoder…"
  };
  publishMockModel();
  window.setTimeout(() => {
    mockModelStatus = {
      ...mockModelStatus,
      phase: "downloading",
      downloadedBytes: Math.round(mockModelStatus.totalBytes * 0.62),
      message: "Downloading encoder…"
    };
    publishMockModel();
  }, 450);
  window.setTimeout(() => {
    mockModelStatus = {
      ...mockModelStatus,
      phase: "ready",
      downloadedBytes: mockModelStatus.totalBytes,
      message: undefined
    };
    publishMockModel();
  }, 1_050);
}

export async function removeEnglishModel(): Promise<void> {
  if (isTauri()) {
    await invoke("remove_english_model");
    return;
  }
  mockModelStatus = { ...mockModelStatus, phase: "notInstalled", downloadedBytes: 0 };
  publishMockModel();
}

export async function onModelStatus(
  callback: (status: ModelStatus) => void
): Promise<UnlistenFn> {
  if (isTauri()) return listen<ModelStatus>("model-status", ({ payload }) => callback(payload));
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
    await invoke("show_overlay_preview", {
      caption: "We should be there in about ten minutes."
    });
  } else {
    window.location.href = "/appearance.html";
  }
}

export async function closeAppearance(): Promise<void> {
  if (isTauri()) {
    await getCurrentWindow().hide();
  } else {
    window.location.href = "/";
  }
}

export async function updateOverlaySettings(settings: OverlaySettings): Promise<void> {
  localStorage.setItem("prollyglot.overlay", JSON.stringify(settings));
  if (isTauri()) await invoke("update_overlay_settings", { settings });
}

export async function showOverlayPreview(caption: string): Promise<void> {
  if (isTauri()) await invoke("show_overlay_preview", { caption });
}

export async function hideOverlayPreview(): Promise<void> {
  if (isTauri()) await invoke("hide_overlay_preview");
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
