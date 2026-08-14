import type { RuntimeSnapshot, SessionLifecycle } from "./types";

export interface RuntimeCursor {
  snapshot?: RuntimeSnapshot;
  visualEventRevisionFloor: number;
}

export interface RuntimeReduction {
  cursor: RuntimeCursor;
  accepted: boolean;
  contractMismatch: boolean;
  sessionChanged: boolean;
}

export const initialRuntimeCursor = (): RuntimeCursor => ({
  visualEventRevisionFloor: 0
});

export function reduceRuntimeSnapshot(
  current: RuntimeCursor,
  next: RuntimeSnapshot,
  expectedContractVersion: number
): RuntimeReduction {
  if (next.contractVersion !== expectedContractVersion) {
    return {
      cursor: current,
      accepted: false,
      contractMismatch: true,
      sessionChanged: false
    };
  }
  if (current.snapshot && next.revision <= current.snapshot.revision) {
    return {
      cursor: current,
      accepted: false,
      contractMismatch: false,
      sessionChanged: false
    };
  }

  const previous = current.snapshot;
  const sessionChanged = previous?.sessionId !== next.sessionId
    || previous?.mode !== next.mode;
  let visualEventRevisionFloor = current.visualEventRevisionFloor;
  if (next.mode !== "visualTranslation" || next.sessionId === null) {
    visualEventRevisionFloor = 0;
  } else if (
    sessionChanged
    || invalidatesVisualOutput(next.lifecycle)
    || (previous?.mode === "visualTranslation"
      && previous.lifecycle === "waiting"
      && next.lifecycle === "running")
  ) {
    visualEventRevisionFloor = next.revision;
  }

  return {
    cursor: { snapshot: next, visualEventRevisionFloor },
    accepted: true,
    contractMismatch: false,
    sessionChanged
  };
}

export function acceptsVisualSessionEvent(
  cursor: RuntimeCursor,
  sessionId: number,
  runtimeRevision: number,
  allowTerminal: boolean
): boolean {
  const current = cursor.snapshot;
  if (!current
    || current.mode !== "visualTranslation"
    || current.sessionId !== sessionId
    || runtimeRevision < cursor.visualEventRevisionFloor
    || runtimeRevision > current.revision) {
    return false;
  }
  return allowTerminal || current.lifecycle === "starting" || current.lifecycle === "running";
}

function invalidatesVisualOutput(lifecycle: SessionLifecycle): boolean {
  return lifecycle === "waiting"
    || lifecycle === "stopping"
    || lifecycle === "failed"
    || lifecycle === "stopped";
}
