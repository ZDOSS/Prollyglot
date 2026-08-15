import type { TranslationLanguage } from "./language-catalog";

export type TranslationWorkloadProfile =
  | "captionLive"
  | "captionFinal"
  | "visualCompact"
  | "visualUniversal";

export interface TranslationJobRequest {
  sessionId: string;
  sourceRevision: number;
  workloadProfile: TranslationWorkloadProfile;
  sourceLanguage: TranslationLanguage;
  targetLanguage: TranslationLanguage;
  text: string;
  coalesceKey: string;
  onStarted?: () => void;
}

export interface TranslationJob extends TranslationJobRequest {
  requestId: number;
  priority: number;
  enqueueTimeMs: number;
  deadlineAtMs: number;
}

export interface TranslationExecutor {
  prepare(
    sourceLanguage: TranslationLanguage,
    targetLanguage: TranslationLanguage
  ): Promise<TranslationPreparation>;
  translate(
    sourceLanguage: TranslationLanguage,
    targetLanguage: TranslationLanguage,
    text: string
  ): Promise<string>;
  terminate(reason: Error): void;
}

export interface TranslationPreparation {
  modelId: string;
  coldStartMs: number;
}

export type TranslationExecutorFactory = (sessionId: string) => TranslationExecutor;

export interface TranslationTelemetry {
  event: "loaded" | "unloaded" | "completed" | "failed" | "timedOut" | "cancelled" | "restarted";
  sessionId: string;
  requestId?: number;
  workloadProfile?: TranslationWorkloadProfile;
  sourceRevision?: number;
  sourceLanguage?: TranslationLanguage;
  targetLanguage?: TranslationLanguage;
  modelId?: string;
  queueWaitMs?: number;
  inferenceMs?: number;
  queuedJobs: number;
  reason?: string;
}

interface SchedulerClock {
  now(): number;
  setTimeout(callback: () => void, delayMs: number): unknown;
  clearTimeout(timer: unknown): void;
}

interface ProfilePolicy {
  priority: number;
  deadlineMs: number;
}

interface Deferred<Result> {
  promise: Promise<Result>;
  resolve(value: Result): void;
  reject(error: Error): void;
  settled: boolean;
}

interface QueuedJob {
  job: TranslationJob;
  deferred: Deferred<string>;
  sequence: number;
}

interface ActiveSession {
  id: string;
  generation: number;
  executor: TranslationExecutor;
  queue: QueuedJob[];
  active?: QueuedJob;
  pumping: boolean;
  preparedRoute?: string;
  preparedModelId?: string;
  preparation?: Promise<void>;
  cancellation: Deferred<never>;
}

const PROFILE_POLICIES: Record<TranslationWorkloadProfile, ProfilePolicy> = {
  captionLive: { priority: 200, deadlineMs: 2_500 },
  captionFinal: { priority: 400, deadlineMs: 5_000 },
  visualCompact: { priority: 300, deadlineMs: 3_500 },
  visualUniversal: { priority: 100, deadlineMs: 8_000 }
};

const PREPARATION_DEADLINE_MS = 120_000;
const MAX_QUEUED_JOBS = 16;

export class TranslationSessionError extends Error {}
export class TranslationSessionCancelledError extends TranslationSessionError {}
export class TranslationSupersededError extends TranslationSessionCancelledError {}
export class TranslationDeadlineError extends TranslationSessionError {}
export class TranslationExecutorTerminatedError extends TranslationSessionError {}

export class TranslationScheduler {
  private active?: ActiveSession;
  private nextGeneration = 1;
  private nextRequestId = 1;
  private nextSequence = 1;
  private readonly createExecutor: TranslationExecutorFactory;
  private readonly report: (telemetry: TranslationTelemetry) => void;
  private readonly clock: SchedulerClock;

  constructor(
    createExecutor: TranslationExecutorFactory,
    report: (telemetry: TranslationTelemetry) => void = () => undefined,
    clock: SchedulerClock = browserClock()
  ) {
    this.createExecutor = createExecutor;
    this.report = report;
    this.clock = clock;
  }

  activeSessionId(): string | undefined {
    return this.active?.id;
  }

