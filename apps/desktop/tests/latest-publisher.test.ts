import assert from "node:assert/strict";
import test from "node:test";

import { LatestPublisher } from "../src/latest-publisher.ts";

function pending(): { promise: Promise<void>; resolve(): void } {
  let resolvePromise!: () => void;
  const promise = new Promise<void>((resolve) => {
    resolvePromise = resolve;
  });
  return { promise, resolve: resolvePromise };
}

async function settle(): Promise<void> {
  await Promise.resolve();
  await Promise.resolve();
}

test("a delayed publisher sends the in-flight value and only its newest replacement", async () => {
  const first = pending();
  const delivered: number[] = [];
  const publisher = new LatestPublisher<number>(async (value) => {
    delivered.push(value);
    if (value === 1) await first.promise;
  });

  publisher.publish(1);
  await settle();
  publisher.publish(2);
  publisher.publish(3);
  first.resolve();
  await publisher.idle();

  assert.deepEqual(delivered, [1, 3]);
});

test("a publish failure is reported without stranding the newest value", async () => {
  const delivered: number[] = [];
  const failures: string[] = [];
  const publisher = new LatestPublisher<number>(async (value) => {
    delivered.push(value);
    if (value === 1) throw new Error("synthetic publish failure");
  }, (error) => failures.push(error instanceof Error ? error.message : String(error)));

  publisher.publish(1);
  publisher.publish(2);
  await publisher.idle();

  assert.deepEqual(delivered, [1, 2]);
  assert.deepEqual(failures, ["synthetic publish failure"]);
});
