import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { LogicalSize } from "@tauri-apps/api/dpi";
import { emit, listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";

import type { DesktopBridge } from "./desktop-bridge";
import { RUNTIME_COMMANDS, RUNTIME_EVENTS } from "./types";
import type {
  ApplicationConfiguration,
  CaptionPresentationFrame,
  CaptureStatus,
  ConfigurationSnapshot,
  ModelCatalogStatus,
  PixelRect,
  RuntimeBootstrap,
  RuntimeStateEvent,
  ShowVisualRegionSelectorCommand,
  SourceSnapshot,
  StartCaptureCommand,
  StartVisualTranslationCommand,
  TranscriptSnapshot,
  TranslationStorageCatalog,
  UpdateConfigurationCommand,
  UpdateCaptionPresentationCommand,
  UpdateVisualPresentationCommand,
  VisualCaptureCapabilities,
  VisualModelCatalogStatus,
  VisualRegionSelected,
  VisualRegionSelectorRequest,
  VisualSourceSnapshot,
  VisualStatus,
  VisualTextClear,
  VisualTextUpdate
} from "./types";

export function createNativeBridge(): DesktopBridge {
  return {
    kind: "native",

    configurationSnapshot: () => invoke<ConfigurationSnapshot>(
      RUNTIME_COMMANDS.configurationSnapshot
    ),
    updateConfiguration: (expectedRevision, config: ApplicationConfiguration) =>
      invoke<ConfigurationSnapshot>(RUNTIME_COMMANDS.updateConfiguration, {
        command: { expectedRevision, config } satisfies UpdateConfigurationCommand
      }),
    onConfiguration: (callback) => listen<ConfigurationSnapshot>(
      RUNTIME_EVENTS.configuration,
      ({ payload }) => callback(payload)
    ),

    sourceSnapshot: () => invoke<SourceSnapshot>(RUNTIME_COMMANDS.sourceSnapshot),
    startCapture: async (selection, language) => {
      await invoke(
        RUNTIME_COMMANDS.startCapture,
        { selection, language } satisfies StartCaptureCommand
      );
    },
    stopCapture: async () => {
      await invoke(RUNTIME_COMMANDS.stopCapture);
    },
    captureStatus: () => invoke<CaptureStatus>(RUNTIME_COMMANDS.captureStatus),
    onCaptureStatus: (callback) => listen<CaptureStatus>(
      RUNTIME_EVENTS.captureStatus,
      ({ payload }) => callback(payload)
    ),

    runtimeBootstrap: () => invoke<RuntimeBootstrap>(RUNTIME_COMMANDS.bootstrap),
    onRuntimeState: (callback) => listen<RuntimeStateEvent>(
      RUNTIME_EVENTS.state,
      ({ payload }) => callback(payload.snapshot)
    ),

    modelStatus: () => invoke<ModelCatalogStatus>("model_status"),
    selectSpeechModel: async (modelId) => {
      await invoke("select_speech_model", { modelId });
    },
    installSpeechModel: async (modelId) => {
      await invoke("install_speech_model", { modelId });
    },
    removeSpeechModel: async (modelId) => {
      await invoke("remove_speech_model", { modelId });
    },
    onModelStatus: (callback) => listen<ModelCatalogStatus>(
      "model-status",
      ({ payload }) => callback(payload)
    ),

    transcriptSnapshot: () => invoke<TranscriptSnapshot>("transcript_snapshot"),
    clearTranscript: async () => {
      await invoke("clear_transcript");
    },
    onTranscriptUpdate: (callback) => listen<TranscriptSnapshot>(
      "transcript-update",
      ({ payload }) => callback(payload)
    ),

    visualCapabilities: () => invoke<VisualCaptureCapabilities>(
      RUNTIME_COMMANDS.visualCapabilities
    ),
    visualSourceSnapshot: () => invoke<VisualSourceSnapshot>(
      RUNTIME_COMMANDS.visualSourceSnapshot
    ),
    pickVisualRegion: pickVisualRegion,
    visualStatus: () => invoke<VisualStatus>(RUNTIME_COMMANDS.visualStatus),
    onVisualStatus: (callback) => listen<VisualStatus>(
      RUNTIME_EVENTS.visualStatus,
      ({ payload }) => callback(payload)
    ),
    visualModelStatus: () => invoke<VisualModelCatalogStatus>("visual_model_status"),
    installVisualModel: async (modelId) => {
      await invoke("install_visual_model", { modelId });
    },
    removeVisualModel: async (modelId) => {
      await invoke("remove_visual_model", { modelId });
    },
    onVisualModelStatus: (callback) => listen<VisualModelCatalogStatus>(
      "visual-model-status",
      ({ payload }) => callback(payload)
    ),
    startVisualTranslation: async (
      selection,
      sourceLanguage,
      targetLanguage,
      detectionMode
    ) => {
      await invoke(
        RUNTIME_COMMANDS.startVisualTranslation,
        {
          selection,
          sourceLanguage,
          targetLanguage,
          detectionMode
        } satisfies StartVisualTranslationCommand
      );
    },
    stopVisualTranslation: async () => {
      await invoke(RUNTIME_COMMANDS.stopVisualTranslation);
    },
    onVisualTextUpdate: (callback) => listen<VisualTextUpdate>(
      RUNTIME_EVENTS.visualText,
      ({ payload }) => callback(payload)
    ),
    onVisualTextClear: (callback) => listen<VisualTextClear>(
      RUNTIME_EVENTS.visualClear,
      ({ payload }) => callback(payload)
    ),

    translationStorageStatus: () => invoke<TranslationStorageCatalog>(
      "translation_model_status"
    ),
    installTranslationModel: async (storageId) => {
      await invoke("install_translation_model", { storageId });
    },
    removeTranslationModel: async (storageId) => {
      await invoke("remove_translation_model", { storageId });
    },
    onTranslationStorageStatus: (callback) => listen<TranslationStorageCatalog>(
      "translation-model-status",
      ({ payload }) => callback(payload)
    ),
    translationModelBaseUrl: () => convertFileSrc(
      "translation",
      "prollyglot-model"
    ).replace(/\/$/u, ""),

    updateCaptionPresentation: async (frame: CaptionPresentationFrame) => {
      await invoke<boolean>(
        RUNTIME_COMMANDS.updateCaptionPresentation,
        { frame } satisfies UpdateCaptionPresentationCommand
      );
    },
    updateVisualPresentation: async (frame) => {
      await invoke<boolean>(
        RUNTIME_COMMANDS.updateVisualPresentation,
        { frame } satisfies UpdateVisualPresentationCommand
      );
    },
    showAppearance: async () => {
      await invoke("show_appearance_window");
    },
    closeAppearance: async () => {
      await invoke("close_appearance_window");
    },
    windowAction: async (action) => {
      const current = getCurrentWindow();
      if (action === "minimize") await current.minimize();
      if (action === "maximize") {
        if (await current.isMaximized()) await current.unmaximize();
        else await current.maximize();
      }
      if (action === "close") await current.close();
    },
    startWindowDrag: async () => {
      await getCurrentWindow().startDragging();
    },
    setWindowLayout: async (layout) => {
      const current = getCurrentWindow();
      if (await current.isMaximized()) await current.unmaximize();
      await current.setSize(layout === "full"
        ? new LogicalSize(1180, 760)
        : new LogicalSize(440, 640));
    },

    reportFrontendDiagnostic: async (scope, message, level = "error") => {
      if (level === "info") console.info(`[${scope}] ${message}`);
      else console.error(`[${scope}] ${message}`);
      try {
        await invoke("report_frontend_diagnostic", { scope, message });
      } catch (error) {
        console.error("Could not write the frontend diagnostic to the Prollyglot log.", error);
      }
    }
  };
}

async function pickVisualRegion(displayId: string): Promise<PixelRect | undefined> {
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