  startSession(sessionId: string): void {
    const normalized = sessionId.trim();
    if (!normalized) throw new Error("A translation session ID is required.");
    if (this.active?.id === normalized) return;
    this.cancelActive(new TranslationSessionCancelledError(
      "The translation session was replaced by newer work."
    ));
    this.active = this.newSession(normalized);
  }

  stopSession(sessionId: string, reason = "The translation session stopped."): void {
    if (this.active?.id !== sessionId) return;
    this.cancelActive(new TranslationSessionCancelledError(reason));
  }

  restartSession(sessionId: string, reason: string): void {
    const session = this.requireSession(sessionId);
    this.restartExecutor(session, new TranslationSessionError(reason));
  }

  cancelQueued(sessionId: string, coalesceKey: string, reason: string): void {
    const session = this.requireSession(sessionId);
    for (let index = session.queue.length - 1; index >= 0; index -= 1) {
      const queued = session.queue[index];
      if (!queued || queued.job.coalesceKey !== coalesceKey) continue;
      session.queue.splice(index, 1);
      queued.deferred.reject(new TranslationSupersededError(reason));
    }
  }

  async prepare(
    sessionId: string,
    sourceLanguage: TranslationLanguage,
    targetLanguage: TranslationLanguage
  ): Promise<void> {
    const session = this.requireSession(sessionId);
    const route = `${sourceLanguage}:${targetLanguage}`;
    if (session.preparedRoute === route) return;
    if (session.preparation) return session.preparation;

    const generation = session.generation;
    const operation = this.withDeadline(
      session,
      session.executor.prepare(sourceLanguage, targetLanguage),
      PREPARATION_DEADLINE_MS,
      `Loading the ${sourceLanguage} to ${targetLanguage} translator timed out.`
    ).then((preparation) => {
      if (this.isCurrent(sessionId, generation)) {
        session.preparedRoute = route;
        session.preparedModelId = preparation.modelId;
        this.report({
          event: "loaded",
          sessionId: session.id,
          sourceLanguage,
          targetLanguage,
          modelId: preparation.modelId,
          inferenceMs: preparation.coldStartMs,
          queuedJobs: session.queue.length
        });
      }
    }).catch((error: unknown) => {
      const normalized = asError(error);
      if ((normalized instanceof TranslationDeadlineError
          || normalized instanceof TranslationExecutorTerminatedError)
        && this.isCurrent(sessionId, generation)) {
        this.restartExecutor(session, normalized);
      }
      throw normalized;
    }).finally(() => {
      if (this.isCurrent(sessionId, generation) && session.preparation === operation) {
        session.preparation = undefined;
      }
    });
    session.preparation = operation;
    return operation;
  }

  submit(request: TranslationJobRequest): Promise<string> {
    const session = this.requireSession(request.sessionId);
    const policy = PROFILE_POLICIES[request.workloadProfile];
    const enqueueTimeMs = this.clock.now();
    const queued: QueuedJob = {
      job: {
        ...request,
        requestId: this.nextRequestId++,
        priority: policy.priority,
        enqueueTimeMs,
        deadlineAtMs: enqueueTimeMs + policy.deadlineMs
      },
      deferred: deferred<string>(),
      sequence: this.nextSequence++
    };

    this.coalesce(session, queued);
    session.queue.push(queued);
    this.enforceBound(session);
    void this.pump(session);
    return queued.deferred.promise;
  }

  private newSession(id: string): ActiveSession {
    const cancellation = deferred<never>();
    // A session may be stopped while no operation is currently racing this
    // promise. Keep that normal cancellation from becoming an unhandled
    // rejection without changing what active races observe.
    void cancellation.promise.catch(() => undefined);
    return {
      id,
      generation: this.nextGeneration++,
      executor: this.createExecutor(id),
      queue: [],
      pumping: false,
      cancellation
    };
  }

  private requireSession(sessionId: string): ActiveSession {
    if (!this.active || this.active.id !== sessionId) {
      throw new TranslationSessionCancelledError("The translation session is no longer active.");
    }
    return this.active;
  }

  private isCurrent(sessionId: string, generation: number): boolean {
    return this.active?.id === sessionId && this.active.generation === generation;
  }

