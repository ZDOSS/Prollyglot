import type { DesktopBridge } from "./desktop-bridge";
import {
  previewCaptureStatus,
  previewConfigurationSnapshot,
  previewRuntimeSnapshot,
  previewSourceSnapshot,
  previewSpeechCatalog,
  previewTranscript,
  previewVisualCapabilities,
  previewVisualModelCatalog,
  previewVisualSources,
  previewVisualStatus
} from "./preview-fixtures";
import type {
  ConfigurationSnapshot,
  CaptionPresentationFrame,
  CaptureStatus,
  ModelCatalogStatus,
  RuntimeSnapshot,
  TranscriptSnapshot,
  VisualModelCatalogStatus,
  VisualPresentationFrame,
  VisualStatus,
  VisualTextClear,
  VisualTextUpdate
} from "./types";

export interface PreviewDesktopBridge extends DesktopBridge {
  setTranscriptForPreview(snapshot: TranscriptSnapshot): void;
  latestCaptionPresentation(): CaptionPresentationFrame | undefined;
  latestVisualPresentation(): VisualPresentationFrame | undefined;
}

export function createPreviewBridge(): PreviewDesktopBridge {
  const sources = previewSourceSnapshot();
  const visualSources = previewVisualSources();
  const capabilities = previewVisualCapabilities();
  let audioStatus = previewCaptureStatus();
  let configuration = previewConfigurationSnapshot();
  try {
    const stored = localStorage.getItem("prollyglot.preview-configuration");
    if (stored) {
      const parsed = JSON.parse(stored) as ConfigurationSnapshot;
      if (
        Number.isSafeInteger(parsed.revision)
        && parsed.revision > 0
        && parsed.config?.schemaVersion === configuration.config.schemaVersion
      ) configuration = parsed;
    }
  } catch {
    localStorage.removeItem("prollyglot.preview-configuration");
  }
  let visualStatus = previewVisualStatus();
  let speechCatalog = previewSpeechCatalog();
  const configuredSpeechModel = configuration.config.models.speechModelId;
  if (
    configuredSpeechModel
    && speechCatalog.models.some(({ modelId }) => modelId === configuredSpeechModel)
  ) speechCatalog.selectedModelId = configuredSpeechModel;
  let visualCatalog = previewVisualModelCatalog();
  let transcript = previewTranscript();
  let runtimeRevision = 0;
  let nextSessionId = 1;
  let activeSessionId: number | null = null;
  let captionPresentation: CaptionPresentationFrame | undefined;
  let visualPresentation: VisualPresentationFrame | undefined;
  let audioLevelTimer: number | undefined;
  let startTimer: number | undefined;
  let captionTimers: number[] = [];
  let visualTimers: number[] = [];

  const runtimeListeners = new Set<(snapshot: RuntimeSnapshot) => void>();
  const configurationListeners = new Set<(snapshot: ConfigurationSnapshot) => void>();
  const captureListeners = new Set<(status: CaptureStatus) => void>();
  const modelListeners = new Set<(status: ModelCatalogStatus) => void>();
  const transcriptListeners = new Set<(snapshot: TranscriptSnapshot) => void>();
  const visualStatusListeners = new Set<(status: VisualStatus) => void>();
  const visualModelListeners = new Set<(status: VisualModelCatalogStatus) => void>();
  const visualTextListeners = new Set<(update: VisualTextUpdate) => void>();
  const visualClearListeners = new Set<(event: VisualTextClear) => void>();

  const runtimeSnapshot = () => previewRuntimeSnapshot({
    revision: runtimeRevision,
    activeSessionId,
    audio: audioStatus,
    visual: visualStatus
  });
  const publishRuntime = () => {
    runtimeRevision += 1;
    const snapshot = runtimeSnapshot();
    for (const listener of runtimeListeners) listener(structuredClone(snapshot));
  };
  const publishCapture = () => {
    for (const listener of captureListeners) listener(structuredClone(audioStatus));
  };
  const publishModels = () => {
    for (const listener of modelListeners) listener(structuredClone(speechCatalog));
  };
  const publishTranscript = () => {
    for (const listener of transcriptListeners) listener(structuredClone(transcript));
  };
  const publishVisualStatus = () => {
    for (const listener of visualStatusListeners) listener(structuredClone(visualStatus));
  };
  const publishVisualModels = () => {
    for (const listener of visualModelListeners) listener(structuredClone(visualCatalog));
  };
  const publishVisualText = (update: VisualTextUpdate) => {
    for (const listener of visualTextListeners) listener(structuredClone(update));
  };
  const requiredSpeechModel = (modelId: string) => {
    const model = speechCatalog.models.find(({ modelId: candidate }) => candidate === modelId);
    if (!model) throw new Error("The selected preview speech model is unavailable.");
    return model;
  };
  const requiredVisualModel = (modelId: string) => {
    const model = visualCatalog.models.find(({ modelId: candidate }) => candidate === modelId);
    if (!model) throw new Error("The selected preview visual model is unavailable.");
    return model;
  };
  const clearAudioTimers = () => {
    if (audioLevelTimer !== undefined) window.clearInterval(audioLevelTimer);
    if (startTimer !== undefined) window.clearTimeout(startTimer);
    for (const timer of captionTimers) window.clearTimeout(timer);
    audioLevelTimer = undefined;
    startTimer = undefined;
    captionTimers = [];
  };

  const bridge: PreviewDesktopBridge = {
    kind: "preview",

    configurationSnapshot: async () => structuredClone(configuration),
    updateConfiguration: async (expectedRevision, config) => {
      if (expectedRevision !== configuration.revision) {
        throw new Error(
          `Configuration revision ${expectedRevision} is stale; current revision is ${configuration.revision}.`
        );
      }
      configuration = {
        revision: configuration.revision + 1,
        config: structuredClone(config)
      };
      localStorage.setItem("prollyglot.preview-configuration", JSON.stringify(configuration));
      for (const listener of configurationListeners) {
        listener(structuredClone(configuration));
      }
      return structuredClone(configuration);
    },
    onConfiguration: async (callback) => {
      configurationListeners.add(callback);
      return () => configurationListeners.delete(callback);
    },

    sourceSnapshot: async () => structuredClone(sources),
    startCapture: async (_selection, language) => {
      clearAudioTimers();
      activeSessionId = nextSessionId++;
      audioStatus = previewCaptureStatus({ state: "starting" });
      transcript = previewTranscript({ revision: transcript.revision + 1 });
      publishTranscript();
      publishCapture();
      publishRuntime();
      startTimer = window.setTimeout(() => {
        startTimer = undefined;
        audioStatus = previewCaptureStatus({ state: "capturing", peak: 0.18 });
        publishCapture();
        publishRuntime();
        audioLevelTimer = window.setInterval(() => {
          audioStatus = { ...audioStatus, peak: 0.08 + Math.random() * 0.72 };
          publishCapture();
        }, 180);
        captionTimers = [
          window.setTimeout(() => {
            transcript = previewTranscript({
              revision: transcript.revision + 1,
              provisional: {
                utteranceId: 0,
                startMicros: 0,
                endMicros: 900_000,
                sourceLanguage: language,
                text: "We should be there",
                isFinal: false
              }
            });
            publishTranscript();
          }, 900),
          window.setTimeout(() => {
            transcript = previewTranscript({
              revision: transcript.revision + 1,
              committed: [{
                utteranceId: 0,
                startMicros: 0,
                endMicros: 1_800_000,
                sourceLanguage: language,
                text: "We should be there in about ten minutes.",
                isFinal: true
              }]
            });
            publishTranscript();
          }, 1_800)
        ];
      }, 420);
    },
    stopCapture: async () => {
      clearAudioTimers();
      audioStatus = { ...audioStatus, state: "stopping", peak: 0 };
      publishCapture();
      publishRuntime();
      audioStatus = previewCaptureStatus();
      activeSessionId = null;
      publishCapture();
      publishRuntime();
    },
    captureStatus: async () => structuredClone(audioStatus),
    onCaptureStatus: async (callback) => {
      captureListeners.add(callback);
      return () => captureListeners.delete(callback);
    },

    runtimeBootstrap: async () => ({ snapshot: structuredClone(runtimeSnapshot()) }),
    onRuntimeState: async (callback) => {
      runtimeListeners.add(callback);
      return () => runtimeListeners.delete(callback);
    },

    inferenceResourceStatus: async () => ({
      revision: 0,
      processResidentBytes: null,
      resources: []
    }),
    reportInferenceResource: async () => ({
      revision: 0,
      processResidentBytes: null,
      resources: []
    }),

    modelStatus: async () => structuredClone(speechCatalog),
    selectSpeechModel: async (modelId) => {
      requiredSpeechModel(modelId);
      speechCatalog = { ...speechCatalog, selectedModelId: modelId };
      configuration = {
        revision: configuration.revision + 1,
        config: {
          ...configuration.config,
          models: { ...configuration.config.models, speechModelId: modelId }
        }
      };
      localStorage.setItem("prollyglot.preview-configuration", JSON.stringify(configuration));
      for (const listener of configurationListeners) {
        listener(structuredClone(configuration));
      }
      publishModels();
    },
    installSpeechModel: async (modelId) => {
      const model = requiredSpeechModel(modelId);
      model.phase = "downloading";
      model.downloadedBytes = 0;
      model.message = "Downloading preview fixture…";
      publishModels();
      window.setTimeout(() => {
        const current = requiredSpeechModel(modelId);
        current.phase = "ready";
        current.downloadedBytes = current.totalBytes;
        current.message = undefined;
        publishModels();
      }, 650);
    },
    removeSpeechModel: async (modelId) => {
      const model = requiredSpeechModel(modelId);
      model.phase = "notInstalled";
      model.downloadedBytes = 0;
      model.message = undefined;
      publishModels();
    },
    onModelStatus: async (callback) => {
      modelListeners.add(callback);
      return () => modelListeners.delete(callback);
    },

    transcriptSnapshot: async () => structuredClone(transcript),
    clearTranscript: async () => {
      transcript = previewTranscript({ revision: transcript.revision + 1 });
      publishTranscript();
    },
    onTranscriptUpdate: async (callback) => {
      transcriptListeners.add(callback);
      return () => transcriptListeners.delete(callback);
    },

    visualCapabilities: async () => structuredClone(capabilities),
    visualSourceSnapshot: async () => structuredClone(visualSources),
    pickVisualRegion: async (displayId) => {
      const display = visualSources.displays.find(({ id }) => id === displayId);
      if (!display) throw new Error("The selected preview display is unavailable.");
      return {
        x: Math.round(display.width * 0.12),
        y: Math.round(display.height * 0.18),
        width: Math.round(display.width * 0.72),
        height: Math.round(display.height * 0.64)
      };
    },
    visualStatus: async () => structuredClone(visualStatus),
    onVisualStatus: async (callback) => {
      visualStatusListeners.add(callback);
      return () => visualStatusListeners.delete(callback);
    },
    visualModelStatus: async () => structuredClone(visualCatalog),
    installVisualModel: async (modelId) => {
      const model = requiredVisualModel(modelId);
      model.phase = "downloading";
      model.downloadedBytes = 0;
      model.message = "Downloading preview OCR fixture…";
      publishVisualModels();
      window.setTimeout(() => {
        const current = requiredVisualModel(modelId);
        current.phase = "ready";
        current.downloadedBytes = current.totalBytes;
        current.message = undefined;
        publishVisualModels();
      }, 650);
    },
    removeVisualModel: async (modelId) => {
      const model = requiredVisualModel(modelId);
      model.phase = "notInstalled";
      model.downloadedBytes = 0;
      model.message = undefined;
      publishVisualModels();
    },
    onVisualModelStatus: async (callback) => {
      visualModelListeners.add(callback);
      return () => visualModelListeners.delete(callback);
    },
    startVisualTranslation: async (
      selection,
      sourceLanguage,
      targetLanguage,
      _detectionMode
    ) => {
      const model = visualCatalog.models[0];
      if (!model || model.phase !== "ready") {
        throw new Error("Install the preview OCR model before starting screen translation.");
      }
      const source = selection.kind === "applicationWindow"
        ? visualSources.windows.find(({ id }) => id === selection.sourceId)
        : visualSources.displays.find(({ id }) => id === (
            selection.kind === "display" ? selection.sourceId : selection.displayId
          ));
      if (!source) throw new Error("The selected preview visual source is unavailable.");
      activeSessionId = nextSessionId++;
      const sessionId = activeSessionId;
      const geometry = selection.kind === "region"
        ? {
            label: `${source.label} · Region`,
            x: source.x + selection.region.x,
            y: source.y + selection.region.y,
            width: selection.region.width,
            height: selection.region.height
          }
        : {
            label: source.label,
            x: source.x,
            y: source.y,
            width: source.width,
            height: source.height
          };
      visualStatus = previewVisualStatus({
        active: true,
        state: "starting",
        sourceLabel: geometry.label,
        message: "Loading preview visual recognition…"
      });
      publishVisualStatus();
      publishRuntime();
      visualTimers = [
        window.setTimeout(() => {
          visualStatus = {
            ...visualStatus,
            state: "capturing",
            message: `Watching preview ${sourceLanguage} text and translating to ${targetLanguage}.`
          };
          publishVisualStatus();
          publishRuntime();
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
          visualStatus = {
            ...visualStatus,
            framesReceived: 4,
            framesAnalyzed: 2,
            framesUnchanged: 1,
            visibleRegions: 1
          };
          publishVisualStatus();
          publishVisualText({
            sessionId,
            runtimeRevision,
            source: geometry,
            visible: [region],
            translationRequests: [region],
            removedTrackIds: []
          });
        }, 900)
      ];
    },
    stopVisualTranslation: async () => {
      for (const timer of visualTimers) window.clearTimeout(timer);
      visualTimers = [];
      visualStatus = {
        ...visualStatus,
        active: true,
        state: "stopping",
        message: "Stopping preview screen translation…"
      };
      publishVisualStatus();
      publishRuntime();
      if (activeSessionId !== null) {
        const event = { sessionId: activeSessionId, runtimeRevision };
        for (const listener of visualClearListeners) listener(event);
      }
      visualStatus = previewVisualStatus();
      activeSessionId = null;
      publishVisualStatus();
      publishRuntime();
    },
    onVisualTextUpdate: async (callback) => {
      visualTextListeners.add(callback);
      return () => visualTextListeners.delete(callback);
    },
    onVisualTextClear: async (callback) => {
      visualClearListeners.add(callback);
      return () => visualClearListeners.delete(callback);
    },

    translationStorageStatus: async () => ({ models: [] }),
    installTranslationModel: async () => {
      throw new Error("Native translation storage is unavailable in browser preview.");
    },
    removeTranslationModel: async () => undefined,
    onTranslationStorageStatus: async () => () => undefined,
    translationModelBaseUrl: () => undefined,

    updateCaptionPresentation: async (frame) => {
      captionPresentation = structuredClone(frame);
    },
    updateVisualPresentation: async (frame) => {
      visualPresentation = structuredClone(frame);
      if (visualStatus.active && visualStatus.state === "capturing") {
        visualStatus = { ...visualStatus, overlayRegions: frame.regions.length };
        publishVisualStatus();
      }
    },
    showAppearance: async () => {
      window.location.href = "/appearance.html";
    },
    closeAppearance: async () => {
      window.location.href = "/";
    },
    windowAction: async () => undefined,
    startWindowDrag: async () => undefined,
    setWindowLayout: async () => undefined,

    reportFrontendDiagnostic: async (scope, message, level = "error") => {
      if (level === "info") console.info(`[${scope}] ${message}`);
      else console.error(`[${scope}] ${message}`);
    },

    setTranscriptForPreview: (snapshot) => {
      transcript = structuredClone(snapshot);
      publishTranscript();
    },
    latestCaptionPresentation: () => captionPresentation
      ? structuredClone(captionPresentation)
      : undefined,
    latestVisualPresentation: () => visualPresentation
      ? structuredClone(visualPresentation)
      : undefined
  };

  return bridge;
}
