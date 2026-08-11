import "./styles.css";

import {
  captureStatus,
  clearTranscript,
  isTauri,
  installSpeechModel,
  modelStatus,
  onCaptureStatus,
  onModelStatus,
  onTranscriptUpdate,
  reportFrontendDiagnostic,
  removeSpeechModel,
  selectSpeechModel,
  showAppearance,
  sourceSnapshot,
  startWindowDrag,
  startCapture,
  stopCapture,
  transcriptSnapshot,
  updateCaptionOutput,
  updateOverlaySettings,
  windowAction
} from "./bridge";
import { CaptionOutputController, supportedSourceLanguage } from "./caption-output";
import { icons } from "./icons";
import { TranslationService } from "./translation";
import type {
  CaptionOutputMode,
  CaptureSelection,
  CaptureStatus,
  ModelCatalogStatus,
  ModelStatus,
  SourceSnapshot,
  TranscriptSegment,
  TranscriptSnapshot,
  TranslationCatalogStatus,
  TranslationModelStatus
} from "./types";
import { DEFAULT_OVERLAY_SETTINGS, type OverlaySettings } from "./types";

const root = document.querySelector<HTMLElement>("#app");
if (!root) throw new Error("missing app root");

root.innerHTML = `
  <section class="app-window main-window" aria-label="Prollyglot controls">
    <header class="titlebar">
      <div class="brand">
        <img class="brand-mark" src="/branding/prollyglot-mark.png" alt="" />
        <span class="brand-name">Prollyglot</span>
        <span class="status-label" data-state="stopped"><span class="status-dot"></span><span id="status-text">Ready</span></span>
      </div>
      <div class="window-controls" aria-label="Window controls">
        <button class="window-control" type="button" data-window-action="minimize" aria-label="Minimize">${icons.minimize}</button>
        <button class="window-control" type="button" data-window-action="maximize" aria-label="Maximize">${icons.maximize}</button>
        <button class="window-control close" type="button" data-window-action="close" aria-label="Close">${icons.close}</button>
      </div>
    </header>

    <div class="main-content">
      <section id="model-setup" class="model-setup" aria-labelledby="model-setup-title" hidden>
        <div class="model-copy">
          <span class="model-kicker">Local model required</span>
          <h2 id="model-setup-title">Local captions</h2>
          <p id="model-message">Download the selected speech model once, then caption offline.</p>
        </div>
        <progress id="model-progress" class="model-progress" max="1" value="0" hidden></progress>
        <button id="model-action" class="secondary-button model-action" type="button">Download model</button>
      </section>

      <section id="translation-setup" class="model-setup translation-setup" aria-labelledby="translation-setup-title" hidden>
        <div class="model-copy">
          <span class="model-kicker">Optional local translation</span>
          <h2 id="translation-setup-title">English captions</h2>
          <p id="translation-message">Download the English translator once, then translate offline.</p>
        </div>
        <progress id="translation-progress" class="model-progress" max="1" value="0" hidden></progress>
        <button id="translation-action" class="secondary-button model-action" type="button">Download translator</button>
      </section>

      <div class="field-group">
        <label class="field-label" for="audio-source">Audio source</label>
        <div class="select-wrap">
          <select id="audio-source" class="select-control" aria-describedby="source-help">
            <option value="system">Everything I hear</option>
          </select>
          ${icons.chevronDown}
        </div>
        <span id="source-help" class="sr-only">Choose all audio from a playback device or one application.</span>
      </div>

      <div class="field-group" id="device-field">
        <label class="field-label" for="playback-device">Playback device</label>
        <div class="select-wrap">
          <select id="playback-device" class="select-control"></select>
          ${icons.chevronDown}
        </div>
      </div>

      <div class="field-grid">
        <div class="field-group">
          <label class="field-label" for="spoken-language">Spoken language</label>
          <div class="select-wrap">
            <select id="spoken-language" class="select-control" aria-describedby="spoken-language-help">
              <option value="en">English</option>
              <option value="es">Spanish</option>
              <option value="ja">Japanese</option>
              <option value="auto">Automatic · mixed languages</option>
            </select>
            ${icons.chevronDown}
          </div>
          <span id="spoken-language-help" class="field-help"></span>
        </div>

        <div class="field-group">
          <label class="field-label" for="caption-language">Caption output</label>
          <div class="select-wrap">
            <select id="caption-language" class="select-control" aria-describedby="caption-language-help">
              <option value="original">Original language</option>
            </select>
            ${icons.chevronDown}
          </div>
          <span id="caption-language-help" class="field-help"></span>
        </div>
      </div>

      <p id="capture-message" class="capture-message" role="status" aria-live="polite"></p>

      <button id="capture-toggle" class="primary-button" type="button">Start Captions</button>
    </div>

    <nav class="utility-nav" aria-label="Application views">
      <button type="button" class="utility-action" data-panel="transcript">${icons.transcript}<span>Transcript</span></button>
      <button type="button" class="utility-action" id="appearance-action">${icons.appearance}<span>Appearance</span></button>
      <button type="button" class="utility-action" data-panel="settings">${icons.settings}<span>Settings</span></button>
    </nav>

    <dialog id="utility-dialog" class="utility-dialog" aria-labelledby="dialog-title">
      <div class="dialog-title-row">
        <h2 id="dialog-title"></h2>
        <button type="button" class="dialog-close" aria-label="Close">${icons.close}</button>
      </div>
      <div id="dialog-content"></div>
    </dialog>
  </section>
`;