  private coalesce(session: ActiveSession, incoming: QueuedJob): void {
    for (let index = session.queue.length - 1; index >= 0; index -= 1) {
      const queued = session.queue[index];
      if (!queued || queued.job.coalesceKey !== incoming.job.coalesceKey) continue;
      session.queue.splice(index, 1);
      queued.deferred.reject(new TranslationSupersededError(
        "Newer translation input replaced this queued request."
      ));
    }
  }

  private enforceBound(session: ActiveSession): void {
    while (session.queue.length > MAX_QUEUED_JOBS) {
      const oldestLowestPriority = [...session.queue]
        .sort((left, right) => left.job.priority - right.job.priority || left.sequence - right.sequence)[0];
      if (!oldestLowestPriority) return;
      session.queue.splice(session.queue.indexOf(oldestLowestPriority), 1);
      oldestLowestPriority.deferred.reject(new TranslationSupersededError(
        "The bounded translation queue replaced stale work."
      ));
    }
  }

  private takeNext(session: ActiveSession): QueuedJob | undefined {
    session.queue.sort((left, right) =>
      right.job.priority - left.job.priority || right.sequence - left.sequence
    );
    return session.queue.shift();
  }

  private async pump(session: ActiveSession): Promise<void> {
    if (session.pumping) return;
    session.pumping = true;
    try {
      while (this.isCurrent(session.id, session.generation)) {
        const next = this.takeNext(session);
        if (!next) break;
        if (next.deferred.settled) continue;
        session.active = next;
        await this.execute(session, next);
        if (session.active === next) session.active = undefined;
      }
    } finally {
      session.pumping = false;
      if (this.isCurrent(session.id, session.generation) && session.queue.length > 0) {
        void this.pump(session);
      }
    }
  }

  private async execute(session: ActiveSession, queued: QueuedJob): Promise<void> {
    const { job, deferred: result } = queued;
    const executorBeforePreparation = session.executor;
    let inferenceStartedAt: number | undefined;
    const now = this.clock.now();
    if (now >= job.deadlineAtMs) {
      result.reject(new TranslationDeadlineError("Translation expired before inference began."));
      this.publish("timedOut", session, job, 0, 0, "queue deadline");
      return;
    }

    const route = `${job.sourceLanguage}:${job.targetLanguage}`;
    try {
      if (session.preparedRoute !== route) {
        await this.prepare(session.id, job.sourceLanguage, job.targetLanguage);
      }
      if (!this.isCurrent(session.id, session.generation) || result.settled) return;
      inferenceStartedAt = this.clock.now();
      if (inferenceStartedAt >= job.deadlineAtMs) {
        result.reject(new TranslationDeadlineError(
          "Translation expired while its model was being prepared."
        ));
        this.publish(
          "timedOut",
          session,
          job,
          inferenceStartedAt - job.enqueueTimeMs,
          0,
          "model preparation exceeded the job deadline"
        );
        return;
      }
      job.onStarted?.();
      const remainingMs = Math.max(1, job.deadlineAtMs - inferenceStartedAt);
      const translated = await this.withDeadline(
        session,
        session.executor.translate(job.sourceLanguage, job.targetLanguage, job.text),
        remainingMs,
        `${job.workloadProfile} translation exceeded its deadline.`
      );
      if (!this.isCurrent(session.id, session.generation) || result.settled) return;
      result.resolve(translated);
      this.publish(
        "completed",
        session,
        job,
        inferenceStartedAt - job.enqueueTimeMs,
        this.clock.now() - inferenceStartedAt
      );
    } catch (error) {
      const normalized = asError(error);
      if (!result.settled) result.reject(normalized);
      if (normalized instanceof TranslationDeadlineError
        && this.isCurrent(session.id, session.generation)) {
        this.publish(
          "timedOut",
          session,
          job,
          Math.max(0, (inferenceStartedAt ?? this.clock.now()) - job.enqueueTimeMs),
          inferenceStartedAt === undefined
            ? 0
            : Math.max(0, this.clock.now() - inferenceStartedAt),
          normalized.message
        );
        if (session.executor === executorBeforePreparation) {
          this.restartExecutor(session, normalized);
        }
      } else if (normalized instanceof TranslationExecutorTerminatedError
        && this.isCurrent(session.id, session.generation)) {
        this.publish("failed", session, job, undefined, undefined, normalized.message);
        if (session.executor === executorBeforePreparation) {
          this.restartExecutor(session, normalized);
        }
      } else if (normalized instanceof TranslationSessionCancelledError) {
        this.publish("cancelled", session, job, undefined, undefined, normalized.message);
      } else {
        this.publish("failed", session, job, undefined, undefined, normalized.message);
      }
    }
  }

