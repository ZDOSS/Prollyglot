import assert from "node:assert/strict";
import test from "node:test";

import {
  TranslationDeadlineError,
  TranslationExecutorTerminatedError,
  TranslationScheduler,
  TranslationSessionCancelledError,
  TranslationSupersededError,
  type TranslationExecutor,
  type TranslationJobRequest
} from "../src/translation-scheduler.ts";

interface Pending<Result> {
  promise: Promise<Result>;
  resolve(value: Result): void;
  reject(error: Error): void;
}

class FakeExecutor implements TranslationExecutor {
  readonly calls: string[] = [];
  terminated?: Error;
  private readonly behavior: (text: string) => Promise<string>;

  constructor(
    behavior: (text: string) => Promise<string> = async (text) => `translated:${text}`
  ) {
    this.behavior = behavior;
  }

  async prepare() {
    return { modelId: "fake-translator", coldStartMs: 0 };
  }

  translate(_source: string, _target: string, text: string): Promise<string> {
    this.calls.push(text);
    return this.behavior(text);
  }

  terminate(reason: Error): void {
    this.terminated = reason;
  }
}

class FakeClock {
  nowMs = 0;
  private nextId = 1;
  private readonly timers = new Map<number, { at: number; callback: () => void }>();

  now(): number {
    return this.nowMs;
  }

  setTimeout(callback: () => void, delayMs: number): number {
    const id = this.nextId++;
    this.timers.set(id, { at: this.nowMs + delayMs, callback });
    return id;
  }

  clearTimeout(timer: unknown): void {
    this.timers.delete(timer as number);
  }

  advance(milliseconds: number): void {
    this.nowMs += milliseconds;
    while (true) {
      const due = [...this.timers.entries()]
        .filter(([, timer]) => timer.at <= this.nowMs)
        .sort((left, right) => left[1].at - right[1].at)[0];
      if (!due) return;
      this.timers.delete(due[0]);
      due[1].callback();
    }
  }
}

function job(
  sessionId: string,
  text: string,
  workloadProfile: TranslationJobRequest["workloadProfile"],
  coalesceKey = text
): TranslationJobRequest {
  return {
    sessionId,
    sourceRevision: 1,
    workloadProfile,
    sourceLanguage: "ja",
    targetLanguage: "en",
    text,
    coalesceKey
  };
}

function pending<Result>(): Pending<Result> {
  let resolvePromise!: (value: Result) => void;
  let rejectPromise!: (error: Error) => void;
  const promise = new Promise<Result>((resolve, reject) => {
    resolvePromise = resolve;
    rejectPromise = reject;
  });
  return { promise, resolve: resolvePromise, reject: rejectPromise };
}

async function settle(): Promise<void> {
  await Promise.resolve();
  await Promise.resolve();
  await Promise.resolve();
}

test("final captions outrank queued provisional and visual work", async () => {
  const blocker = pending<string>();
  const executor = new FakeExecutor((text) => text === "blocker"
    ? blocker.promise
    : Promise.resolve(`translated:${text}`));
  const scheduler = new TranslationScheduler(() => executor);
  scheduler.startSession("captions:1");

  const active = scheduler.submit(job("captions:1", "blocker", "captionLive"));
  await settle();
  const provisional = scheduler.submit(job("captions:1", "partial", "captionLive"));
  const visual = scheduler.submit(job("captions:1", "sign", "visualCompact"));
  const final = scheduler.submit(job("captions:1", "final", "captionFinal"));

  blocker.resolve("translated:blocker");
  await Promise.all([active, provisional, visual, final]);
  assert.deepEqual(executor.calls, ["blocker", "final", "sign", "partial"]);
});

