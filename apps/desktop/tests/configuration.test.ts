import assert from "node:assert/strict";
import test from "node:test";

import {
  ConfigurationController,
  initializeConfiguration,
  migrateLegacyConfiguration,
  type ConfigurationBridge,
  type ConfigurationStorage
} from "../src/configuration.ts";
import { DEFAULT_APPLICATION_CONFIGURATION } from "../src/generated/runtime.ts";
import type {
  ApplicationConfiguration,
  ConfigurationSnapshot
} from "../src/types.ts";

class MemoryStorage implements ConfigurationStorage {
  readonly values = new Map<string, string>();
  readonly removed: string[] = [];

  getItem(key: string): string | null {
    return this.values.get(key) ?? null;
  }

  removeItem(key: string): void {
    this.removed.push(key);
    this.values.delete(key);
  }
}

class ConfigurationFake implements ConfigurationBridge {
  snapshotValue: ConfigurationSnapshot = {
    revision: 1,
    config: structuredClone(DEFAULT_APPLICATION_CONFIGURATION)
  };
  writes = 0;
  failWrites = false;
  mismatchReadback = false;
  staleOnce = false;
  blockFirstWrite?: Promise<void>;
  private readonly listeners = new Set<(snapshot: ConfigurationSnapshot) => void>();

  async configurationSnapshot(): Promise<ConfigurationSnapshot> {
    const snapshot = structuredClone(this.snapshotValue);
    if (this.mismatchReadback && this.writes > 0) snapshot.config.viewMode = "full";
    return snapshot;
  }

  async updateConfiguration(
    expectedRevision: number,
    config: ApplicationConfiguration
  ): Promise<ConfigurationSnapshot> {
    this.writes += 1;
    if (this.writes === 1 && this.blockFirstWrite) await this.blockFirstWrite;
    if (this.failWrites) throw new Error("disk unavailable");
    if (this.staleOnce) {
      this.staleOnce = false;
      this.snapshotValue = {
        revision: this.snapshotValue.revision + 1,
        config: {
          ...this.snapshotValue.config,
          models: { speechModelId: "external-model" }
        }
      };
      throw new Error(
        `Configuration revision ${expectedRevision} is stale; current revision is ${this.snapshotValue.revision}.`
      );
    }
    if (expectedRevision !== this.snapshotValue.revision) throw new Error("stale revision");
    this.snapshotValue = {
      revision: expectedRevision + 1,
      config: structuredClone(config)
    };
    for (const listener of this.listeners) listener(structuredClone(this.snapshotValue));
    return structuredClone(this.snapshotValue);
  }

  async onConfiguration(
    callback: (snapshot: ConfigurationSnapshot) => void
  ): Promise<() => void> {
    this.listeners.add(callback);
    return () => this.listeners.delete(callback);
  }
}

test("legacy WebView values migrate once into the versioned native document", async () => {
  const storage = new MemoryStorage();
  storage.values.set("prollyglot.view-mode", "compact");
  storage.values.set("prollyglot.caption-output", "both");
  storage.values.set("prollyglot.translation-target", "es");
  storage.values.set("prollyglot.overlay", JSON.stringify({
    ...DEFAULT_APPLICATION_CONFIGURATION.overlay,
    fontSize: 44
  }));
  storage.values.set("prollyglot.visual-translation", JSON.stringify({
    mode: "display",
    sourceLanguage: "ja",
    targetLanguage: "en",
    displayId: "display-1"
  }));

  const migrated = migrateLegacyConfiguration(
    structuredClone(DEFAULT_APPLICATION_CONFIGURATION),
    storage
  );
  assert.equal(migrated.config.viewMode, "compact");
  assert.equal(migrated.config.captions.outputMode, "both");
  assert.equal(migrated.config.captions.translationTarget, "es");
  assert.equal(migrated.config.overlay.fontSize, 44);
  assert.equal(migrated.config.visual.sourceMode, "display");
  assert.equal(migrated.config.visual.detectionMode, "focused");
  assert.equal(migrated.config.legacyWebviewImported, true);
  assert.deepEqual(migrated.diagnostics, []);

  const bridge = new ConfigurationFake();
  const controller = await initializeConfiguration(bridge, storage);
  assert.equal(controller.snapshot().config.viewMode, "compact");
  assert.equal(storage.values.size, 0);
  assert.ok(storage.removed.includes("prollyglot.overlay"));
});

