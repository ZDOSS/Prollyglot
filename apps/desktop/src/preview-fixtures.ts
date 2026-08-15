import {
  DEFAULT_APPLICATION_CONFIGURATION,
  RUNTIME_CONTRACT_VERSION
} from "./generated/runtime.ts";
import { SPOKEN_LANGUAGES } from "./language-catalog.ts";
import type {
  ConfigurationSnapshot,
  CaptureStatus,
  ModelCatalogStatus,
  RuntimeSnapshot,
  SourceSnapshot,
  TranscriptSnapshot,
  VisualCaptureCapabilities,
  VisualModelCatalogStatus,
  VisualSourceSnapshot,
  VisualStatus
} from "./types";

export function previewConfigurationSnapshot(): ConfigurationSnapshot {
  return {
    revision: 1,
    config: {
      ...structuredClone(DEFAULT_APPLICATION_CONFIGURATION),
      legacyWebviewImported: false,
      models: { speechModelId: "preview-speech-english" }
    }
  };
}

/** Preview data is intentionally small and fictional; it is not a copied model inventory. */
export function previewSourceSnapshot(): SourceSnapshot {
  return {
    playbackDevices: [
      { id: "preview-speakers", name: "Speakers (preview)", isDefault: true },
      { id: "preview-headphones", name: "Headphones (preview)", isDefault: false }
    ],
    applications: [
      {
        id: "preview-browser",
        name: "Browser preview",
        instanceCount: 1,
        deviceIds: ["preview-speakers"]
      }
    ]
  };
}

export function previewSpeechCatalog(): ModelCatalogStatus {
  return {
    selectedModelId: "preview-speech-english",
    models: [
      {
        phase: "notInstalled",
        modelId: "preview-speech-english",
        displayName: "Preview English Streaming",
        profile: "Preview · Fast",
        description: "A deterministic browser-preview fixture for English caption controls.",
        languages: ["en"],
        downloadedBytes: 0,
        totalBytes: 48 * 1024 * 1024
      },
      {
        phase: "notInstalled",
        modelId: "preview-speech-multilingual",
        displayName: "Preview Multilingual Streaming",
        profile: "Preview · Multilingual",
        description: "A deterministic fixture that exercises language and model navigation.",
        languages: ["auto", ...SPOKEN_LANGUAGES.map(({ code }) => code)],
        downloadedBytes: 0,
        totalBytes: 256 * 1024 * 1024
      }
    ]
  };
}

export function previewVisualCapabilities(): VisualCaptureCapabilities {
  return {
    windowsGraphicsCapture: true,
    systemPicker: false,
    desktopDuplicationExperiment: false
  };
}

export function previewVisualSources(): VisualSourceSnapshot {
  return {
    windows: [
      {
        id: "preview-window-browser",
        kind: "applicationWindow",
        label: "Japanese news — preview browser",
        x: 80,
        y: 60,
        width: 1_280,
        height: 720
      }
    ],
    displays: [
      {
        id: "preview-display-primary",
        kind: "display",
        label: "Display 1 · Preview",
        x: 0,
        y: 0,
        width: 1_920,
        height: 1_080
      }
    ]
  };
}

export function previewVisualModelCatalog(): VisualModelCatalogStatus {
  return {
    models: [{
      phase: "notInstalled",
      modelId: "preview-visual-ocr",
      displayName: "Preview Multilingual OCR",
      profile: "Preview · Visual OCR",
      description: "A deterministic browser-preview fixture for screen translation.",
      languages: SPOKEN_LANGUAGES.map(({ code }) => code),
      downloadedBytes: 0,
      totalBytes: 32 * 1024 * 1024
    }]
  };
}

export function previewCaptureStatus(
  patch: Partial<CaptureStatus> = {}
): CaptureStatus {
  return { state: "stopped", peak: 0, droppedFrames: 0, ...patch };
}

export function previewVisualStatus(
  patch: Partial<VisualStatus> = {}
): VisualStatus {
  return {
    active: false,
    state: "stopped",
    framesReceived: 0,
    framesAnalyzed: 0,
    framesUnchanged: 0,
    replacedFrames: 0,
    visibleRegions: 0,
    overlayRegions: 0,
    ...patch
  };
}

export function previewTranscript(
  patch: Partial<TranscriptSnapshot> = {}
): TranscriptSnapshot {
  return { revision: 0, committed: [], ...patch };
}

export interface PreviewRuntimeFixture {
  revision: number;
  activeSessionId: number | null;
  audio: CaptureStatus;
  visual: VisualStatus;
}

export function previewRuntimeSnapshot(fixture: PreviewRuntimeFixture): RuntimeSnapshot {
  const visualOwnsRuntime = fixture.visual.state !== "stopped";
  const audioOwnsRuntime = !visualOwnsRuntime && fixture.audio.state !== "stopped";
  const mode = visualOwnsRuntime
    ? "visualTranslation"
    : audioOwnsRuntime ? "audioCaptions" : null;
  const state = visualOwnsRuntime ? fixture.visual.state : fixture.audio.state;
  const lifecycle = state === "capturing" ? "running" : state;
  const message = visualOwnsRuntime ? fixture.visual.message : fixture.audio.message;
  return {
    contractVersion: RUNTIME_CONTRACT_VERSION,
    revision: fixture.revision,
    sessionId: mode ? fixture.activeSessionId : null,
    mode,
    source: mode === "visualTranslation"
      ? {
          id: "preview-visual",
          kind: "applicationWindow",
          label: fixture.visual.sourceLabel ?? "Preview window"
        }
      : mode === "audioCaptions"
        ? { id: "preview-output", kind: "systemOutput", label: "Preview speakers" }
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
}
