export * from "./generated/runtime";

export type ModelPhase = "checking" | "notInstalled" | "downloading" | "ready" | "corrupt" | "failed";

export interface ModelStatus {
  phase: ModelPhase;
  modelId: string;
  displayName: string;
  profile: string;
  description: string;
  languages: string[];
  downloadedBytes: number;
  totalBytes: number;
  message?: string;
}

export interface ModelCatalogStatus {
  selectedModelId: string;
  models: ModelStatus[];
}

export interface VisualModelCatalogStatus {
  models: ModelStatus[];
}


export interface TranscriptSegment {
  utteranceId: number;
  startMicros: number;
  endMicros: number;
  sourceLanguage: string;
  text: string;
  isFinal: boolean;
}

export interface TranscriptSnapshot {
  revision: number;
  provisional?: TranscriptSegment;
  committed: TranscriptSegment[];
}

export type CaptionOutputMode = "original" | "translated" | "both";

export interface CaptionOutputEntry {
  key: string;
  sourceLanguage: string;
  original: string;
  translation?: string;
  translationPending?: boolean;
  isFinal: boolean;
}

export interface CaptionOutputPayload {
  mode: CaptionOutputMode;
  targetLanguage?: string;
  originalCaption: string;
  entries: CaptionOutputEntry[];
}

export type TranslationPhase =
  | "checking"
  | "notInstalled"
  | "downloading"
  | "loading"
  | "ready"
  | "corrupt"
  | "failed";

export interface TranslationModelStatus {
  phase: TranslationPhase;
  kind: "direct" | "toEnglish" | "manyToMany";
  sourceLanguages: string[];
  targetLanguages: string[];
  modelId: string;
  displayName: string;
  license: "Apache-2.0" | "MIT";
  downloadedBytes: number;
  totalBytes: number;
  message?: string;
}

export interface TranslationCatalogStatus {
  models: TranslationModelStatus[];
}

export type BilingualLayout = "stacked" | "sideBySide";

export interface OverlaySettings {
  fontFamily: string;
  fontSize: number;
  textColor: string;
  translatedTextColor: string;
  bilingualLayout: BilingualLayout;
  backgroundOpacity: number;
  width: number;
  maximumLines: number;
  readingTimeSeconds: number;
  fadeDurationMs: number;
  position: "topCenter" | "bottomCenter" | "bottomLeft" | "bottomRight";
  clickThrough: boolean;
}

export const DEFAULT_OVERLAY_SETTINGS: OverlaySettings = {
  fontFamily: '"Segoe UI Variable", "Segoe UI", sans-serif',
  fontSize: 36,
  textColor: "#f4f6f5",
  translatedTextColor: "#86e3b0",
  bilingualLayout: "stacked",
  backgroundOpacity: 0.75,
  width: 720,
  maximumLines: 3,
  readingTimeSeconds: 15,
  fadeDurationMs: 800,
  position: "bottomCenter",
  clickThrough: true
};
