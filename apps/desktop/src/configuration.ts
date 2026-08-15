import type { DesktopBridge, Unsubscribe } from "./desktop-bridge.ts";
import { supportedTranslationLanguage } from "./language-catalog.ts";
import type {
  ApplicationConfiguration,
  ConfigurationSnapshot,
  OverlaySettings,
  VisualPreferences
} from "./types.ts";

export const LEGACY_CONFIGURATION_KEYS = [
  "prollyglot.caption-output",
  "prollyglot.translation-target",
  "prollyglot.view-mode",
  "prollyglot.overlay",
  "prollyglot.visual-translation"
] as const;

export interface ConfigurationStorage {
  getItem(key: string): string | null;
  removeItem(key: string): void;
}

export interface LegacyMigrationResult {
  config: ApplicationConfiguration;
  diagnostics: string[];
}

export type ConfigurationMutation = (config: ApplicationConfiguration) => void;
export type ConfigurationListener = (
  snapshot: Readonly<ConfigurationSnapshot>
) => void;

interface PendingMutation {
  mutate: ConfigurationMutation;
  resolve: (snapshot: ConfigurationSnapshot) => void;
  reject: (error: unknown) => void;
}

export type ConfigurationBridge = Pick<
  DesktopBridge,
  "configurationSnapshot" | "updateConfiguration" | "onConfiguration"
>;

export async function initializeConfiguration(
  bridge: ConfigurationBridge,
  storage: ConfigurationStorage,
  diagnostic: (message: string) => void = () => undefined
): Promise<ConfigurationController> {
  let snapshot = await bridge.configurationSnapshot();
  if (snapshot.diagnostic) diagnostic(snapshot.diagnostic);

  if (!snapshot.config.legacyWebviewImported) {
    const migration = migrateLegacyConfiguration(snapshot.config, storage);
    for (const message of migration.diagnostics) diagnostic(message);
    try {
      const accepted = await bridge.updateConfiguration(snapshot.revision, migration.config);
      const readback = await bridge.configurationSnapshot();
      if (
        readback.revision !== accepted.revision
        || !configurationEqual(readback.config, accepted.config)
      ) {
        throw new Error("Native configuration readback did not match the accepted migration.");
      }
      snapshot = readback;
      for (const key of LEGACY_CONFIGURATION_KEYS) storage.removeItem(key);
    } catch (error) {
      diagnostic(
        `Could not finish the one-time settings migration: ${errorMessage(error)}`
      );
    }
  }

  const controller = new ConfigurationController(bridge, snapshot, diagnostic);
  await controller.connect();
  return controller;
}

export function migrateLegacyConfiguration(
  current: ApplicationConfiguration,
  storage: ConfigurationStorage
): LegacyMigrationResult {
  const config = structuredClone(current);
  const diagnostics: string[] = [];

  const viewMode = storage.getItem("prollyglot.view-mode");
  if (viewMode === "full" || viewMode === "compact") config.viewMode = viewMode;
  else if (viewMode !== null) diagnostics.push("Ignored an invalid legacy window-layout setting.");

  const captionMode = storage.getItem("prollyglot.caption-output");
  if (captionMode === "original" || captionMode === "translated" || captionMode === "both") {
    config.captions.outputMode = captionMode;
  } else if (captionMode === "english") {
    config.captions.outputMode = "translated";
  } else if (captionMode !== null) {
    diagnostics.push("Ignored an invalid legacy caption-output setting.");
  }

  const translationTarget = storage.getItem("prollyglot.translation-target");
  if (translationTarget === "off") {
    delete config.captions.translationTarget;
    config.captions.outputMode = "original";
  } else if (translationTarget !== null) {
    const supported = supportedTranslationLanguage(translationTarget);
    if (supported) config.captions.translationTarget = supported;
    else diagnostics.push("Ignored an invalid legacy translation-language setting.");
  }

  migrateJsonSetting(
    storage.getItem("prollyglot.overlay"),
    isOverlaySettings,
    (overlay) => { config.overlay = overlay; },
    "caption appearance",
    diagnostics
  );
  migrateJsonSetting(
    storage.getItem("prollyglot.visual-translation"),
    isVisualPreferences,
    (visual) => { config.visual = visual; },
    "screen translation",
    diagnostics
  );

  if (
    config.captions.outputMode !== "original"
    && !config.captions.translationTarget
  ) {
    config.captions.outputMode = "original";
  }
  config.legacyWebviewImported = true;
  return { config, diagnostics };
}

export class ConfigurationController {
  private readonly bridge: ConfigurationBridge;
  private readonly diagnostic: (message: string) => void;
  private snapshotValue: ConfigurationSnapshot;
  private readonly listeners = new Set<ConfigurationListener>();
  private readonly pending: PendingMutation[] = [];
  private pumping?: Promise<void>;
  private unsubscribe?: Unsubscribe;

  constructor(
    bridge: ConfigurationBridge,
    initial: ConfigurationSnapshot,
    diagnostic: (message: string) => void = () => undefined
  ) {
    this.bridge = bridge;
    this.diagnostic = diagnostic;
    this.snapshotValue = structuredClone(initial);
  }

  snapshot(): Readonly<ConfigurationSnapshot> {
    return this.snapshotValue;
  }

