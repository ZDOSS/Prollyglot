import type { CaptionPresentationFrame } from "./types";

export interface PresentationIdentity {
  sessionId: number;
  runtimeRevision: number;
  presentationRevision: number;
}

export type CaptionDisplayState =
  | { phase: "visible"; nextAtMs?: number }
  | { phase: "fading"; nextAtMs: number }
  | { phase: "hidden" };

const TRANSLATION_MAX_WAIT_MS = 30_000;

export function acceptsPresentationFrame(
  current: PresentationIdentity | undefined,
  next: PresentationIdentity
): boolean {
  if (!current) return true;
  if (next.runtimeRevision < current.runtimeRevision) return false;
  if (next.sessionId !== current.sessionId) {
    return next.runtimeRevision > current.runtimeRevision;
  }
  return next.presentationRevision > current.presentationRevision;
}

export class PresentationCursor<Frame extends PresentationIdentity> {
  private frame?: Frame;

  accept(next: Frame): boolean {
    if (!acceptsPresentationFrame(this.frame, next)) return false;
    this.frame = structuredClone(next);
    return true;
  }

  current(): Frame | undefined {
    return this.frame ? structuredClone(this.frame) : undefined;
  }
}

export function captionDisplayState(
  frame: CaptionPresentationFrame,
  readingTimeSeconds: number,
  fadeDurationMs: number,
  nowMs: number
): CaptionDisplayState {
  if (frame.phase === "cleared" || frame.entries.length === 0 || frame.readableAtMs <= 0) {
    return { phase: "hidden" };
  }
  if (frame.phase === "active") return { phase: "visible" };

  const readingDeadline = frame.readableAtMs + Math.max(0, readingTimeSeconds) * 1_000;
  const translationDeadline = frame.entries.some(({ translationPending }) => translationPending)
    ? frame.readableAtMs + TRANSLATION_MAX_WAIT_MS
    : 0;
  const fadeAtMs = Math.max(readingDeadline, translationDeadline);
  if (nowMs < fadeAtMs) return { phase: "visible", nextAtMs: fadeAtMs };

  const hiddenAtMs = fadeAtMs + Math.max(0, fadeDurationMs);
  if (fadeDurationMs > 0 && nowMs < hiddenAtMs) {
    return { phase: "fading", nextAtMs: hiddenAtMs };
  }
  return { phase: "hidden" };
}
