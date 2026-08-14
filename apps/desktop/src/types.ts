export * from "./generated/runtime";

export type SourceId = string;

export interface PlaybackDevice {
  id: SourceId;
  name: string;
  isDefault: boolean;
}

export interface ApplicationSource {
  id: SourceId;
  name: string;
  processId: number;
  deviceIds: SourceId[];
}

export interface SourceSnapshot {
  playbackDevices: PlaybackDevice[];
  applications: ApplicationSource[];
}

export type CaptureSelection =
  | { kind: "systemDefault" }
  | { kind: "systemOutput"; deviceId: SourceId }
  | { kind: "application"; processId: number };

export type CaptureState =
  | "starting"
  | "capturing"
  | "waiting"
  | "stopping"
  | "stopped"
  | "failed";

export interface CaptureStatus {
  state: CaptureState;
  peak: number;
  droppedFrames: number;
  sourceLabel?: string;
  message?: string;
}

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

export interface VisualCaptureCapabilities {
  windowsGraphicsCapture: boolean;
  systemPicker: boolean;
  desktopDuplicationExperiment: boolean;
  message?: string;
}

export type VisualSourceKind = "applicationWindow" | "display";
export type VisualDetectionMode = "focused" | "allText";

export interface VisualSource {
  id: string;
  kind: VisualSourceKind;
  label: string;
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface VisualSourceSnapshot {
  windows: VisualSource[];
  displays: VisualSource[];
}

export interface VisualCaptureGeometry {
  label: string;
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface PixelRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export type VisualCaptureSelection =
  | { kind: "applicationWindow"; sourceId: string }
  | { kind: "display"; sourceId: string }
  | { kind: "region"; displayId: string; region: PixelRect };

export type VisualState =
  | "starting"
  | "capturing"
  | "waiting"
  | "stopping"
  | "stopped"
  | "failed";

export interface VisualStatus {
  active: boolean;
  state: VisualState;
  sourceLabel?: string;
  framesReceived: number;
  framesAnalyzed: number;
  framesUnchanged: number;
  replacedFrames: number;
  visibleRegions: number;
  overlayRegions: number;
  message?: string;
}

export interface VisualRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface StableVisualTextRegion {
  trackId: number;
  textRevision: number;
  text: string;
  confidence: number;
  language?: string;
  script?: string;
  bounds: VisualRect;
}

export interface VisualTextUpdate {
  source: VisualCaptureGeometry;
  visible: StableVisualTextRegion[];
  translationRequests: StableVisualTextRegion[];
  removedTrackIds: number[];
}

export interface VisualOutputRegion {
  trackId: number;
  textRevision: number;
  original: string;
  translation?: string;
  translationPending: boolean;
  retained?: boolean;
  bounds: VisualRect;
}

export interface VisualOutputPayload {
  sourceWidth: number;
  sourceHeight: number;
  sourceLanguage: string;
  targetLanguage: string;
  scanning: boolean;
  regions: VisualOutputRegion[];
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
