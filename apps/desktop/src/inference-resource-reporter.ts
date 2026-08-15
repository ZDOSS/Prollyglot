import type { TranslationTelemetry } from "./translation-scheduler";
import type {
  InferenceResourceSnapshot,
  ReportInferenceResourceCommand,
  RuntimeSnapshot,
  SessionMode
} from "./types";

interface NativeOwner {
  sessionId: number;
  mode: SessionMode;
}

type ResourceReport = (
  command: ReportInferenceResourceCommand
) => Promise<InferenceResourceSnapshot>;

/**
 * Serializes WebView inference-lifetime reports and binds each disposable
 * translation worker to the native session that caused it to load.
 */
export class InferenceResourceReporter {
  private readonly translationOwners = new Map<string, NativeOwner>();
  private readonly report: ResourceReport;
  private readonly runtimeSnapshot: () => RuntimeSnapshot | undefined;
  private readonly reportDiagnostic: (message: string) => void;
  private serial = Promise.resolve();

  constructor(
    report: ResourceReport,
    runtimeSnapshot: () => RuntimeSnapshot | undefined,
    reportDiagnostic: (message: string) => void = () => undefined
  ) {
    this.report = report;
    this.runtimeSnapshot = runtimeSnapshot;
    this.reportDiagnostic = reportDiagnostic;
  }

  acceptTranslationTelemetry(telemetry: TranslationTelemetry): void {
    if (telemetry.event === "loaded") {
      const runtime = this.runtimeSnapshot();
      if (!telemetry.modelId || !isInferenceActive(runtime)) {
        this.reportDiagnostic(
          `Ignored a translation load outside an active native session (${telemetry.sessionId}).`
        );
        return;
      }
      const owner = { sessionId: runtime.sessionId, mode: runtime.mode };
      this.translationOwners.set(telemetry.sessionId, owner);
      this.enqueue({
        ...owner,
        ownerId: telemetry.sessionId,
        kind: "translation",
        phase: "loaded",
        modelId: telemetry.modelId,
        coldStartMillis: finiteMilliseconds(telemetry.inferenceMs)
      });
      return;
    }

    if (telemetry.event !== "unloaded") return;
    const owner = this.translationOwners.get(telemetry.sessionId);
    if (!owner) return;
    this.translationOwners.delete(telemetry.sessionId);
    this.enqueue({
      ...owner,
      ownerId: telemetry.sessionId,
      kind: "translation",
      phase: "unloaded",
      modelId: telemetry.modelId ?? null,
      coldStartMillis: 0
    });
  }

  async settled(): Promise<void> {
    await this.serial;
  }

  private enqueue(command: ReportInferenceResourceCommand): void {
    this.serial = this.serial
      .then(() => this.report(command))
      .then(() => undefined)
      .catch((error: unknown) => {
        const message = error instanceof Error ? error.message : String(error);
        this.reportDiagnostic(`Inference resource report failed: ${message}`);
      });
  }
}

function isInferenceActive(runtime: RuntimeSnapshot | undefined): runtime is RuntimeSnapshot & {
  sessionId: number;
  mode: SessionMode;
} {
  return runtime !== undefined
    && runtime.sessionId !== null
    && runtime.mode !== null
    && (runtime.lifecycle === "starting"
      || runtime.lifecycle === "running"
      || runtime.lifecycle === "waiting");
}

function finiteMilliseconds(value: number | undefined): number {
  return Number.isFinite(value) ? Math.max(0, Math.round(value ?? 0)) : 0;
}
