import {
  initialRuntimeCursor,
  reduceRuntimeSnapshot,
  type RuntimeCursor,
  type RuntimeReduction
} from "./runtime-state.ts";
import type {
  ConfigurationSnapshot,
  CaptionOutputMode,
  CaptureStatus,
  ModelCatalogStatus,
  ModelStatus,
  RuntimeSnapshot,
  SourceSnapshot,
  TranscriptSnapshot,
  TranslationCatalogStatus,
  VisualCaptureCapabilities,
  VisualModelCatalogStatus,
  VisualSourceSnapshot,
  VisualStatus
} from "./types";

export type AppViewMode = "full" | "compact";
export type AppDestination =
  | "captions"
  | "visual"
  | "transcript"
  | "models"
  | "appearance"
  | "settings";
export type NoticeTone = "neutral" | "success" | "error";

export interface AppNotice {
  message: string;
  tone: NoticeTone;
}

export interface AppPreferences {
  acceptedSpokenLanguage: string;
  captionMode: CaptionOutputMode;
  translationTarget: string;
}

export interface AppState {
  configuration?: ConfigurationSnapshot;
  runtime: RuntimeCursor;
  runtimeContractMismatch?: number;
  sources: SourceSnapshot;
  captureStatus: CaptureStatus;
  speechModels: ModelCatalogStatus;
  transcript: TranscriptSnapshot;
  translations: TranslationCatalogStatus;
  visualCapabilities: VisualCaptureCapabilities;
  visualSources: VisualSourceSnapshot;
  visualModels: VisualModelCatalogStatus;
  visualStatus: VisualStatus;
  navigation: {
    viewMode: AppViewMode;
    destination: AppDestination;
  };
  preferences: AppPreferences;
  notices: {
    capture?: AppNotice;
    settings?: AppNotice;
  };
  transcriptFollowLatest: boolean;
}

export const FALLBACK_SPEECH_MODEL: ModelStatus = {
  phase: "failed",
  modelId: "initial-english",
  displayName: "English streaming model",
  profile: "English",
  description: "Local streaming English captions.",
  languages: ["en"],
  downloadedBytes: 0,
  totalBytes: 0,
  message: "No English speech models are available."
};

export interface InitialAppStateOptions {
  configuration?: ConfigurationSnapshot;
  viewMode?: AppViewMode;
  acceptedSpokenLanguage?: string;
  captionMode?: CaptionOutputMode;
  translationTarget?: string;
  translations?: TranslationCatalogStatus;
}

export function createInitialAppState(options: InitialAppStateOptions = {}): AppState {
  const configured = options.configuration?.config;
  return {
    configuration: options.configuration
      ? structuredClone(options.configuration)
      : undefined,
    runtime: initialRuntimeCursor(),
    sources: { playbackDevices: [], applications: [] },
    captureStatus: { state: "stopped", peak: 0, droppedFrames: 0 },
    speechModels: {
      selectedModelId: FALLBACK_SPEECH_MODEL.modelId,
      models: [{ ...FALLBACK_SPEECH_MODEL, languages: [...FALLBACK_SPEECH_MODEL.languages] }]
    },
    transcript: { revision: 0, committed: [] },
    translations: structuredClone(options.translations ?? { models: [] }),
    visualCapabilities: {
      windowsGraphicsCapture: false,
      systemPicker: false,
      desktopDuplicationExperiment: false,
      message: "Checking Windows screen capture…"
    },
    visualSources: { windows: [], displays: [] },
    visualModels: { models: [] },
    visualStatus: stoppedVisualStatus(),
    navigation: {
      viewMode: configured?.viewMode ?? options.viewMode ?? "full",
      destination: "captions"
    },
    preferences: {
      acceptedSpokenLanguage: configured?.captions.spokenLanguage
        ?? options.acceptedSpokenLanguage
        ?? "en",
      captionMode: configured?.captions.outputMode ?? options.captionMode ?? "original",
      translationTarget: configured?.captions.translationTarget
        ?? options.translationTarget
        ?? "off"
    },
    notices: {},
    transcriptFollowLatest: true
  };
}

export type AppAction =
  | { type: "configuration/accepted"; snapshot: ConfigurationSnapshot }
  | { type: "runtime/received"; snapshot: RuntimeSnapshot; expectedContractVersion: number }
  | { type: "sources/replaced"; snapshot: SourceSnapshot }
  | { type: "capture/status"; status: CaptureStatus }
  | { type: "speech/catalog"; catalog: ModelCatalogStatus }
  | { type: "transcript/received"; transcript: TranscriptSnapshot }
  | { type: "translation/catalog"; catalog: TranslationCatalogStatus }
  | { type: "visual/capabilities"; capabilities: VisualCaptureCapabilities }
  | { type: "visual/sources"; snapshot: VisualSourceSnapshot }
  | { type: "visual/catalog"; catalog: VisualModelCatalogStatus }
  | { type: "visual/status"; status: VisualStatus }
  | { type: "navigation/view-mode"; viewMode: AppViewMode }
  | { type: "navigation/destination"; destination: AppDestination }
  | { type: "preferences/spoken-language"; language: string }
  | { type: "preferences/caption-mode"; mode: CaptionOutputMode }
  | { type: "preferences/translation-target"; language: string }
  | { type: "notice/capture"; notice?: AppNotice }
  | { type: "notice/settings"; notice?: AppNotice }
  | { type: "transcript/follow-latest"; follow: boolean };