  private withDeadline<Result>(
    session: ActiveSession,
    operation: Promise<Result>,
    delayMs: number,
    message: string
  ): Promise<Result> {
    let timer: unknown;
    const timeout = new Promise<never>((_resolve, reject) => {
      timer = this.clock.setTimeout(
        () => reject(new TranslationDeadlineError(message)),
        Math.max(1, delayMs)
      );
    });
    return Promise.race([operation, timeout, session.cancellation.promise])
      .finally(() => this.clock.clearTimeout(timer));
  }

  private restartExecutor(session: ActiveSession, error: Error): void {
    this.publishUnload(session, error.message);
    session.executor.terminate(error);
    session.executor = this.createExecutor(session.id);
    session.preparedRoute = undefined;
    session.preparedModelId = undefined;
    session.preparation = undefined;
    this.publish("restarted", session, undefined, undefined, undefined, error.message);
  }

  private cancelActive(error: TranslationSessionCancelledError): void {
    const session = this.active;
    if (!session) return;
    this.active = undefined;
    this.publishUnload(session, error.message);
    session.cancellation.reject(error);
    session.executor.terminate(error);
    if (session.active && !session.active.deferred.settled) {
      session.active.deferred.reject(error);
    }
    for (const queued of session.queue) queued.deferred.reject(error);
    session.queue = [];
    this.publish("cancelled", session, session.active?.job, undefined, undefined, error.message);
  }

  private publishUnload(session: ActiveSession, reason: string): void {
    if (!session.preparedRoute && !session.preparedModelId) return;
    this.report({
      event: "unloaded",
      sessionId: session.id,
      modelId: session.preparedModelId,
      queuedJobs: session.queue.length,
      reason
    });
  }

  private publish(
    event: TranslationTelemetry["event"],
    session: ActiveSession,
    job?: TranslationJob,
    queueWaitMs?: number,
    inferenceMs?: number,
    reason?: string
  ): void {
    this.report({
      event,
      sessionId: session.id,
      requestId: job?.requestId,
      workloadProfile: job?.workloadProfile,
      sourceRevision: job?.sourceRevision,
      sourceLanguage: job?.sourceLanguage,
      targetLanguage: job?.targetLanguage,
      queueWaitMs,
      inferenceMs,
      queuedJobs: session.queue.length,
      reason
    });
  }
}

function deferred<Result>(): Deferred<Result> {
  let resolvePromise!: (value: Result) => void;
  let rejectPromise!: (error: Error) => void;
  const state: Deferred<Result> = {
    promise: Promise.resolve(undefined as Result),
    resolve: () => undefined,
    reject: () => undefined,
    settled: false
  };
  state.promise = new Promise<Result>((resolve, reject) => {
    resolvePromise = resolve;
    rejectPromise = reject;
  });
  state.resolve = (value) => {
    if (state.settled) return;
    state.settled = true;
    resolvePromise(value);
  };
  state.reject = (error) => {
    if (state.settled) return;
    state.settled = true;
    rejectPromise(error);
  };
  return state;
}

function browserClock(): SchedulerClock {
  return {
    now: () => Date.now(),
    setTimeout: (callback, delayMs) => globalThis.setTimeout(callback, delayMs),
    clearTimeout: (timer) => globalThis.clearTimeout(timer as ReturnType<typeof setTimeout>)
  };
}

function asError(error: unknown): Error {
  return error instanceof Error ? error : new Error(String(error));
}

export function isExpectedTranslationCancellation(error: unknown): boolean {
  return error instanceof TranslationSessionCancelledError;
}