test("corrupt legacy values cannot poison launch and are diagnosed once", async () => {
  const storage = new MemoryStorage();
  storage.values.set("prollyglot.view-mode", "sideways");
  storage.values.set("prollyglot.overlay", "{broken json");
  const diagnostics: string[] = [];

  const controller = await initializeConfiguration(
    new ConfigurationFake(),
    storage,
    (message) => diagnostics.push(message)
  );

  assert.equal(controller.snapshot().config.viewMode, "full");
  assert.equal(controller.snapshot().config.legacyWebviewImported, true);
  assert.equal(diagnostics.length, 2);
  assert.equal(storage.values.size, 0);
});

test("legacy keys remain until native write and readback agree", async () => {
  const storage = new MemoryStorage();
  storage.values.set("prollyglot.view-mode", "compact");
  const bridge = new ConfigurationFake();
  bridge.mismatchReadback = true;

  const controller = await initializeConfiguration(bridge, storage);

  assert.equal(storage.values.get("prollyglot.view-mode"), "compact");
  assert.equal(controller.snapshot().config.viewMode, "full");
});

test("rapid changes coalesce while preserving every mutation", async () => {
  const bridge = new ConfigurationFake();
  bridge.snapshotValue.config.legacyWebviewImported = true;
  let releaseFirst!: () => void;
  bridge.blockFirstWrite = new Promise<void>((resolve) => { releaseFirst = resolve; });
  const controller = new ConfigurationController(bridge, bridge.snapshotValue);
  await controller.connect();

  const first = controller.update((config) => { config.viewMode = "compact"; });
  await Promise.resolve();
  const second = controller.update((config) => { config.overlay.fontSize = 44; });
  const third = controller.update((config) => { config.captions.spokenLanguage = "es"; });
  releaseFirst();
  await Promise.all([first, second, third]);

  assert.equal(bridge.writes, 2);
  assert.equal(controller.snapshot().config.viewMode, "compact");
  assert.equal(controller.snapshot().config.overlay.fontSize, 44);
  assert.equal(controller.snapshot().config.captions.spokenLanguage, "es");
});

test("a change queued by a completed coalesced write cannot be stranded", async () => {
  const bridge = new ConfigurationFake();
  bridge.snapshotValue.config.legacyWebviewImported = true;
  let releaseFirst!: () => void;
  bridge.blockFirstWrite = new Promise<void>((resolve) => { releaseFirst = resolve; });
  const controller = new ConfigurationController(bridge, bridge.snapshotValue);
  await controller.connect();

  const first = controller.update((config) => { config.viewMode = "compact"; });
  await Promise.resolve();
  const second = controller.update((config) => { config.overlay.fontSize = 44; });
  releaseFirst();
  await first;
  await second.then(() => controller.update((config) => {
    config.overlay.maximumLines = 4;
  }));

  assert.equal(controller.snapshot().config.overlay.maximumLines, 4);
  assert.equal(bridge.writes, 3);
});

test("a stale write rebases preferences without erasing a native model update", async () => {
  const bridge = new ConfigurationFake();
  bridge.snapshotValue.config.legacyWebviewImported = true;
  bridge.staleOnce = true;
  const controller = new ConfigurationController(bridge, bridge.snapshotValue);
  await controller.connect();

  await controller.update((config) => { config.viewMode = "compact"; });

  assert.equal(controller.snapshot().config.viewMode, "compact");
  assert.equal(controller.snapshot().config.models.speechModelId, "external-model");
  assert.equal(bridge.writes, 2);
});