export interface AppDispatchResult {
  changed: boolean;
  runtime?: RuntimeReduction;
}

export type AppStoreListener = (
  state: Readonly<AppState>,
  previous: Readonly<AppState>,
  action: AppAction
) => void;

export class AppStore {
  private state: AppState;
  private readonly listeners = new Set<AppStoreListener>();

  constructor(initial: AppState) {
    this.state = structuredClone(initial);
  }

  getState(): Readonly<AppState> {
    return this.state;
  }

  subscribe(listener: AppStoreListener): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  dispatch(action: AppAction): AppDispatchResult {
    const previous = this.state;
    const reduced = reduceAppState(previous, action);
    if (reduced.state === previous) {
      return { changed: false, runtime: reduced.runtime };
    }
    this.state = reduced.state;
    for (const listener of this.listeners) listener(this.state, previous, action);
    return { changed: true, runtime: reduced.runtime };
  }
}

interface AppReduction {
  state: AppState;
  runtime?: RuntimeReduction;
}

export function reduceAppState(state: AppState, action: AppAction): AppReduction {
  switch (action.type) {
    case "configuration/accepted": {
      if (
        state.configuration
        && action.snapshot.revision < state.configuration.revision
      ) return { state };
      const snapshot = structuredClone(action.snapshot);
      return {
        state: {
          ...state,
          configuration: snapshot,
          navigation: {
            ...state.navigation,
            viewMode: snapshot.config.viewMode
          },
          preferences: {
            acceptedSpokenLanguage: snapshot.config.captions.spokenLanguage,
            captionMode: snapshot.config.captions.outputMode,
            translationTarget: snapshot.config.captions.translationTarget ?? "off"
          }
        }
      };
    }
    case "runtime/received": {
      const runtime = reduceRuntimeSnapshot(
        state.runtime,
        action.snapshot,
        action.expectedContractVersion
      );
      if (runtime.contractMismatch) {
        if (state.runtimeContractMismatch === action.snapshot.contractVersion) {
          return { state, runtime };
        }
        return {
          state: { ...state, runtimeContractMismatch: action.snapshot.contractVersion },
          runtime
        };
      }
      if (!runtime.accepted) return { state, runtime };
      return {
        state: {
          ...state,
          runtime: runtime.cursor,
          runtimeContractMismatch: undefined
        },
        runtime
      };
    }
    case "sources/replaced":
      return { state: { ...state, sources: structuredClone(action.snapshot) } };
    case "capture/status":
      return { state: { ...state, captureStatus: structuredClone(action.status) } };
    case "speech/catalog":
      return { state: { ...state, speechModels: structuredClone(action.catalog) } };
    case "transcript/received":
      if (action.transcript.revision < state.transcript.revision) return { state };
      return { state: { ...state, transcript: structuredClone(action.transcript) } };
    case "translation/catalog":
      return { state: { ...state, translations: structuredClone(action.catalog) } };
    case "visual/capabilities":
      return { state: { ...state, visualCapabilities: structuredClone(action.capabilities) } };
    case "visual/sources":
      return { state: { ...state, visualSources: structuredClone(action.snapshot) } };
    case "visual/catalog":
      return { state: { ...state, visualModels: structuredClone(action.catalog) } };
    case "visual/status":
      return { state: { ...state, visualStatus: structuredClone(action.status) } };
    case "navigation/view-mode":
      if (state.navigation.viewMode === action.viewMode) return { state };
      return {
        state: {
          ...state,
          navigation: { ...state.navigation, viewMode: action.viewMode }
        }
      };
    case "navigation/destination":
      if (state.navigation.destination === action.destination) return { state };
      return {
        state: {
          ...state,
          navigation: { ...state.navigation, destination: action.destination }
        }
      };
    case "preferences/spoken-language":
      if (state.preferences.acceptedSpokenLanguage === action.language) return { state };
      return {
        state: {
          ...state,
          preferences: { ...state.preferences, acceptedSpokenLanguage: action.language }
        }
      };
    case "preferences/caption-mode":
      if (state.preferences.captionMode === action.mode) return { state };
      return {
        state: {
          ...state,
          preferences: { ...state.preferences, captionMode: action.mode }
        }
      };
    case "preferences/translation-target":
      if (state.preferences.translationTarget === action.language) return { state };
      return {
        state: {
          ...state,
          preferences: { ...state.preferences, translationTarget: action.language }
        }
      };
    case "notice/capture":
      return {
        state: {
          ...state,
          notices: { ...state.notices, capture: cloneNotice(action.notice) }
        }
      };
    case "notice/settings":
      return {
        state: {
          ...state,
          notices: { ...state.notices, settings: cloneNotice(action.notice) }
        }
      };
    case "transcript/follow-latest":
      if (state.transcriptFollowLatest === action.follow) return { state };
      return { state: { ...state, transcriptFollowLatest: action.follow } };
  }
}

function cloneNotice(notice: AppNotice | undefined): AppNotice | undefined {
  return notice ? { ...notice } : undefined;
}

function stoppedVisualStatus(): VisualStatus {
  return {
    active: false,
    state: "stopped",
    framesReceived: 0,
    framesAnalyzed: 0,
    framesUnchanged: 0,
    replacedFrames: 0,
    visibleRegions: 0,
    overlayRegions: 0
  };
}
