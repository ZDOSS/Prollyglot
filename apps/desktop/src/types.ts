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

export type ModelPhase = "notInstalled" | "downloading" | "ready" | "corrupt" | "failed";

export interface ModelStatus {
  phase: ModelPhase;
  modelId: string;
  displayName: string;
  downloadedBytes: number;
  totalBytes: number;
  message?: string;
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

export interface OverlaySettings {
  fontFamily: string;
  fontSize: number;
  textColor: string;
  backgroundOpacity: number;
  width: number;
  maximumLines: number;
  position: "topCenter" | "bottomCenter" | "bottomLeft" | "bottomRight";
  clickThrough: boolean;
}

export const DEFAULT_OVERLAY_SETTINGS: OverlaySettings = {
  fontFamily: '"Segoe UI Variable", "Segoe UI", sans-serif',
  fontSize: 36,
  textColor: "#f4f6f5",
  backgroundOpacity: 0.75,
  width: 720,
  maximumLines: 2,
  position: "bottomCenter",
  clickThrough: true
};