test("new provisional text coalesces queued text from the same utterance", async () => {
  const blocker = pending<string>();
  const executor = new FakeExecutor((text) => text === "blocker"
    ? blocker.promise
    : Promise.resolve(`translated:${text}`));
  const scheduler = new TranslationScheduler(() => executor);
  scheduler.startSession("captions:1");

  const active = scheduler.submit(job("captions:1", "blocker", "captionFinal"));
  await settle();
  const stale = scheduler.submit(job("captions:1", "old partial", "captionLive", "utterance:4"));
  const staleResult = assert.rejects(stale, TranslationSupersededError);
  const newest = scheduler.submit(job("captions:1", "new partial", "captionLive", "utterance:4"));

  blocker.resolve("translated:blocker");
  await Promise.all([active, newest, staleResult]);
  assert.deepEqual(executor.calls, ["blocker", "new partial"]);
});

test("a failing translator rejects one job and continues with current work", async () => {
  const executor = new FakeExecutor(async (text) => {
    if (text === "bad") throw new Error("synthetic inference failure");
    return `translated:${text}`;
  });
  const scheduler = new TranslationScheduler(() => executor);
  scheduler.startSession("visual:1");

  await assert.rejects(
    scheduler.submit(job("visual:1", "bad", "visualCompact")),
    /synthetic inference failure/
  );
  assert.equal(
    await scheduler.submit(job("visual:1", "current", "visualCompact")),
    "translated:current"
  );
  assert.equal(executor.terminated, undefined);
});

test("a crashed executor is replaced before later work runs", async () => {
  const executors: FakeExecutor[] = [];
  const scheduler = new TranslationScheduler(() => {
    const executor = executors.length === 0
      ? new FakeExecutor(async () => {
        throw new TranslationExecutorTerminatedError("synthetic worker crash");
      })
      : new FakeExecutor();
    executors.push(executor);
    return executor;
  });
  scheduler.startSession("visual:1");

  await assert.rejects(
    scheduler.submit(job("visual:1", "crash", "visualCompact")),
    TranslationExecutorTerminatedError
  );
  assert.equal(executors.length, 2);
  assert.ok(executors[0]?.terminated instanceof TranslationExecutorTerminatedError);
  assert.equal(
    await scheduler.submit(job("visual:1", "fresh", "visualCompact")),
    "translated:fresh"
  );
});

test("a never-resolving inference reaches its deadline and restarts only its session worker", async () => {
  const clock = new FakeClock();
  const executors: FakeExecutor[] = [];
  const scheduler = new TranslationScheduler(() => {
    const executor = executors.length === 0
      ? new FakeExecutor(() => new Promise(() => undefined))
      : new FakeExecutor();
    executors.push(executor);
    return executor;
  }, () => undefined, clock);
  scheduler.startSession("visual:1");

  const timedOut = scheduler.submit(job("visual:1", "never", "visualCompact"));
  const timedOutResult = assert.rejects(timedOut, TranslationDeadlineError);
  for (let attempts = 0; attempts < 20 && executors[0]?.calls.length === 0; attempts += 1) {
    await settle();
  }
  assert.deepEqual(executors[0]?.calls, ["never"]);
  clock.advance(3_500);
  await timedOutResult;
  await settle();

  assert.equal(executors.length, 2);
  assert.ok(executors[0]?.terminated instanceof TranslationDeadlineError);
  assert.equal(
    await scheduler.submit(job("visual:1", "fresh", "visualCompact")),
    "translated:fresh"
  );
});

test("stopping a session rejects active and queued work and ignores late completion", async () => {
  const slow = pending<string>();
  const executor = new FakeExecutor((text) => text === "slow"
    ? slow.promise
    : Promise.resolve(`translated:${text}`));
  const scheduler = new TranslationScheduler(() => executor);
  scheduler.startSession("captions:1");

  const active = scheduler.submit(job("captions:1", "slow", "captionFinal"));
  const activeResult = assert.rejects(active, TranslationSessionCancelledError);
  await settle();
  const queued = scheduler.submit(job("captions:1", "queued", "captionLive"));
  const queuedResult = assert.rejects(queued, TranslationSessionCancelledError);
  scheduler.stopSession("captions:1");
  slow.resolve("too late");

  await Promise.all([activeResult, queuedResult]);
  assert.ok(executor.terminated instanceof TranslationSessionCancelledError);
  assert.equal(scheduler.activeSessionId(), undefined);
});