const sourceSelect = requireElement<HTMLSelectElement>("#audio-source");
const deviceSelect = requireElement<HTMLSelectElement>("#playback-device");
const deviceField = requireElement<HTMLElement>("#device-field");
const spokenLanguage = requireElement<HTMLSelectElement>("#spoken-language");
const captionLanguage = requireElement<HTMLSelectElement>("#caption-language");
const captureToggle = requireElement<HTMLButtonElement>("#capture-toggle");
const captureMessage = requireElement<HTMLElement>("#capture-message");
const statusLabel = requireElement<HTMLElement>(".status-label");
const statusText = requireElement<HTMLElement>("#status-text");
const modelSetup = requireElement<HTMLElement>("#model-setup");
const modelSetupTitle = requireElement<HTMLElement>("#model-setup-title");
const modelMessage = requireElement<HTMLElement>("#model-message");
const modelProgress = requireElement<HTMLProgressElement>("#model-progress");
const modelAction = requireElement<HTMLButtonElement>("#model-action");
const translationSetup = requireElement<HTMLElement>("#translation-setup");
const translationSetupTitle = requireElement<HTMLElement>("#translation-setup-title");
const translationMessage = requireElement<HTMLElement>("#translation-message");
const translationProgress = requireElement<HTMLProgressElement>("#translation-progress");
const translationAction = requireElement<HTMLButtonElement>("#translation-action");
const dialog = requireElement<HTMLDialogElement>("#utility-dialog");
const CAPTION_MODE_STORAGE_KEY = "prollyglot.caption-output";

let snapshot: SourceSnapshot = { playbackDevices: [], applications: [] };
let currentStatus: CaptureStatus = { state: "stopped", peak: 0, droppedFrames: 0 };
const FALLBACK_MODEL: ModelStatus = {
  phase: "failed",
  modelId: "initial-english",
  displayName: "English streaming model",
  profile: "English",
  description: "Local streaming English captions.",
  languages: ["en"],
  downloadedBytes: 0,
  totalBytes: 0,
  message: "No English speech models are available."
};
let currentModels: ModelCatalogStatus = {
  selectedModelId: FALLBACK_MODEL.modelId,
  models: [FALLBACK_MODEL]
};
const useMockTranslation = !isTauri()
  && !new URLSearchParams(window.location.search).has("realTranslation");
const translationService = new TranslationService(useMockTranslation);
let currentTranslations = translationService.snapshot();
let currentTranscript: TranscriptSnapshot = { revision: 0, committed: [] };
let settingsNotice: { message: string; tone: "neutral" | "success" | "error" } | undefined;
let acceptedSpokenLanguage = "en";
let preferredCaptionMode = storedCaptionMode();
let transcriptFollowLatest = true;
const FOLLOW_SYSTEM_DEFAULT = "__follow-system-default__";
const TRANSCRIPT_BOTTOM_THRESHOLD = 48;
const LANGUAGE_LABELS: Record<string, string> = {
  auto: "Automatic detection",
  en: "English",
  es: "Spanish",
  ja: "Japanese"
};
const captionOutput = new CaptionOutputController(
  translationService,
  (payload) => {
    if (dialog.open && dialog.dataset.panel === "transcript") renderTranscriptPanel();
    return updateCaptionOutput(payload);
  },
  (message) => {
    captureMessage.textContent = message;
    void reportFrontendDiagnostic("translation", message);
  },
  (message) => {
    void reportFrontendDiagnostic("translation-performance", message, "info");
  }
);

function requireElement<T extends Element>(selector: string): T {
  const element = document.querySelector<T>(selector);
  if (!element) throw new Error(`missing element: ${selector}`);
  return element;
}

function option(value: string, label: string, selected = false): HTMLOptionElement {
  const element = document.createElement("option");
  element.value = value;
  element.textContent = label;
  element.selected = selected;
  return element;
}

function storedCaptionMode(): CaptionOutputMode {
  const stored = localStorage.getItem(CAPTION_MODE_STORAGE_KEY);
  return stored === "english" || stored === "both" ? stored : "original";
}

function storedOverlaySettings(): OverlaySettings {
  try {
    const stored = localStorage.getItem("prollyglot.overlay");
    return stored
      ? { ...DEFAULT_OVERLAY_SETTINGS, ...(JSON.parse(stored) as Partial<OverlaySettings>) }
      : { ...DEFAULT_OVERLAY_SETTINGS };
  } catch {
    return { ...DEFAULT_OVERLAY_SETTINGS };
  }
}

function populateSources(nextSnapshot: SourceSnapshot) {
  snapshot = nextSnapshot;
  const previousSource = sourceSelect.value;
  const previousDevice = deviceSelect.value;

  sourceSelect.replaceChildren(option("system", "Everything I hear"));
  for (const application of snapshot.applications) {
    sourceSelect.append(option(`application:${application.processId}`, `Only ${application.name}`));
  }
  if ([...sourceSelect.options].some(({ value }) => value === previousSource)) {
    sourceSelect.value = previousSource;
  }

  const defaultDevice = snapshot.playbackDevices.find(({ isDefault }) => isDefault);
  const followLabel = defaultDevice
    ? `Follow system default — ${defaultDevice.name}`
    : "Follow system default";
  deviceSelect.replaceChildren(
    option(FOLLOW_SYSTEM_DEFAULT, followLabel, !previousDevice || previousDevice === FOLLOW_SYSTEM_DEFAULT)
  );
  for (const device of snapshot.playbackDevices) {
    const label = device.isDefault ? `${device.name} — Pin current default` : device.name;
    deviceSelect.append(option(device.id, label, device.id === previousDevice));
  }
  if (![...deviceSelect.options].some(({ value }) => value === previousDevice)) {
    deviceSelect.value = FOLLOW_SYSTEM_DEFAULT;
  }
  updateSourceMode();
}

function updateSourceMode() {
  const system = sourceSelect.value === "system";
  deviceField.hidden = !system;
}

function selectedCapture(): CaptureSelection {
  if (sourceSelect.value === "system") {
    if (!deviceSelect.value) throw new Error("No playback device is available.");
    if (deviceSelect.value === FOLLOW_SYSTEM_DEFAULT) return { kind: "systemDefault" };
    return { kind: "systemOutput", deviceId: deviceSelect.value };
  }
  const processId = Number(sourceSelect.value.split(":")[1]);
  if (!Number.isInteger(processId)) throw new Error("The selected application is unavailable.");
  return { kind: "application", processId };
}

