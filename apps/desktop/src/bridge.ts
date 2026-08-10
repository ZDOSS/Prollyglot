import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";

import type {
  CaptureSelection,
  CaptureStatus,
  OverlaySettings,
  SourceSnapshot
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

const publishMockStatus = () => {
  for (const listener of mockStatusListeners) listener(mockStatus);
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
  publishMockStatus();
  window.setTimeout(() => {
    mockStatus = { state: "capturing", peak: 0.18, droppedFrames: 0 };
    publishMockStatus();
    mockTimer = window.setInterval(() => {
      mockStatus = { ...mockStatus, peak: 0.08 + Math.random() * 0.72 };
      publishMockStatus();
    }, 180);
  }, 420);
}

export async function stopCapture(): Promise<void> {
  if (isTauri()) {
    await invoke("stop_capture");
    return;
  }

  if (mockTimer !== undefined) window.clearInterval(mockTimer);
  mockTimer = undefined;
  mockStatus = { state: "stopped", peak: 0, droppedFrames: 0 };
  publishMockStatus();
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