  subscribe(listener: ConfigurationListener): Unsubscribe {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  async connect(): Promise<void> {
    this.unsubscribe = await this.bridge.onConfiguration((snapshot) => {
      if (snapshot.revision < this.snapshotValue.revision) return;
      this.accept(snapshot);
    });
    const latest = await this.bridge.configurationSnapshot();
    if (latest.revision >= this.snapshotValue.revision) this.accept(latest);
  }

  dispose(): void {
    this.unsubscribe?.();
    this.unsubscribe = undefined;
  }

  update(mutate: ConfigurationMutation): Promise<ConfigurationSnapshot> {
    const result = new Promise<ConfigurationSnapshot>((resolve, reject) => {
      this.pending.push({ mutate, resolve, reject });
    });
    this.startPump();
    return result;
  }

  private startPump(): void {
    if (this.pumping) return;
    this.pumping = this.pump().finally(() => {
      this.pumping = undefined;
      if (this.pending.length > 0) this.startPump();
    });
  }

  private async pump(): Promise<void> {
    while (this.pending.length > 0) {
      const batch = this.pending.splice(0);
      try {
        const accepted = await this.writeBatch(batch);
        for (const item of batch) item.resolve(structuredClone(accepted));
      } catch (error) {
        for (const item of batch) item.reject(error);
        this.diagnostic(`Could not save settings: ${errorMessage(error)}`);
      }
    }
  }

  private async writeBatch(batch: PendingMutation[]): Promise<ConfigurationSnapshot> {
    for (let attempt = 0; attempt < 2; attempt += 1) {
      const config = structuredClone(this.snapshotValue.config);
      for (const { mutate } of batch) mutate(config);
      try {
        const accepted = await this.bridge.updateConfiguration(
          this.snapshotValue.revision,
          config
        );
        const readback = await this.bridge.configurationSnapshot();
        if (
          readback.revision !== accepted.revision
          || !configurationEqual(readback.config, accepted.config)
        ) {
          throw new Error("Native configuration readback did not match the accepted write.");
        }
        this.accept(readback);
        return readback;
      } catch (error) {
        if (attempt === 0 && errorMessage(error).toLowerCase().includes("stale")) {
          this.accept(await this.bridge.configurationSnapshot());
          continue;
        }
        throw error;
      }
    }
    throw new Error("Could not reconcile the latest configuration revision.");
  }

  private accept(snapshot: ConfigurationSnapshot): void {
    if (
      snapshot.revision === this.snapshotValue.revision
      && configurationEqual(snapshot.config, this.snapshotValue.config)
    ) return;
    this.snapshotValue = structuredClone(snapshot);
    for (const listener of this.listeners) listener(this.snapshotValue);
  }
}

function migrateJsonSetting<T>(
  raw: string | null,
  validate: (value: unknown) => value is T,
  apply: (value: T) => void,
  label: string,
  diagnostics: string[]
): void {
  if (raw === null) return;
  try {
    const value: unknown = JSON.parse(raw);
    if (!validate(value)) throw new Error("invalid shape");
    apply(structuredClone(value));
  } catch {
    diagnostics.push(`Ignored an invalid legacy ${label} setting.`);
  }
}

function isOverlaySettings(value: unknown): value is OverlaySettings {
  if (!isRecord(value)) return false;
  return typeof value.fontFamily === "string"
    && value.fontFamily.trim().length > 0
    && integerBetween(value.fontSize, 18, 96)
    && isHexColor(value.textColor)
    && isHexColor(value.translatedTextColor)
    && (value.bilingualLayout === "stacked" || value.bilingualLayout === "sideBySide")
    && finiteBetween(value.backgroundOpacity, 0, 1)
    && integerBetween(value.width, 320, 1_600)
    && integerBetween(value.maximumLines, 1, 4)
    && integerBetween(value.readingTimeSeconds, 3, 60)
    && integerBetween(value.fadeDurationMs, 0, 5_000)
    && ["topCenter", "bottomCenter", "bottomLeft", "bottomRight"].includes(
      String(value.position)
    )
    && typeof value.clickThrough === "boolean";
}

function isVisualPreferences(value: unknown): value is VisualPreferences {
  if (!isRecord(value)) return false;
  const mode = value.sourceMode ?? value.mode ?? "applicationWindow";
  const sourceLanguage = supportedTranslationLanguage(String(value.sourceLanguage ?? "ja"));
  const targetLanguage = supportedTranslationLanguage(String(value.targetLanguage ?? "en"));
  const detectionMode = value.detectionMode ?? "focused";
  if (
    !["applicationWindow", "display", "region"].includes(String(mode))
    || !sourceLanguage
    || !targetLanguage
    || sourceLanguage === targetLanguage
    || (detectionMode !== "focused" && detectionMode !== "allText")
  ) return false;
  if (value.windowId !== undefined && !validOpaqueId(value.windowId)) return false;
  if (value.displayId !== undefined && !validOpaqueId(value.displayId)) return false;
  value.sourceMode = mode;
  value.sourceLanguage = sourceLanguage;
  value.targetLanguage = targetLanguage;
  value.detectionMode = detectionMode;
  delete value.mode;
  return true;
}

function validOpaqueId(value: unknown): value is string {
  return typeof value === "string"
    && value.trim().length > 0
    && value.length <= 1_024
    && !/[\u0000-\u001f\u007f]/u.test(value);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function finiteBetween(value: unknown, minimum: number, maximum: number): value is number {
  return typeof value === "number"
    && Number.isFinite(value)
    && value >= minimum
    && value <= maximum;
}

function integerBetween(value: unknown, minimum: number, maximum: number): value is number {
  return finiteBetween(value, minimum, maximum) && Number.isInteger(value);
}

function isHexColor(value: unknown): value is string {
  return typeof value === "string" && /^#[0-9a-f]{6}$/iu.test(value);
}

function configurationEqual(
  left: ApplicationConfiguration,
  right: ApplicationConfiguration
): boolean {
  return JSON.stringify(left) === JSON.stringify(right);
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