function languageLabel(language: string): string {
  return LANGUAGE_LABELS[language] ?? language;
}

function selectedTranslationModel(
  catalog = currentTranslations,
  language = spokenLanguage.value
): TranslationModelStatus | undefined {
  const sourceLanguage = supportedSourceLanguage(language);
  return sourceLanguage
    ? catalog.models.find((model) => model.sourceLanguage === sourceLanguage)
    : undefined;
}

function translationRequested(): boolean {
  return captionOutput.outputMode() !== "original";
}

function renderCaptionOutputControl(): void {
  const sourceLanguage = supportedSourceLanguage(spokenLanguage.value);
  const allowedModes: Array<[CaptionOutputMode, string]> = sourceLanguage
    ? [
        ["original", "Original only · translation off"],
        ["english", "English only · translated"],
        ["both", "Original + English"]
      ]
    : [["original", spokenLanguage.value === "en" ? "English (original)" : "Original language"]];
  const selectedMode = sourceLanguage ? preferredCaptionMode : "original";
  captionLanguage.replaceChildren(
    ...allowedModes.map(([value, label]) => option(value, label, value === selectedMode))
  );
  captionLanguage.value = selectedMode;
  captionLanguage.disabled = !sourceLanguage;
  if (captionOutput.outputMode() !== selectedMode) captionOutput.setOutputMode(selectedMode);

  const help = requireElement<HTMLElement>("#caption-language-help");
  if (spokenLanguage.value === "en") {
    help.textContent = "The recognized speech is already English.";
  } else if (spokenLanguage.value === "auto") {
    help.textContent = "Choose Japanese or Spanish to translate reliably; Automatic does not yet report each detected language.";
  } else {
    const model = selectedTranslationModel();
    if (!translationRequested()) {
      help.textContent = model?.phase === "ready"
        ? "Translation is off. Choose English only or Original + English to use the installed translator."
        : "Translation is off. Choose English only or Original + English to install and use a translator.";
    } else if (model?.phase === "ready") {
      help.textContent = "English is on and generated locally after each original caption is finalized.";
    } else if (model?.phase === "loading") {
      help.textContent = "Original captions stay live while the local English translator loads.";
    } else {
      help.textContent = "Original captions stay live until the optional English translator is installed.";
    }
  }
  renderTranslationSetup();
  prepareSelectedTranslator();
}

function prepareSelectedTranslator(): void {
  const sourceLanguage = supportedSourceLanguage(spokenLanguage.value);
  const model = selectedTranslationModel();
  if (!sourceLanguage || !translationRequested() || model?.phase !== "ready") return;
  void translationService.prepare(sourceLanguage).catch((error) => {
    const message = error instanceof Error ? error.message : String(error);
    captureMessage.textContent = `English translator could not start: ${message}`;
    void reportFrontendDiagnostic("translation-model", `${languageLabel(sourceLanguage)} preload: ${message}`);
  });
}

function renderTranslationSetup(): void {
  const model = selectedTranslationModel();
  if (!model || !translationRequested() || model.phase === "ready") {
    translationSetup.hidden = true;
    return;
  }

  translationSetup.hidden = false;
  translationSetupTitle.textContent = `${languageLabel(model.sourceLanguage)} to English`;
  translationProgress.hidden = model.phase !== "downloading";
  translationProgress.max = Math.max(model.totalBytes, 1);
  translationProgress.value = Math.min(model.downloadedBytes, translationProgress.max);
  const modelChangesBlocked = currentStatus.state !== "stopped" && currentStatus.state !== "failed";
  translationAction.disabled = model.phase === "checking"
    || model.phase === "downloading"
    || model.phase === "loading"
    || modelChangesBlocked;

  if (model.phase === "checking") {
    translationAction.textContent = "Checking local files…";
    translationMessage.textContent = model.message ?? "Checking the local translation model…";
  } else if (model.phase === "downloading") {
    const percent = model.totalBytes > 0
      ? Math.round((model.downloadedBytes / model.totalBytes) * 100)
      : 0;
    translationAction.textContent = `Downloading ${percent}%`;
    translationMessage.textContent = model.message ?? "Downloading and verifying the translator…";
  } else if (model.phase === "loading") {
    translationAction.textContent = "Loading translator…";
    translationMessage.textContent = "Original captions remain immediate while English translation starts.";
  } else {
    translationAction.textContent = model.phase === "corrupt"
      ? "Repair translator"
      : model.phase === "failed" ? "Retry translator" : `Download · ${formatBytes(model.totalBytes)}`;
    const baseMessage = model.message
      ?? `Download ${model.displayName} once. Captions and translation then stay on this PC.`;
    translationMessage.textContent = modelChangesBlocked
      ? `${baseMessage} Stop captions before changing translation models.`
      : baseMessage;
  }
}

function renderTranslationStatus(catalog: TranslationCatalogStatus): void {
  const newlyFailed = catalog.models.find((model) => {
    if (model.phase !== "failed" && model.phase !== "corrupt") return false;
    const previous = currentTranslations.models.find(({ modelId }) => modelId === model.modelId);
    return previous?.phase !== "failed" && previous?.phase !== "corrupt";
  });
  const completed = catalog.models.find((model) =>
    model.phase === "ready"
    && currentTranslations.models.find(({ modelId }) => modelId === model.modelId)?.phase === "downloading"
  );
  const failed = catalog.models.find((model) =>
    (model.phase === "failed" || model.phase === "corrupt")
    && currentTranslations.models.find(({ modelId }) => modelId === model.modelId)?.phase === "downloading"
  );
  if (completed) {
    settingsNotice = {
      message: `${completed.displayName} is installed. Choose ${languageLabel(completed.sourceLanguage)} and Original + English under Caption output to use it.`,
      tone: "success"
    };
  } else if (failed) {
    settingsNotice = {
      message: failed.message ?? `${failed.displayName} could not be installed.`,
      tone: "error"
    };
  }
  if (newlyFailed) {
    void reportFrontendDiagnostic(
      "translation-model",
      `${newlyFailed.displayName}: ${newlyFailed.message ?? newlyFailed.phase}`
    );
  }
  currentTranslations = catalog;
  renderCaptionOutputControl();
  if (dialog.open && dialog.dataset.panel === "settings") renderSettingsPanel();
}

