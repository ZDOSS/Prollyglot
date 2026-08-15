export * from "./generated/runtime";

import {
  DEFAULT_APPLICATION_CONFIGURATION,
  type OverlaySettings
} from "./generated/runtime";

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
  storage?: "native" | "legacy";
}

export interface TranslationCatalogStatus {
  models: TranslationModelStatus[];
}

export interface TranslationStorageStatus {
  phase: Exclude<TranslationPhase, "loading">;
  storageId: string;
  downloadedBytes: number;
  totalBytes: number;
  message?: string;
}

export interface TranslationStorageCatalog {
  models: TranslationStorageStatus[];
}

export const DEFAULT_OVERLAY_SETTINGS: OverlaySettings = {
  ...DEFAULT_APPLICATION_CONFIGURATION.overlay
};
