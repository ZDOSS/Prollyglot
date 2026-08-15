import type { DesktopBridge } from "./desktop-bridge";
import { createNativeBridge } from "./native-bridge";
import { createPreviewBridge, type PreviewDesktopBridge } from "./preview-bridge";
import type { TranscriptSnapshot, VisualPresentationFrame } from "./types";

declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
    __PROLLYGLOT_PREVIEW__?: {
      setTranscript: (snapshot: TranscriptSnapshot) => void;
      visualPresentation?: VisualPresentationFrame;
    };
  }
}

export const isTauri = (): boolean => window.__TAURI_INTERNALS__ !== undefined;

const selectedBridge: DesktopBridge = isTauri()
  ? createNativeBridge()
  : createPreviewBridge();

export const desktopBridge = selectedBridge;

if (selectedBridge.kind === "preview" && import.meta.env.DEV) {
  const preview = selectedBridge as PreviewDesktopBridge;
  window.__PROLLYGLOT_PREVIEW__ = {
    setTranscript: (snapshot) => preview.setTranscriptForPreview(snapshot)
  };
}