function renderLanguageGuidance() {
  const help = requireElement<HTMLElement>("#spoken-language-help");
  help.textContent = spokenLanguage.value === "auto"
    ? "For mixed-language audio. Detection can add delay or choose the wrong language."
    : "Choosing the language guides recognition and usually improves accuracy.";
}

function captionAction(language: string): string {
  return language === "auto"
    ? "detect and caption the spoken language"
    : `caption ${languageLabel(language)} speech`;
}

function modelSupportsLanguage(model: ModelStatus, language = spokenLanguage.value): boolean {
  return model.languages.includes(language);
}

function modelLanguageSummary(model: ModelStatus): string {
  const explicit = model.languages.filter((language) => language !== "auto");
  const labels = explicit.map(languageLabel);
  if (model.languages.includes("auto")) labels.push("Auto detect");
  return labels.join(" · ");
}

function renderStatus(status: CaptureStatus) {
  const stateChanged = currentStatus.state !== status.state;
  currentStatus = status;
  statusLabel.dataset.state = status.state;
  const labels: Record<CaptureStatus["state"], string> = {
    starting: "Starting",
    capturing: "Live",
    waiting: "Waiting",
    stopping: "Stopping",
    stopped: "Ready",
    failed: "Error"
  };
  statusText.textContent = labels[status.state];
  captureMessage.textContent = status.message ?? "";
  captureToggle.textContent = status.state === "capturing" || status.state === "waiting" ? "Stop Captions" : "Start Captions";
  captureToggle.classList.toggle("stop", status.state === "capturing" || status.state === "waiting");
  updatePrimaryAvailability();
  renderTranslationSetup();
  document.documentElement.style.setProperty("--audio-peak", String(status.peak));
  if (stateChanged && dialog.open && dialog.dataset.panel === "settings") renderSettingsPanel();
}

function updatePrimaryAvailability() {
  const transitioning = currentStatus.state === "starting" || currentStatus.state === "stopping";
  const running = currentStatus.state === "capturing" || currentStatus.state === "waiting";
  const model = selectedModel();
  captureToggle.disabled = transitioning
    || (!running && (model.phase !== "ready" || !modelSupportsLanguage(model)));
  spokenLanguage.disabled = transitioning || running;
}

