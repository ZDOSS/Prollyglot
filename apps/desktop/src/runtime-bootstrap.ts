import type { DesktopBridge } from "./desktop-bridge";
import type { CaptureStatus, RuntimeSnapshot, VisualStatus } from "./types";

type RuntimeBootstrapBridge = Pick<DesktopBridge,
  | "onRuntimeState"
  | "onCaptureStatus"
  | "onVisualStatus"
  | "runtimeBootstrap"
  | "captureStatus"
  | "visualStatus"
>;

export interface RuntimeBootstrapSinks {
  applyRuntime: (snapshot: RuntimeSnapshot) => void;
  renderCapture: (status: CaptureStatus) => void;
  renderVisual: (status: VisualStatus) => void;
}

export async function initializeRuntimeBootstrap(
  bridge: RuntimeBootstrapBridge,
  sinks: RuntimeBootstrapSinks
): Promise<void> {
  let bootstrapping = true;
  let newestEvent: RuntimeSnapshot | undefined;
  await bridge.onRuntimeState((next) => {
    if (bootstrapping) {
      if (!newestEvent || next.revision > newestEvent.revision) newestEvent = next;
      return;
    }
    sinks.applyRuntime(next);
  });
  await Promise.all([
    bridge.onCaptureStatus(sinks.renderCapture),
    bridge.onVisualStatus(sinks.renderVisual)
  ]);
  const [bootstrap, audioStatus, screenStatus] = await Promise.all([
    bridge.runtimeBootstrap(),
    bridge.captureStatus(),
    bridge.visualStatus()
  ]);
  sinks.renderCapture(audioStatus);
  sinks.renderVisual(screenStatus);
  bootstrapping = false;
  const newest = newestEvent && newestEvent.revision > bootstrap.snapshot.revision
    ? newestEvent
    : bootstrap.snapshot;
  sinks.applyRuntime(newest);
}