function formatBytes(bytes: number): string {
  if (bytes <= 0) return "Unknown size";
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function selectedModel(catalog = currentModels): ModelStatus {
  return catalog.models.find(({ modelId }) => modelId === catalog.selectedModelId)
    ?? catalog.models[0]
    ?? FALLBACK_MODEL;
}

function renderModelStatus(catalog: ModelCatalogStatus) {
  const completed = catalog.models.find((model) =>
    model.phase === "ready"
    && currentModels.models.find(({ modelId }) => modelId === model.modelId)?.phase === "downloading"
  );
  const failed = catalog.models.find((model) =>
    model.phase === "failed"
    && currentModels.models.find(({ modelId }) => modelId === model.modelId)?.phase === "downloading"
  );
  if (completed) {
    settingsNotice = {
      message: `${completed.displayName} is installed and ready to use.`,
      tone: "success"
    };
  } else if (failed) {
    settingsNotice = {
      message: failed.message ?? `${failed.displayName} could not be installed.`,
      tone: "error"
    };
  }
  currentModels = catalog;
  const status = selectedModel(catalog);
  const compatible = modelSupportsLanguage(status);
  const ready = status.phase === "ready" && compatible;
  modelSetup.hidden = ready;
  modelSetupTitle.textContent = spokenLanguage.value === "auto"
    ? "Automatic language detection"
    : `${languageLabel(spokenLanguage.value)} captions`;
  modelProgress.hidden = status.phase !== "downloading";
  modelProgress.max = Math.max(status.totalBytes, 1);
  modelProgress.value = Math.min(status.downloadedBytes, modelProgress.max);
  modelAction.disabled = status.phase === "checking" || status.phase === "downloading" || !compatible;

  if (!compatible) {
    modelAction.textContent = "Choose compatible model";
    modelMessage.textContent = `${status.displayName} does not support ${languageLabel(spokenLanguage.value)}.`;
  } else if (status.phase === "checking") {
    modelAction.textContent = "Checking local models…";
    modelMessage.textContent = status.message ?? "Checking installed speech models without delaying the app window…";
  } else if (status.phase === "downloading") {
    const percent = status.totalBytes > 0
      ? Math.round((status.downloadedBytes / status.totalBytes) * 100)
      : 0;
    modelAction.textContent = `Downloading ${percent}%`;
    modelMessage.textContent = status.message ?? "Downloading and verifying the model…";
  } else if (status.phase === "corrupt") {
    modelAction.textContent = "Repair model";
    modelMessage.textContent = status.message ?? "The local model needs to be downloaded again.";
  } else if (status.phase === "failed") {
    modelAction.textContent = "Retry download";
    modelMessage.textContent = status.message ?? "The model could not be installed.";
  } else {
    modelAction.textContent = `Download · ${formatBytes(status.totalBytes)}`;
    modelMessage.textContent = `Download ${status.displayName} once, then ${captionAction(spokenLanguage.value)} offline.`;
  }
  updatePrimaryAvailability();
  if (dialog.open && dialog.dataset.panel === "settings") renderSettingsPanel();
}

function renderTranscript(snapshot: TranscriptSnapshot) {
  currentTranscript = snapshot;
  captionOutput.updateTranscript(snapshot);
  if (dialog.open && dialog.dataset.panel === "transcript") renderTranscriptPanel();
}

type SourceRefreshResult =
  | { ok: true; snapshot: SourceSnapshot }
  | { ok: false; message: string };

async function refreshSources(): Promise<SourceRefreshResult> {
  captureMessage.textContent = "";
  try {
    const nextSnapshot = await sourceSnapshot();
    populateSources(nextSnapshot);
    return { ok: true, snapshot: nextSnapshot };
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    captureMessage.textContent = message;
    return { ok: false, message };
  }
}

function dialogContent(): HTMLElement {
  return requireElement<HTMLElement>("#dialog-content");
}

function formatTimestamp(micros: number): string {
  const seconds = Math.max(0, Math.floor(micros / 1_000_000));
  const minutes = Math.floor(seconds / 60);
  return `${String(minutes).padStart(2, "0")}:${String(seconds % 60).padStart(2, "0")}`;
}

function appendTranscriptCaption(
  item: HTMLElement,
  segment: TranscriptSegment,
  provisional = false
): void {
  const copy = document.createElement("span");
  copy.className = "transcript-copy";
  const original = document.createElement("span");
  original.className = "transcript-text transcript-original";
  original.lang = segment.sourceLanguage === "auto" ? "" : segment.sourceLanguage;
  original.textContent = segment.text;
  const mode = captionOutput.outputMode();
  const translated = captionOutput.translationFor(segment);

  if (mode === "original" || provisional) {
    copy.append(original);
    if (provisional && mode !== "original") {
      const note = document.createElement("span");
      note.className = "transcript-translation-state";
      note.textContent = "English follows when this caption is finalized.";
      copy.append(note);
    }
    item.append(copy);
    return;
  }

  const english = document.createElement("span");
  english.className = "transcript-text transcript-translation";
  english.lang = "en";
  if (translated?.phase === "ready") english.textContent = translated.text;

  if (mode === "both") copy.append(original);
  if (translated?.phase === "ready") {
    copy.append(english);
  } else {
    if (mode === "english") {
      original.classList.add("translation-fallback");
      copy.append(original);
    }
    const note = document.createElement("span");
    note.className = "transcript-translation-state";
    note.textContent = translated?.phase === "failed"
      ? "English unavailable · showing original"
      : captionOutput.isTranslationPending(segment)
        ? "Translating to English…"
        : "English translator is not ready";
    copy.append(note);
  }
  item.append(copy);
}

function renderTranscriptPanel(forceLatest = false) {
  const content = dialogContent();
  const previousList = content.querySelector<HTMLOListElement>(".transcript-list");
  const previousScrollTop = previousList?.scrollTop ?? 0;
  const previousDistanceFromBottom = previousList
    ? previousList.scrollHeight - previousList.clientHeight - previousList.scrollTop
    : 0;
  const shouldFollowLatest = forceLatest
    || !previousList
    || transcriptFollowLatest
    || previousDistanceFromBottom <= TRANSCRIPT_BOTTOM_THRESHOLD;
  content.replaceChildren();

  const toolbar = document.createElement("div");
  toolbar.className = "dialog-toolbar";
  const summary = document.createElement("span");
  summary.className = "dialog-summary";
  summary.textContent = `${currentTranscript.committed.length} finalized ${currentTranscript.committed.length === 1 ? "caption" : "captions"}`;
  const actions = document.createElement("div");
  actions.className = "dialog-toolbar-actions";
  const latest = document.createElement("button");
  latest.type = "button";
  latest.className = "text-button";
  latest.textContent = "Latest";
  latest.hidden = shouldFollowLatest;
  const clear = document.createElement("button");
  clear.type = "button";
  clear.className = "text-button";
  clear.textContent = "Clear";
  clear.disabled = currentTranscript.committed.length === 0 && !currentTranscript.provisional;
  clear.addEventListener("click", async () => {
    try {
      await clearTranscript();
    } catch (error) {
      captureMessage.textContent = error instanceof Error ? error.message : String(error);
    }
  });
  actions.append(latest, clear);
  toolbar.append(summary, actions);
  content.append(toolbar);

  if (currentTranscript.committed.length === 0 && !currentTranscript.provisional) {
    transcriptFollowLatest = true;
    const empty = document.createElement("p");
    empty.className = "empty-copy";
    empty.textContent = "Finalized captions from this session will appear here.";
    content.append(empty);
    return;
  }

  const list = document.createElement("ol");
  list.className = "transcript-list";
  list.setAttribute("aria-label", "Session transcript");
  for (const segment of currentTranscript.committed) {
    const item = document.createElement("li");
    item.className = "transcript-segment";
    const timestamp = document.createElement("time");
    timestamp.textContent = formatTimestamp(segment.startMicros);
    item.append(timestamp);
    appendTranscriptCaption(item, segment);
    list.append(item);
  }
  if (currentTranscript.provisional) {
    const item = document.createElement("li");
    item.className = "transcript-segment provisional";
    const timestamp = document.createElement("time");
    timestamp.textContent = "Live";
    item.append(timestamp);
    appendTranscriptCaption(item, currentTranscript.provisional, true);
    list.append(item);
  }
  content.append(list);

  const updateFollowState = () => {
    const distanceFromBottom = list.scrollHeight - list.clientHeight - list.scrollTop;
    transcriptFollowLatest = distanceFromBottom <= TRANSCRIPT_BOTTOM_THRESHOLD;
    latest.hidden = transcriptFollowLatest;
  };
  list.addEventListener("scroll", updateFollowState, { passive: true });
  latest.addEventListener("click", () => {
    transcriptFollowLatest = true;
    list.scrollTop = list.scrollHeight;
    latest.hidden = true;
  });

  requestAnimationFrame(() => {
    if (shouldFollowLatest) {
      transcriptFollowLatest = true;
      list.scrollTop = list.scrollHeight;
      latest.hidden = true;
    } else {
      transcriptFollowLatest = false;
      list.scrollTop = Math.min(previousScrollTop, list.scrollHeight - list.clientHeight);
      latest.hidden = false;
    }
  });
}

function renderSettingsPanel() {
  const content = dialogContent();
  content.innerHTML = `
    <section class="settings-section" aria-labelledby="model-settings-title">
      <span class="model-kicker">Local speech models</span>
      <h3 id="model-settings-title">Recognition model</h3>
      <p class="settings-copy">Choose a speed, language, and quality tradeoff. Every option streams locally; selections apply to the next caption session.</p>
      <div id="model-options" class="model-options"></div>
      <p id="settings-action-status" class="settings-action-status" role="status" aria-live="polite"></p>
    </section>
    <section class="settings-section settings-section-divided" aria-labelledby="translation-settings-title">
      <span class="model-kicker">Optional local translation</span>
      <h3 id="translation-settings-title">English translation models</h3>
      <p class="settings-copy">Installing stores a translator locally but does not turn it on. In the main window, choose Japanese or Spanish under Spoken language, then choose English only or Original + English under Caption output.</p>
      <div id="translation-model-options" class="model-options"></div>
    </section>
    <section class="settings-section settings-section-divided" aria-labelledby="audio-settings-title">
      <span class="model-kicker">Audio</span>
      <h3 id="audio-settings-title">Available sources</h3>
      <p class="settings-copy">Refresh after opening or closing an audio-producing application or changing playback devices.</p>
      <button class="secondary-button settings-wide-action" id="refresh-sources" type="button">${icons.refresh}<span>Refresh audio sources</span></button>
    </section>
  `;

  const options = requireElement<HTMLElement>("#model-options");
  const modelChangesBlocked = currentStatus.state !== "stopped" && currentStatus.state !== "failed";
  const anotherDownloadRunning = currentModels.models.some(({ phase }) => phase === "downloading");

  for (const model of currentModels.models) {
    const selected = model.modelId === currentModels.selectedModelId;
    const compatible = modelSupportsLanguage(model);
    const card = document.createElement("article");
    card.className = "model-option";
    card.dataset.selected = String(selected);

    const heading = document.createElement("div");
    heading.className = "model-option-heading";
    const names = document.createElement("div");
    const profile = document.createElement("span");
    profile.className = "model-profile";
    profile.textContent = model.profile;
    const name = document.createElement("h4");
    name.textContent = model.displayName;
    names.append(profile, name);

    const badge = document.createElement("span");
    badge.className = "model-state-badge";
    badge.dataset.phase = model.phase;
    badge.textContent = model.phase === "checking"
      ? "Checking"
      : selected
      ? model.phase === "ready" ? "In use" : "Selected"
      : model.phase === "ready" ? "Installed" : "Optional";
    heading.append(names, badge);

    const description = document.createElement("p");
    description.className = "model-option-description";
    description.textContent = model.description;
    const metadata = document.createElement("p");
    metadata.className = "model-option-metadata";
    metadata.textContent = `${modelLanguageSummary(model)} · ${formatBytes(model.totalBytes)} · CPU`;
    card.append(heading, description, metadata);

    if (model.phase === "downloading") {
      const progress = document.createElement("progress");
      progress.className = "model-progress model-option-progress";
      progress.max = Math.max(model.totalBytes, 1);
      progress.value = Math.min(model.downloadedBytes, progress.max);
      card.append(progress);
    }
    if (model.message && model.phase !== "ready") {
      const message = document.createElement("p");
      message.className = "model-option-message";
      message.dataset.tone = model.phase === "failed" || model.phase === "corrupt" ? "error" : "neutral";
      message.textContent = model.message;
      card.append(message);
    }

    const actions = document.createElement("div");
    actions.className = "model-option-actions";
    const primary = document.createElement("button");
    primary.type = "button";
    primary.className = "secondary-button model-option-action";

    if (model.phase === "ready") {
      primary.textContent = selected
        ? "In use"
        : compatible ? "Use model" : `${modelLanguageSummary(model)} only`;
      primary.disabled = selected || modelChangesBlocked || !compatible;
      if (!selected && compatible) {
        primary.addEventListener("click", async () => {
          primary.disabled = true;
          setSettingsNotice(`Selecting ${model.displayName}…`, "neutral");
          try {
            await selectSpeechModel(model.modelId);
            settingsNotice = {
              message: `${model.displayName} will be used for the next caption session.`,
              tone: "success"
            };
            renderModelStatus(await modelStatus());
          } catch (error) {
            setSettingsNotice(error instanceof Error ? error.message : String(error), "error");
            primary.disabled = false;
          }
        });
      }
    } else if (model.phase === "checking") {
      primary.textContent = "Checking…";
      primary.disabled = true;
    } else if (model.phase === "downloading") {
      const percent = model.totalBytes > 0
        ? Math.round((model.downloadedBytes / model.totalBytes) * 100)
        : 0;
      primary.textContent = `Downloading ${percent}%`;
      primary.disabled = true;
    } else {
      primary.textContent = model.phase === "corrupt"
        ? "Repair"
        : model.phase === "failed" ? "Retry" : "Download";
      primary.disabled = modelChangesBlocked || anotherDownloadRunning;
      primary.addEventListener("click", async () => {
        primary.disabled = true;
        setSettingsNotice(`Starting ${model.displayName} download…`, "neutral");
        try {
          await installSpeechModel(model.modelId);
        } catch (error) {
          setSettingsNotice(error instanceof Error ? error.message : String(error), "error");
          primary.disabled = false;
        }
      });
    }
    actions.append(primary);

    if (model.phase === "ready") {
      const remove = document.createElement("button");
      remove.type = "button";
      remove.className = "text-button danger-text";
      remove.textContent = "Remove";
      remove.disabled = modelChangesBlocked || anotherDownloadRunning;
      remove.addEventListener("click", async () => {
        remove.disabled = true;
        setSettingsNotice(`Removing ${model.displayName}…`, "neutral");
        try {
          await removeSpeechModel(model.modelId);
          settingsNotice = {
            message: `${model.displayName} was removed from this PC.`,
            tone: "success"
          };
          renderModelStatus(await modelStatus());
        } catch (error) {
          setSettingsNotice(error instanceof Error ? error.message : String(error), "error");
          remove.disabled = false;
        }
      });
      actions.append(remove);
    }
    card.append(actions);
    options.append(card);
  }

  const translationOptions = requireElement<HTMLElement>("#translation-model-options");
  const translationChangesBlocked = currentStatus.state !== "stopped" && currentStatus.state !== "failed";
  const anotherTranslationDownload = currentTranslations.models.some(({ phase }) => phase === "downloading");
  for (const model of currentTranslations.models) {
    const card = document.createElement("article");
    card.className = "model-option";
    card.dataset.selected = String(
      spokenLanguage.value === model.sourceLanguage && captionOutput.outputMode() !== "original"
    );

    const heading = document.createElement("div");
    heading.className = "model-option-heading";
    const names = document.createElement("div");
    const profile = document.createElement("span");
    profile.className = "model-profile";
    profile.textContent = `${languageLabel(model.sourceLanguage)} → English`;
    const name = document.createElement("h4");
    name.textContent = model.displayName;
    names.append(profile, name);
    const badge = document.createElement("span");
    badge.className = "model-state-badge";
    badge.dataset.phase = model.phase;
    badge.textContent = model.phase === "ready"
      ? "Installed"
      : model.phase === "loading" ? "Loading" : model.phase === "downloading" ? "Downloading" : "Optional";
    heading.append(names, badge);

    const description = document.createElement("p");
    description.className = "model-option-description";
    description.textContent = `Translates finalized ${languageLabel(model.sourceLanguage)} captions to English locally.`;
    const metadata = document.createElement("p");
    metadata.className = "model-option-metadata";
    metadata.textContent = `${formatBytes(model.totalBytes)} · CPU · Apache-2.0`;
    card.append(heading, description, metadata);

    if (model.phase === "downloading") {
      const progress = document.createElement("progress");
      progress.className = "model-progress model-option-progress";
      progress.max = Math.max(model.totalBytes, 1);
      progress.value = Math.min(model.downloadedBytes, progress.max);
      card.append(progress);
    }
    if (model.message && model.phase !== "ready") {
      const message = document.createElement("p");
      message.className = "model-option-message";
      message.dataset.tone = model.phase === "failed" || model.phase === "corrupt" ? "error" : "neutral";
      message.textContent = model.message;
      card.append(message);
    }

    const actions = document.createElement("div");
    actions.className = "model-option-actions";
    const primary = document.createElement("button");
    primary.type = "button";
    primary.className = "secondary-button model-option-action";
    if (model.phase === "ready") {
      primary.textContent = "Installed";
      primary.disabled = true;
    } else if (model.phase === "checking") {
      primary.textContent = "Checking…";
      primary.disabled = true;
    } else if (model.phase === "loading") {
      primary.textContent = "Loading…";
      primary.disabled = true;
    } else if (model.phase === "downloading") {
      const percent = model.totalBytes > 0
        ? Math.round((model.downloadedBytes / model.totalBytes) * 100)
        : 0;
      primary.textContent = `Downloading ${percent}%`;
      primary.disabled = true;
    } else {
      primary.textContent = model.phase === "corrupt"
        ? "Repair"
        : model.phase === "failed" ? "Retry" : "Download";
      primary.disabled = translationChangesBlocked || anotherTranslationDownload;
      primary.addEventListener("click", async () => {
        primary.disabled = true;
        setSettingsNotice(`Starting ${model.displayName} download…`, "neutral");
        try {
          await translationService.install(model.sourceLanguage);
        } catch (error) {
          setSettingsNotice(error instanceof Error ? error.message : String(error), "error");
          primary.disabled = false;
        }
      });
    }
    actions.append(primary);

    if (model.phase === "ready") {
      const remove = document.createElement("button");
      remove.type = "button";
      remove.className = "text-button danger-text";
      remove.textContent = "Remove";
      remove.disabled = translationChangesBlocked || anotherTranslationDownload;
      remove.addEventListener("click", async () => {
        remove.disabled = true;
        setSettingsNotice(`Removing ${model.displayName}…`, "neutral");
        try {
          await translationService.remove(model.sourceLanguage);
          setSettingsNotice(`${model.displayName} was removed from this PC.`, "success");
        } catch (error) {
          setSettingsNotice(error instanceof Error ? error.message : String(error), "error");
          remove.disabled = false;
        }
      });
      actions.append(remove);
    }
    card.append(actions);
    translationOptions.append(card);
  }
  renderSettingsNotice();

  const refresh = requireElement<HTMLButtonElement>("#refresh-sources");
  refresh.addEventListener("click", async () => {
    refresh.disabled = true;
    setSettingsNotice("Refreshing audio sources…", "neutral");
    const result = await refreshSources();
    if (result.ok) {
      const deviceCount = result.snapshot.playbackDevices.length;
      const applicationCount = result.snapshot.applications.length;
      setSettingsNotice(
        `Found ${deviceCount} playback ${deviceCount === 1 ? "device" : "devices"} and ${applicationCount} ${applicationCount === 1 ? "application" : "applications"}.`,
        "success"
      );
    } else {
      setSettingsNotice(result.message, "error");
    }
    refresh.disabled = false;
  });

}

function renderSettingsNotice() {
  const status = document.querySelector<HTMLElement>("#settings-action-status");
  if (!status) return;
  status.textContent = settingsNotice?.message ?? "";
  status.dataset.tone = settingsNotice?.tone ?? "neutral";
}

function setSettingsNotice(message: string, tone: "neutral" | "success" | "error") {
  settingsNotice = { message, tone };
  renderSettingsNotice();
}

async function updateSpokenLanguage() {
  const language = spokenLanguage.value;
  renderLanguageGuidance();
  renderCaptionOutputControl();
  captureMessage.textContent = "";
  const current = selectedModel();
  if (modelSupportsLanguage(current, language)) {
    acceptedSpokenLanguage = language;
    renderModelStatus(currentModels);
    return;
  }

  const candidates = currentModels.models.filter((model) => modelSupportsLanguage(model, language));
  const candidate = candidates.find(({ phase }) => phase === "ready") ?? candidates[0];
  if (!candidate) {
    spokenLanguage.value = acceptedSpokenLanguage;
    captureMessage.textContent = `No installed model catalog supports ${languageLabel(language)}.`;
    renderCaptionOutputControl();
    renderModelStatus(currentModels);
    return;
  }

  try {
    await selectSpeechModel(candidate.modelId);
    acceptedSpokenLanguage = language;
    renderModelStatus(await modelStatus());
  } catch (error) {
    spokenLanguage.value = acceptedSpokenLanguage;
    captureMessage.textContent = error instanceof Error ? error.message : String(error);
    renderCaptionOutputControl();
    renderModelStatus(currentModels);
  }
}

sourceSelect.addEventListener("change", updateSourceMode);
spokenLanguage.addEventListener("change", () => void updateSpokenLanguage());
captionLanguage.addEventListener("change", () => {
  const mode = captionLanguage.value as CaptionOutputMode;
  if (mode !== "original" && mode !== "english" && mode !== "both") return;
  preferredCaptionMode = mode;
  localStorage.setItem(CAPTION_MODE_STORAGE_KEY, mode);
  captionOutput.setOutputMode(mode);
  renderCaptionOutputControl();
  if (dialog.open && dialog.dataset.panel === "transcript") renderTranscriptPanel();
});
modelAction.addEventListener("click", async () => {
  captureMessage.textContent = "";
  try {
    await installSpeechModel(selectedModel().modelId);
  } catch (error) {
    captureMessage.textContent = error instanceof Error ? error.message : String(error);
  }
});
translationAction.addEventListener("click", async () => {
  captureMessage.textContent = "";
  const model = selectedTranslationModel();
  if (!model) return;
  try {
    await translationService.install(model.sourceLanguage);
  } catch (error) {
    captureMessage.textContent = error instanceof Error ? error.message : String(error);
  }
});
captureToggle.addEventListener("click", async () => {
  captureMessage.textContent = "";
  try {
    if (currentStatus.state === "capturing" || currentStatus.state === "waiting") {
      await stopCapture();
    } else {
      const model = selectedModel();
      if (model.phase !== "ready") throw new Error("Install the selected speech model first.");
      if (!modelSupportsLanguage(model)) {
        throw new Error(`${model.displayName} does not support ${languageLabel(spokenLanguage.value)}.`);
      }
      await startCapture(selectedCapture(), spokenLanguage.value);
    }
  } catch (error) {
    renderStatus({
      state: "failed",
      peak: 0,
      droppedFrames: currentStatus.droppedFrames,
      message: error instanceof Error ? error.message : String(error)
    });
  }
});

requireElement<HTMLButtonElement>("#appearance-action").addEventListener("click", () => void showAppearance());

function reportWindowControlError(action: string, error: unknown): void {
  const message = error instanceof Error ? error.message : String(error);
  captureMessage.textContent = `Window ${action} failed: ${message}`;
  void reportFrontendDiagnostic("window-control", `${action}: ${message}`);
}

requireElement<HTMLElement>(".titlebar").addEventListener("mousedown", (event) => {
  if (event.button !== 0) return;
  const target = event.target;
  if (target instanceof Element && target.closest("button, input, select, a")) return;
  const operation = event.detail === 2 ? windowAction("maximize") : startWindowDrag();
  void operation.catch((error) => reportWindowControlError(event.detail === 2 ? "maximize" : "drag", error));
});

for (const button of document.querySelectorAll<HTMLButtonElement>("[data-window-action]")) {
  button.addEventListener("click", () => {
    const action = button.dataset.windowAction;
    if (action === "minimize" || action === "maximize" || action === "close") {
      void windowAction(action).catch((error) => reportWindowControlError(action, error));
    }
  });
}

for (const button of document.querySelectorAll<HTMLButtonElement>("[data-panel]")) {
  button.addEventListener("click", () => {
    const panel = button.dataset.panel === "transcript" ? "transcript" : "settings";
    const title = panel === "transcript" ? "Transcript" : "Settings";
    dialog.dataset.panel = panel;
    requireElement<HTMLElement>("#dialog-title").textContent = title;
    if (panel === "settings") {
      settingsNotice = undefined;
      renderSettingsPanel();
    }
    dialog.showModal();
    if (panel === "transcript") renderTranscriptPanel(true);
  });
}

requireElement<HTMLButtonElement>(".dialog-close").addEventListener("click", () => dialog.close());
dialog.addEventListener("click", (event) => {
  if (event.target === dialog) dialog.close();
});

translationService.subscribe(renderTranslationStatus);
renderLanguageGuidance();
renderCaptionOutputControl();
void Promise.all([
  updateOverlaySettings(storedOverlaySettings()).catch((error) => {
    const message = error instanceof Error ? error.message : String(error);
    void reportFrontendDiagnostic("overlay-settings", `startup restore: ${message}`);
  }),
  refreshSources(),
  captureStatus().then(renderStatus),
  modelStatus().then(renderModelStatus),
  translationService.initialize().then(renderTranslationStatus).catch((error) => {
    const message = error instanceof Error ? error.message : String(error);
    captureMessage.textContent = message;
    void reportFrontendDiagnostic("translation-model", message);
  }),
  transcriptSnapshot().then(renderTranscript),
  onCaptureStatus(renderStatus),
  onModelStatus(renderModelStatus),
  onTranscriptUpdate(renderTranscript)
]);
