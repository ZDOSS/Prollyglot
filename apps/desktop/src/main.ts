import "./styles.css";

import { desktopBridge, isTauri } from "./bridge";
import { CaptionOutputController, supportedSourceLanguage } from "./caption-output";
import { AppearancePanel } from "./appearance-panel";
import {
  initializeConfiguration,
  type ConfigurationMutation
} from "./configuration";
import {
  AppStore,
  FALLBACK_SPEECH_MODEL,
  createInitialAppState,
  type AppDestination,
  type AppViewMode
} from "./app-store";
import { errorMessage, isApplicationError } from "./errors";
import { icons } from "./icons";
import {
  SPOKEN_LANGUAGES,
  languageLabel,
  supportedTranslationLanguage
} from "./language-catalog";
import { SettingsPanel, type SettingsNoticeTone } from "./settings";
import {
  acceptsVisualSessionEvent as acceptsVisualRuntimeEvent
} from "./runtime-state";
import { TranslationService, translationStatusForRoute } from "./translation";
import { VisualPanel } from "./visual-panel";
import { VisualTranslationController } from "./visual-translation";
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
  TranslationModelStatus,
  VisualModelCatalogStatus,
  RuntimeSnapshot,
  VisualSourceSnapshot,
  VisualStatus
} from "./types";
import { RUNTIME_CONTRACT_VERSION } from "./types";

const {
  captureStatus,
  clearTranscript,
  installSpeechModel,
  installVisualModel,
  modelStatus,
  onCaptureStatus,
  onModelStatus,
  onRuntimeState,
  onTranscriptUpdate,
  onVisualModelStatus,
  onVisualStatus,
  onVisualTextClear,
  onVisualTextUpdate,
  pickVisualRegion,
  reportFrontendDiagnostic,
  removeSpeechModel,
  removeVisualModel,
  runtimeBootstrap,
  selectSpeechModel,
  setWindowLayout,
  showAppearance,
  sourceSnapshot,
  startWindowDrag,
  startCapture,
  startVisualTranslation,
  stopCapture,
  stopVisualTranslation,
  transcriptSnapshot,
  updateCaptionPresentation,
  updateVisualPresentation,
  visualCapabilities,
  visualModelStatus,
  visualSourceSnapshot,
  visualStatus,
  windowAction
} = desktopBridge;

const root = document.querySelector<HTMLElement>("#app");
if (!root) throw new Error("missing app root");

root.innerHTML = `
  <section class="app-window main-window" aria-label="Prollyglot controls" data-view-mode="full">
    <header class="titlebar">
      <div class="brand">
        <img class="brand-mark" src="/branding/prollyglot-mark.png" alt="" />
        <span class="brand-name">Prollyglot</span>
        <span class="status-label" data-state="stopped"><span class="status-dot"></span><span id="status-text">Ready</span></span>
      </div>
      <div class="titlebar-actions">
        <button id="view-mode-toggle" class="view-mode-toggle" type="button">
          <span class="view-mode-icon">${icons.compact}</span>
          <span class="view-mode-label">Compact view</span>
        </button>
        <div class="window-controls" aria-label="Window controls">
          <button class="window-control" type="button" data-window-action="minimize" aria-label="Minimize">${icons.minimize}</button>
          <button class="window-control" type="button" data-window-action="maximize" aria-label="Maximize">${icons.maximize}</button>
          <button class="window-control close" type="button" data-window-action="close" aria-label="Close">${icons.close}</button>
        </div>
      </div>
    </header>

    <div class="desktop-frame">
      <nav class="desktop-nav" aria-label="Application views">
        <div class="desktop-nav-primary">
          <button type="button" class="desktop-nav-action is-active" data-workspace="captions" aria-current="page">${icons.captions}<span>Captions</span></button>
          <button type="button" class="desktop-nav-action" data-panel="visual">${icons.screen}<span>Screen translation</span></button>
          <button type="button" class="desktop-nav-action" data-panel="transcript">${icons.transcript}<span>Transcript</span></button>
          <button type="button" class="desktop-nav-action" data-panel="models">${icons.models}<span>Models</span></button>
          <button type="button" class="desktop-nav-action" data-panel="appearance">${icons.appearance}<span>Appearance</span></button>
          <button type="button" class="desktop-nav-action" data-panel="settings">${icons.settings}<span>Settings</span></button>
        </div>
        <div class="desktop-nav-footer">
          <span class="privacy-state"><span class="status-dot"></span>Local processing</span>
          <span class="version-state">Pre-release · ${__PROLLYGLOT_VERSION__}</span>
        </div>
      </nav>

      <main class="workspace">
        <section id="caption-workspace" class="workspace-page" aria-labelledby="caption-page-title">
          <header class="workspace-heading">
            <div>
              <h1 id="caption-page-title">Captions</h1>
              <p>Transcribe and translate audio playing on this PC.</p>
            </div>
          </header>

          <div class="caption-workspace-grid">
            <section class="main-content caption-setup" aria-label="Caption setup">
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
                  <h2 id="translation-setup-title">Translated captions</h2>
                  <p id="translation-message">Download the selected translator once, then translate offline.</p>
                </div>
                <progress id="translation-progress" class="model-progress" max="1" value="0" hidden></progress>
                <button id="translation-action" class="secondary-button model-action" type="button">Download translator</button>
              </section>

              <div class="capture-field-grid source-field-grid">
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
              </div>

              <div class="field-grid">
                <div class="field-group">
                  <label class="field-label" for="spoken-language">Spoken language</label>
                  <div class="select-wrap">
                    <select id="spoken-language" class="select-control" aria-describedby="spoken-language-help"></select>
                    ${icons.chevronDown}
                  </div>
                  <span id="spoken-language-help" class="field-help"></span>
                </div>

                <div class="field-group">
                  <label class="field-label" for="translation-target">Translate to</label>
                  <div class="select-wrap">
                    <select id="translation-target" class="select-control" aria-describedby="translation-target-help"></select>
                    ${icons.chevronDown}
                  </div>
                  <span id="translation-target-help" class="field-help"></span>
                </div>

                <div class="field-group caption-output-field">
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

              <div class="capture-actions">
                <button id="capture-toggle" class="primary-button" type="button">Start Captions</button>
                <button id="visual-toggle" class="secondary-button screen-translation-button" type="button">Translate Screen…</button>
              </div>
            </section>

            <aside class="session-preview" aria-labelledby="session-preview-title">
              <header class="session-preview-header">
                <div>
                  <span class="session-preview-kicker">Current session</span>
                  <h2 id="session-preview-title">Live transcript</h2>
                </div>
                <button type="button" class="text-button" data-panel="transcript">Open transcript</button>
              </header>
              <div id="session-preview-content" class="session-preview-content"></div>
            </aside>
          </div>
        </section>

        <dialog id="utility-dialog" class="utility-dialog" aria-labelledby="dialog-title">
          <div class="dialog-title-row">
            <div class="dialog-heading-copy">
              <h2 id="dialog-title"></h2>
              <p id="dialog-subtitle"></p>
            </div>
            <button type="button" class="dialog-close" aria-label="Close">${icons.close}</button>
          </div>
          <div id="dialog-content"></div>
          <p id="settings-action-status" class="settings-action-status" role="status" aria-live="polite" hidden></p>
        </dialog>
      </main>
    </div>

    <nav class="utility-nav compact-nav" aria-label="Compact application views">
      <button type="button" class="utility-action" data-panel="transcript">${icons.transcript}<span>Transcript</span></button>
      <button type="button" class="utility-action" data-appearance>${icons.appearance}<span>Appearance</span></button>
      <button type="button" class="utility-action" data-panel="models">${icons.models}<span>Models</span></button>
    </nav>
  </section>
`;

const sourceSelect = requireElement<HTMLSelectElement>("#audio-source");
const deviceSelect = requireElement<HTMLSelectElement>("#playback-device");
const deviceField = requireElement<HTMLElement>("#device-field");
const spokenLanguage = requireElement<HTMLSelectElement>("#spoken-language");
const translationTarget = requireElement<HTMLSelectElement>("#translation-target");
const captionLanguage = requireElement<HTMLSelectElement>("#caption-language");
const captureToggle = requireElement<HTMLButtonElement>("#capture-toggle");
const visualToggle = requireElement<HTMLButtonElement>("#visual-toggle");
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
const appWindow = requireElement<HTMLElement>(".main-window");
const viewModeToggle = requireElement<HTMLButtonElement>("#view-mode-toggle");
const sessionPreviewContent = requireElement<HTMLElement>("#session-preview-content");
const captionWorkspace = requireElement<HTMLElement>("#caption-workspace");
type DialogPanel = Exclude<AppDestination, "captions">;

const useMockTranslation = !isTauri()
  && !new URLSearchParams(window.location.search).has("realTranslation");
const translationService = new TranslationService(useMockTranslation);
const configuration = await initializeConfiguration(
  desktopBridge,
  localStorage,
  (message) => {
    void reportFrontendDiagnostic("configuration", message);
  }
);
const appStore = new AppStore(createInitialAppState({
  configuration: structuredClone(configuration.snapshot()),
  translations: translationService.snapshot()
}));
const appState = () => appStore.getState();
const FOLLOW_SYSTEM_DEFAULT = "__follow-system-default__";
const TRANSCRIPT_BOTTOM_THRESHOLD = 48;
const settingsPanel = new SettingsPanel();
const visualPanel = new VisualPanel(
  structuredClone(configuration.snapshot().config.visual),
  (preferences) => {
    void persistConfiguration((config) => {
      config.visual = structuredClone(preferences);
    });
  }
);
const appearancePanel = new AppearancePanel();
const captionOutput = new CaptionOutputController(
  translationService,
  (frame) => {
    if (dialog.open && dialog.dataset.panel === "transcript") renderTranscriptPanel();
    renderSessionPreview();
    return updateCaptionPresentation(frame);
  },
  (message) => {
    setCaptureNotice(message, "error");
    void reportFrontendDiagnostic("translation", message);
  },
  (message) => {
    void reportFrontendDiagnostic("translation-performance", message, "info");
  }
);
const visualTranslation = new VisualTranslationController(
  translationService,
  updateVisualPresentation,
  (message) => {
    void reportFrontendDiagnostic("visual-translation", message);
  },
  (message) => {
    void reportFrontendDiagnostic("visual-translation-performance", message, "info");
  }
);

configuration.subscribe((snapshot) => {
  const previous = appState();
  appStore.dispatch({ type: "configuration/accepted", snapshot: structuredClone(snapshot) });
  const next = appState();
  if (previous.navigation.viewMode !== next.navigation.viewMode) renderViewMode();
  if (
    previous.preferences.acceptedSpokenLanguage
      !== next.preferences.acceptedSpokenLanguage
    || previous.preferences.translationTarget !== next.preferences.translationTarget
    || previous.preferences.captionMode !== next.preferences.captionMode
  ) {
    spokenLanguage.value = next.preferences.acceptedSpokenLanguage;
    populateTranslationTargets();
    translationTarget.value = next.preferences.translationTarget;
    captionOutput.setOutputMode(next.preferences.captionMode);
    const target = supportedTranslationLanguage(next.preferences.translationTarget);
    if (target) captionOutput.setTranslationTarget(target);
    renderLanguageGuidance();
    renderCaptionOutputControl();
  }
});

function requireElement<T extends Element>(selector: string): T {
  const element = document.querySelector<T>(selector);
  if (!element) throw new Error(`missing element: ${selector}`);
  return element;
}

async function persistConfiguration(mutate: ConfigurationMutation): Promise<void> {
  try {
    await configuration.update(mutate);
  } catch (error) {
    setCaptureNotice(`Could not save settings: ${errorMessage(error)}`, "error");
  }
}

function option(value: string, label: string, selected = false): HTMLOptionElement {
  const element = document.createElement("option");
  element.value = value;
  element.textContent = label;
  element.selected = selected;
  return element;
}

function renderViewMode(): void {
  const viewMode = appState().navigation.viewMode;
  appWindow.dataset.viewMode = viewMode;
  const compact = viewMode === "compact";
  viewModeToggle.setAttribute("aria-label", compact ? "Open full view" : "Use compact view");
  viewModeToggle.querySelector<HTMLElement>(".view-mode-icon")!.innerHTML = compact
    ? icons.fullView
    : icons.compact;
  viewModeToggle.querySelector<HTMLElement>(".view-mode-label")!.textContent = compact
    ? "Open full view"
    : "Compact view";
}

async function changeViewMode(next: AppViewMode): Promise<void> {
  if (next === appState().navigation.viewMode) return;
  if (dialog.open) dialog.close();
  appStore.dispatch({ type: "navigation/view-mode", viewMode: next });
  await persistConfiguration((config) => { config.viewMode = next; });
  renderViewMode();
  setActiveNavigation("captions");
  try {
    await setWindowLayout(next);
  } catch (error) {
    reportWindowControlError(`switch to ${next} view`, error);
  }
}

function populateSpokenLanguageOptions(): void {
  spokenLanguage.replaceChildren(
    ...SPOKEN_LANGUAGES.map(({ code, label }) => option(code, label, code === "en")),
    option("auto", "Automatic · mixed languages")
  );
  spokenLanguage.value = appState().preferences.acceptedSpokenLanguage;
}

function populateTranslationTargets(): void {
  const sourceLanguage = supportedSourceLanguage(spokenLanguage.value);
  if (!sourceLanguage) {
    translationTarget.replaceChildren(option("off", "Off · original language", true));
    translationTarget.disabled = true;
    requireElement<HTMLElement>("#translation-target-help").textContent =
      "Automatic recognition does not yet report a stable source language for translation.";
    return;
  }

  translationTarget.disabled = false;
  translationTarget.replaceChildren(
    option("off", "Off · original language"),
    ...SPOKEN_LANGUAGES
      .filter(({ code }) => code !== sourceLanguage)
      .map(({ code, label }) => option(code, label))
  );
  const preferredTranslationTarget = appState().preferences.translationTarget;
  const preferredAvailable = [...translationTarget.options]
    .some(({ value }) => value === preferredTranslationTarget);
  translationTarget.value = preferredAvailable
    ? preferredTranslationTarget
    : sourceLanguage === "en" ? "off" : "en";
  requireElement<HTMLElement>("#translation-target-help").textContent =
    translationTarget.value === "off"
      ? "Recognition stays local and captions remain in the spoken language."
      : `Translation to ${languageLabel(translationTarget.value)} runs locally.`;
}

function populateSources(nextSnapshot: SourceSnapshot) {
  appStore.dispatch({ type: "sources/replaced", snapshot: nextSnapshot });
  const sources = appState().sources;
  const previousSource = sourceSelect.value;
  const configuredAudioSource = configuration.snapshot().config.captions.audioSource;
  const previousDevice = deviceSelect.value || (
    configuredAudioSource.kind === "playbackDevice"
      ? configuredAudioSource.deviceId
      : FOLLOW_SYSTEM_DEFAULT
  );

  sourceSelect.replaceChildren(option("system", "Everything I hear"));
  for (const application of sources.applications) {
    sourceSelect.append(option(`application:${application.processId}`, `Only ${application.name}`));
  }
  if ([...sourceSelect.options].some(({ value }) => value === previousSource)) {
    sourceSelect.value = previousSource;
  }

  const defaultDevice = sources.playbackDevices.find(({ isDefault }) => isDefault);
  const followLabel = defaultDevice
    ? `Follow system default — ${defaultDevice.name}`
    : "Follow system default";
  deviceSelect.replaceChildren(
    option(FOLLOW_SYSTEM_DEFAULT, followLabel, !previousDevice || previousDevice === FOLLOW_SYSTEM_DEFAULT)
  );
  for (const device of sources.playbackDevices) {
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

function selectedTranslationModel(
  catalog = appState().translations,
  source = spokenLanguage.value,
  target = translationTarget.value
): TranslationModelStatus | undefined {
  const sourceLanguage = supportedSourceLanguage(source);
  const targetLanguage = supportedTranslationLanguage(target);
  return sourceLanguage && targetLanguage
    ? translationStatusForRoute(catalog, sourceLanguage, targetLanguage)
    : undefined;
}

function translationRequested(): boolean {
  return captionOutput.outputMode() !== "original";
}

function renderCaptionOutputControl(): void {
  const sourceLanguage = supportedSourceLanguage(spokenLanguage.value);
  const targetLanguage = supportedTranslationLanguage(translationTarget.value);
  const targetLabel = targetLanguage ? languageLabel(targetLanguage) : "translation";
  const routeAvailable = sourceLanguage
    && targetLanguage
    && sourceLanguage !== targetLanguage;
  if (targetLanguage) captionOutput.setTranslationTarget(targetLanguage);
  const allowedModes: Array<[CaptionOutputMode, string]> = routeAvailable
    ? [
        ["original", "Original only · translation off"],
        ["translated", `${targetLabel} only · translated`],
        ["both", `Original + ${targetLabel}`]
      ]
    : [["original", "Original language"]];
  const selectedMode = routeAvailable ? appState().preferences.captionMode : "original";
  captionLanguage.replaceChildren(
    ...allowedModes.map(([value, label]) => option(value, label, value === selectedMode))
  );
  captionLanguage.value = selectedMode;
  captionLanguage.disabled = !routeAvailable;
  if (captionOutput.outputMode() !== selectedMode) captionOutput.setOutputMode(selectedMode);

  const help = requireElement<HTMLElement>("#caption-language-help");
  if (!sourceLanguage) {
    help.textContent = "Choose a specific spoken language to enable local translation.";
  } else if (!targetLanguage) {
    help.textContent = "Choose a Translate to language to enable translated captions.";
  } else {
    const model = selectedTranslationModel();
    if (!translationRequested()) {
      help.textContent = model?.phase === "ready"
        ? `Translation is off. Choose ${targetLabel} only or Original + ${targetLabel} to use the installed translator.`
        : `Translation is off. Choose ${targetLabel} only or Original + ${targetLabel} to install a translator.`;
    } else if (model?.phase === "ready") {
      help.textContent = `${targetLabel} starts from live partial speech and is corrected again when each caption finalizes.`;
    } else if (model?.phase === "loading") {
      help.textContent = `Original captions stay live while the local ${targetLabel} translator loads.`;
    } else {
      help.textContent = `Original captions stay live until the optional ${targetLabel} translator is installed.`;
    }
  }
  renderTranslationSetup();
  prepareSelectedTranslator();
}

function prepareSelectedTranslator(): void {
  const sourceLanguage = supportedSourceLanguage(spokenLanguage.value);
  const targetLanguage = supportedTranslationLanguage(translationTarget.value);
  const model = selectedTranslationModel();
  if (!sourceLanguage || !targetLanguage || !translationRequested() || model?.phase !== "ready") return;
  void captionOutput.prepare(sourceLanguage).catch((error: unknown) => {
    const message = error instanceof Error ? error.message : String(error);
    setCaptureNotice(
      `${languageLabel(targetLanguage)} translator could not start: ${message}`,
      "error"
    );
    void reportFrontendDiagnostic(
      "translation-model",
      `${languageLabel(sourceLanguage)} to ${languageLabel(targetLanguage)} preload: ${message}`
    );
  });
}

function renderTranslationSetup(): void {
  const model = selectedTranslationModel();
  if (!model || !translationRequested() || model.phase === "ready") {
    translationSetup.hidden = true;
    return;
  }

  translationSetup.hidden = false;
  translationSetupTitle.textContent = `${languageLabel(spokenLanguage.value)} to ${languageLabel(translationTarget.value)}`;
  translationProgress.hidden = model.phase !== "downloading";
  translationProgress.max = Math.max(model.totalBytes, 1);
  translationProgress.value = Math.min(model.downloadedBytes, translationProgress.max);
  const modelChangesBlocked = audioActive() || visualEngaged();
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
    translationMessage.textContent = "Original captions remain immediate while local translation starts.";
  } else {
    translationAction.textContent = model.phase === "corrupt"
      ? "Repair translator"
      : model.phase === "failed" ? "Retry translator" : `Download · ${formatBytes(model.totalBytes)}`;
    const baseMessage = model.message
      ?? `Download ${model.displayName} once. Captions and translation then stay on this PC.`;
    translationMessage.textContent = modelChangesBlocked
      ? `${baseMessage} Stop captions or screen translation before changing translation models.`
      : baseMessage;
  }
}

function renderTranslationStatus(catalog: TranslationCatalogStatus): void {
  const previousCatalog = appState().translations;
  const newlyFailed = catalog.models.find((model) => {
    if (model.phase !== "failed" && model.phase !== "corrupt") return false;
    const previous = previousCatalog.models.find(({ modelId }) => modelId === model.modelId);
    return previous?.phase !== "failed" && previous?.phase !== "corrupt";
  });
  const completed = catalog.models.find((model) =>
    model.phase === "ready"
    && previousCatalog.models.find(({ modelId }) => modelId === model.modelId)?.phase === "downloading"
  );
  const failed = catalog.models.find((model) =>
    (model.phase === "failed" || model.phase === "corrupt")
    && previousCatalog.models.find(({ modelId }) => modelId === model.modelId)?.phase === "downloading"
  );
  if (completed) {
    setSettingsNotice(
      `${completed.displayName} is installed and available from the Translate to control.`,
      "success"
    );
  } else if (failed) {
    setSettingsNotice(
      failed.message ?? `${failed.displayName} could not be installed.`,
      "error"
    );
  }
  if (newlyFailed) {
    void reportFrontendDiagnostic(
      "translation-model",
      `${newlyFailed.displayName}: ${newlyFailed.message ?? newlyFailed.phase}`
    );
  }
  appStore.dispatch({ type: "translation/catalog", catalog });
  renderCaptionOutputControl();
  if (dialog.open && dialog.dataset.panel === "models") renderSettingsPanel();
  if (dialog.open && dialog.dataset.panel === "visual") renderVisualPanel();
}

function renderLanguageGuidance() {
  const help = requireElement<HTMLElement>("#spoken-language-help");
  const language = SPOKEN_LANGUAGES.find(({ code }) => code === spokenLanguage.value);
  help.textContent = spokenLanguage.value === "auto"
    ? "For mixed-language audio. Detection can add delay or choose the wrong language."
    : language?.tier === "broad"
      ? "Supported by Nemotron's broad-coverage tier; accuracy can vary more than its primary languages."
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

function renderStatus(status: CaptureStatus) {
  const stateChanged = appState().captureStatus.state !== status.state;
  appStore.dispatch({ type: "capture/status", status });
  renderHeaderStatus();
  setCaptureNotice(status.message);
  const stoppable = status.state === "starting"
    || status.state === "capturing"
    || status.state === "waiting";
  captureToggle.textContent = stoppable ? "Stop Captions" : "Start Captions";
  captureToggle.classList.toggle("stop", stoppable);
  updatePrimaryAvailability();
  renderTranslationSetup();
  document.documentElement.style.setProperty("--audio-peak", String(status.peak));
  if (stateChanged && dialog.open && dialog.dataset.panel === "models") renderSettingsPanel();
  if (dialog.open && dialog.dataset.panel === "visual") renderVisualPanel();
}

function runtimeCaptureState(snapshot: RuntimeSnapshot): CaptureStatus["state"] {
  if (snapshot.lifecycle === "running") return "capturing";
  return snapshot.lifecycle;
}

function runtimeSessionActive(snapshot: RuntimeSnapshot): boolean {
  return snapshot.lifecycle === "starting"
    || snapshot.lifecycle === "running"
    || snapshot.lifecycle === "waiting"
    || snapshot.lifecycle === "stopping";
}

function applyRuntimeSnapshot(next: RuntimeSnapshot): void {
  const reduction = appStore.dispatch({
    type: "runtime/received",
    snapshot: next,
    expectedContractVersion: RUNTIME_CONTRACT_VERSION
  }).runtime;
  if (!reduction) return;
  if (reduction.contractMismatch) {
    const message = `The desktop runtime contract is ${next.contractVersion}; this interface expects ${RUNTIME_CONTRACT_VERSION}. Restart Prollyglot after updating.`;
    setCaptureNotice(message, "error");
    void reportFrontendDiagnostic("runtime-contract", message);
    return;
  }
  if (!reduction.accepted) return;
  const changedSession = reduction.sessionChanged;
  const presentationEpoch = next.sessionId === null
    ? undefined
    : { sessionId: next.sessionId, runtimeRevision: next.revision };
  const presentationActive = next.lifecycle === "starting"
    || next.lifecycle === "running"
    || next.lifecycle === "waiting";
  captionOutput.setPresentationEpoch(
    presentationActive && next.mode === "audioCaptions" ? presentationEpoch : undefined
  );
  visualTranslation.setPresentationEpoch(
    presentationActive && next.mode === "visualTranslation" ? presentationEpoch : undefined
  );
  const message = next.failure?.message ?? next.health.message ?? undefined;
  const sourceLabel = next.source?.label ?? undefined;

  if (next.mode === "audioCaptions") {
    const base = changedSession
      ? { state: "stopped" as const, peak: 0, droppedFrames: 0 }
      : appState().captureStatus;
    renderStatus({
      ...base,
      state: runtimeCaptureState(next),
      sourceLabel,
      message
    });
    const visualStatus = appState().visualStatus;
    if (visualStatus.active
      || visualStatus.state === "starting"
      || visualStatus.state === "waiting"
      || visualStatus.state === "stopping") {
      renderVisualStatus({
        active: false,
        state: "stopped",
        framesReceived: 0,
        framesAnalyzed: 0,
        framesUnchanged: 0,
        replacedFrames: 0,
        visibleRegions: 0,
        overlayRegions: 0
      });
    }
    return;
  }

  if (next.mode === "visualTranslation") {
    const base = changedSession
      ? {
          framesReceived: 0,
          framesAnalyzed: 0,
          framesUnchanged: 0,
          replacedFrames: 0,
          visibleRegions: 0,
          overlayRegions: 0
        }
      : appState().visualStatus;
    renderVisualStatus({
      ...base,
      active: runtimeSessionActive(next),
      state: runtimeCaptureState(next),
      sourceLabel,
      message
    });
    const captureStatus = appState().captureStatus;
    if (captureStatus.state === "starting"
      || captureStatus.state === "capturing"
      || captureStatus.state === "waiting"
      || captureStatus.state === "stopping") {
      renderStatus({ state: "stopped", peak: 0, droppedFrames: 0 });
    }
    return;
  }

  renderStatus({ state: "stopped", peak: 0, droppedFrames: 0 });
  renderVisualStatus({
    active: false,
    state: "stopped",
    framesReceived: 0,
    framesAnalyzed: 0,
    framesUnchanged: 0,
    replacedFrames: 0,
    visibleRegions: 0,
    overlayRegions: 0
  });
}

function audioActive(): boolean {
  const currentRuntime = appState().runtime.snapshot;
  if (currentRuntime) {
    return currentRuntime.mode === "audioCaptions" && runtimeSessionActive(currentRuntime);
  }
  const status = appState().captureStatus;
  return status.state === "starting"
    || status.state === "capturing"
    || status.state === "waiting"
    || status.state === "stopping";
}

function waitForRuntimeStopped(timeoutMs = 15_000): Promise<void> {
  const currentRuntime = appState().runtime.snapshot;
  if (!currentRuntime || currentRuntime.lifecycle === "stopped") return Promise.resolve();
  return new Promise((resolve, reject) => {
    let unsubscribe: () => void = () => undefined;
    const finish = () => {
      unsubscribe();
      window.clearTimeout(timeout);
    };
    const check = () => {
      if (appState().runtime.snapshot?.lifecycle !== "stopped") return;
      finish();
      resolve();
    };
    const timeout = window.setTimeout(() => {
      finish();
      reject(new Error("Prollyglot could not finish stopping the previous session in time. Restart the app and try again."));
    }, timeoutMs);
    unsubscribe = appStore.subscribe(check);
  });
}

function visualEngaged(): boolean {
  const currentRuntime = appState().runtime.snapshot;
  if (currentRuntime) {
    return currentRuntime.mode === "visualTranslation" && runtimeSessionActive(currentRuntime);
  }
  const visualStatus = appState().visualStatus;
  return visualStatus.active
    || visualStatus.state === "starting"
    || visualStatus.state === "stopping";
}

function renderHeaderStatus(): void {
  const currentRuntime = appState().runtime.snapshot;
  if (currentRuntime) {
    const state = runtimeCaptureState(currentRuntime);
    const labels: Record<CaptureStatus["state"], string> = {
      starting: "Starting",
      capturing: currentRuntime.mode === "visualTranslation" ? "Screen" : "Live",
      waiting: "Waiting",
      stopping: "Stopping",
      stopped: "Ready",
      failed: "Error"
    };
    statusLabel.dataset.state = state;
    statusText.textContent = labels[state];
    return;
  }
  const visualState = appState().visualStatus.state;
  const state = visualEngaged() || (visualState === "failed" && !audioActive())
    ? visualState
    : appState().captureStatus.state;
  const labels: Record<CaptureStatus["state"], string> = {
    starting: "Starting",
    capturing: visualEngaged() ? "Screen" : "Live",
    waiting: "Waiting",
    stopping: "Stopping",
    stopped: "Ready",
    failed: "Error"
  };
  statusLabel.dataset.state = state;
  statusText.textContent = labels[state];
}

function renderVisualStatus(status: VisualStatus): void {
  const previous = appState().visualStatus;
  const changed = previous.state !== status.state
    || previous.active !== status.active
    || previous.sourceLabel !== status.sourceLabel
    || previous.message !== status.message;
  appStore.dispatch({ type: "visual/status", status });
  visualToggle.textContent = status.active ? "View Screen Translation" : "Translate Screen…";
  visualToggle.dataset.active = String(status.active);
  visualToggle.disabled = status.state === "starting" || status.state === "stopping";
  renderHeaderStatus();
  updatePrimaryAvailability();
  renderModelStatus(appState().speechModels);
  renderTranslationSetup();
  if (changed && dialog.open && dialog.dataset.panel === "models") renderSettingsPanel();
  if (dialog.open && dialog.dataset.panel === "visual") {
    if (changed) renderVisualPanel();
    else visualPanel.updateStatus(status);
  }
}

function renderVisualModelStatus(catalog: VisualModelCatalogStatus): void {
  const previousCatalog = appState().visualModels;
  const completed = catalog.models.find((model) =>
    model.phase === "ready"
    && previousCatalog.models.find(({ modelId }) => modelId === model.modelId)?.phase === "downloading"
  );
  const failed = catalog.models.find((model) =>
    model.phase === "failed"
    && previousCatalog.models.find(({ modelId }) => modelId === model.modelId)?.phase === "downloading"
  );
  if (completed) {
    setSettingsNotice(`${completed.displayName} is installed and ready.`, "success");
  } else if (failed) {
    setSettingsNotice(
      failed.message ?? `${failed.displayName} could not be installed.`,
      "error"
    );
  }
  appStore.dispatch({ type: "visual/catalog", catalog });
  if (dialog.open && dialog.dataset.panel === "models") renderSettingsPanel();
  if (dialog.open && dialog.dataset.panel === "visual") renderVisualPanel();
}

function updatePrimaryAvailability() {
  const captureStatus = appState().captureStatus;
  const stoppable = captureStatus.state === "starting"
    || captureStatus.state === "capturing"
    || captureStatus.state === "waiting";
  const blockedByVisual = visualEngaged();
  const model = selectedModel();
  captureToggle.disabled = captureStatus.state === "stopping" || blockedByVisual
    || (!stoppable && (model.phase !== "ready" || !modelSupportsLanguage(model)));
  spokenLanguage.disabled = audioActive() || blockedByVisual;
}

function formatBytes(bytes: number): string {
  if (bytes <= 0) return "Unknown size";
  return `${(bytes / (1024 * 1024)).toFixed(1)} MiB`;
}

function selectedModel(catalog = appState().speechModels): ModelStatus {
  return catalog.models.find(({ modelId }) => modelId === catalog.selectedModelId)
    ?? catalog.models[0]
    ?? FALLBACK_SPEECH_MODEL;
}

function renderModelStatus(catalog: ModelCatalogStatus) {
  const previousCatalog = appState().speechModels;
  const completed = catalog.models.find((model) =>
    model.phase === "ready"
    && previousCatalog.models.find(({ modelId }) => modelId === model.modelId)?.phase === "downloading"
  );
  const failed = catalog.models.find((model) =>
    model.phase === "failed"
    && previousCatalog.models.find(({ modelId }) => modelId === model.modelId)?.phase === "downloading"
  );
  if (completed) {
    setSettingsNotice(`${completed.displayName} is installed and ready to use.`, "success");
  } else if (failed) {
    setSettingsNotice(
      failed.message ?? `${failed.displayName} could not be installed.`,
      "error"
    );
  }
  appStore.dispatch({ type: "speech/catalog", catalog });
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
  modelAction.disabled = status.phase === "checking"
    || status.phase === "downloading"
    || !compatible
    || visualEngaged();

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
  if (dialog.open && dialog.dataset.panel === "models") renderSettingsPanel();
}

function renderTranscript(snapshot: TranscriptSnapshot) {
  const accepted = appStore.dispatch({ type: "transcript/received", transcript: snapshot });
  if (!accepted.changed && snapshot.revision < appState().transcript.revision) return;
  captionOutput.updateTranscript(snapshot);
  renderSessionPreview();
  if (dialog.open && dialog.dataset.panel === "transcript") renderTranscriptPanel();
}

type SourceRefreshResult =
  | { ok: true; snapshot: SourceSnapshot }
  | { ok: false; message: string };

async function refreshSources(): Promise<SourceRefreshResult> {
  setCaptureNotice(undefined);
  try {
    const nextSnapshot = await sourceSnapshot();
    populateSources(nextSnapshot);
    return { ok: true, snapshot: nextSnapshot };
  } catch (error) {
    const message = errorMessage(error);
    setCaptureNotice(message, "error");
    return { ok: false, message };
  }
}

async function refreshVisualSources(): Promise<VisualSourceSnapshot> {
  const next = await visualSourceSnapshot();
  appStore.dispatch({ type: "visual/sources", snapshot: next });
  if (dialog.open && dialog.dataset.panel === "visual") renderVisualPanel();
  return next;
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
  segment: TranscriptSegment
): void {
  const copy = document.createElement("span");
  copy.className = "transcript-copy";
  const original = document.createElement("span");
  original.className = "transcript-text transcript-original";
  original.lang = segment.sourceLanguage === "auto" ? "" : segment.sourceLanguage;
  original.textContent = segment.text;
  const mode = captionOutput.outputMode();
  const translated = captionOutput.translationFor(segment);
  const targetLanguage = captionOutput.translationTarget();
  const targetLabel = languageLabel(targetLanguage);

  if (mode === "original") {
    copy.append(original);
    item.append(copy);
    return;
  }

  const translation = document.createElement("span");
  translation.className = "transcript-text transcript-translation";
  translation.lang = targetLanguage;
  if (translated?.phase === "ready") translation.textContent = translated.text;

  if (mode === "both") copy.append(original);
  if (translated?.phase === "ready") {
    copy.append(translation);
  } else {
    if (mode === "translated") {
      original.classList.add("translation-fallback");
      copy.append(original);
    }
    const note = document.createElement("span");
    note.className = "transcript-translation-state";
    note.textContent = translated?.phase === "failed"
      ? `${targetLabel} unavailable · showing original`
      : captionOutput.isTranslationPending(segment)
        ? `Translating to ${targetLabel}…`
        : `${targetLabel} translator is not ready`;
    copy.append(note);
  }
  item.append(copy);
}

function renderSessionPreview(): void {
  sessionPreviewContent.replaceChildren();
  const transcript = appState().transcript;
  const segments = transcript.committed.slice(-6);
  if (transcript.provisional) segments.push(transcript.provisional);
  if (segments.length === 0) {
    const empty = document.createElement("div");
    empty.className = "session-preview-empty";
    empty.innerHTML = `${icons.transcript}<strong>Waiting for captions</strong><span>The newest finalized and provisional text will stay visible here.</span>`;
    sessionPreviewContent.append(empty);
    return;
  }

  const list = document.createElement("ol");
  list.className = "session-preview-list";
  list.setAttribute("aria-label", "Latest captions");
  for (const segment of segments) {
    const item = document.createElement("li");
    item.className = `transcript-segment${segment.isFinal ? "" : " provisional"}`;
    const timestamp = document.createElement("time");
    timestamp.textContent = segment.isFinal ? formatTimestamp(segment.startMicros) : "Live";
    item.append(timestamp);
    appendTranscriptCaption(item, segment);
    list.append(item);
  }
  sessionPreviewContent.append(list);
  requestAnimationFrame(() => {
    sessionPreviewContent.scrollTop = sessionPreviewContent.scrollHeight;
  });
}

function renderTranscriptPanel(forceLatest = false) {
  const transcript = appState().transcript;
  const content = dialogContent();
  content.className = "";
  const previousList = content.querySelector<HTMLOListElement>(".transcript-list");
  const previousScrollTop = previousList?.scrollTop ?? 0;
  const previousDistanceFromBottom = previousList
    ? previousList.scrollHeight - previousList.clientHeight - previousList.scrollTop
    : 0;
  const shouldFollowLatest = forceLatest
    || !previousList
    || appState().transcriptFollowLatest
    || previousDistanceFromBottom <= TRANSCRIPT_BOTTOM_THRESHOLD;
  content.replaceChildren();

  const toolbar = document.createElement("div");
  toolbar.className = "dialog-toolbar";
  const summary = document.createElement("span");
  summary.className = "dialog-summary";
  summary.textContent = `${transcript.committed.length} finalized ${transcript.committed.length === 1 ? "caption" : "captions"}`;
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
  clear.disabled = transcript.committed.length === 0 && !transcript.provisional;
  clear.addEventListener("click", async () => {
    try {
      await clearTranscript();
    } catch (error) {
      setCaptureNotice(error instanceof Error ? error.message : String(error), "error");
    }
  });
  actions.append(latest, clear);
  toolbar.append(summary, actions);
  content.append(toolbar);

  if (transcript.committed.length === 0 && !transcript.provisional) {
    appStore.dispatch({ type: "transcript/follow-latest", follow: true });
    const empty = document.createElement("p");
    empty.className = "empty-copy";
    empty.textContent = "Finalized captions from this session will appear here.";
    content.append(empty);
    return;
  }

  const list = document.createElement("ol");
  list.className = "transcript-list";
  list.setAttribute("aria-label", "Session transcript");
  for (const segment of transcript.committed) {
    const item = document.createElement("li");
    item.className = "transcript-segment";
    const timestamp = document.createElement("time");
    timestamp.textContent = formatTimestamp(segment.startMicros);
    item.append(timestamp);
    appendTranscriptCaption(item, segment);
    list.append(item);
  }
  if (transcript.provisional) {
    const item = document.createElement("li");
    item.className = "transcript-segment provisional";
    const timestamp = document.createElement("time");
    timestamp.textContent = "Live";
    item.append(timestamp);
    appendTranscriptCaption(item, transcript.provisional);
    list.append(item);
  }
  content.append(list);

  const updateFollowState = () => {
    const distanceFromBottom = list.scrollHeight - list.clientHeight - list.scrollTop;
    const follow = distanceFromBottom <= TRANSCRIPT_BOTTOM_THRESHOLD;
    appStore.dispatch({ type: "transcript/follow-latest", follow });
    latest.hidden = follow;
  };
  list.addEventListener("scroll", updateFollowState, { passive: true });
  latest.addEventListener("click", () => {
    appStore.dispatch({ type: "transcript/follow-latest", follow: true });
    list.scrollTop = list.scrollHeight;
    latest.hidden = true;
  });

  requestAnimationFrame(() => {
    if (shouldFollowLatest) {
      appStore.dispatch({ type: "transcript/follow-latest", follow: true });
      list.scrollTop = list.scrollHeight;
      latest.hidden = true;
    } else {
      appStore.dispatch({ type: "transcript/follow-latest", follow: false });
      list.scrollTop = Math.min(previousScrollTop, list.scrollHeight - list.clientHeight);
      latest.hidden = false;
    }
  });
}

function renderSettingsPanel() {
  const content = dialogContent();
  const activeTranslationModel = selectedTranslationModel();
  settingsPanel.render(content, {
    speechCatalog: appState().speechModels,
    translationCatalog: appState().translations,
    visualCatalog: appState().visualModels,
    spokenLanguage: spokenLanguage.value,
    modelChangesBlocked: audioActive() || visualEngaged(),
    translationRequested: translationRequested(),
    activeTranslationModelId: activeTranslationModel?.modelId,
    visualRequested: appState().visualStatus.active
  }, {
    announce: setSettingsNotice,
    installSpeech: installSpeechModel,
    selectSpeech: async (modelId) => {
      await selectSpeechModel(modelId);
      renderModelStatus(await modelStatus());
    },
    removeSpeech: async (modelId) => {
      await removeSpeechModel(modelId);
      renderModelStatus(await modelStatus());
    },
    installTranslation: (modelId) => translationService.install(modelId),
    removeTranslation: (modelId) => translationService.remove(modelId),
    installVisual: installVisualModel,
    removeVisual: removeVisualModel,
    refreshSources: async () => {
      const result = await refreshSources();
      if (!result.ok) throw new Error(result.message);
      return {
        playbackDevices: result.snapshot.playbackDevices.length,
        applications: result.snapshot.applications.length
      };
    }
  });
  renderSettingsNotice();
}

function renderGeneralSettingsPanel(): void {
  const currentViewMode = appState().navigation.viewMode;
  const content = dialogContent();
  content.className = "general-settings-content";
  content.innerHTML = `
    <section class="general-settings-section" aria-labelledby="audio-settings-title">
      <div>
        <h3 id="audio-settings-title">Audio sources</h3>
        <p>Refresh after opening or closing an audio-producing application or changing playback devices.</p>
      </div>
      <button id="refresh-audio-sources" class="secondary-button" type="button">${icons.refresh}<span>Refresh sources</span></button>
      <p id="refresh-audio-result" class="settings-inline-status" role="status" aria-live="polite"></p>
    </section>
    <section class="general-settings-section" aria-labelledby="privacy-settings-title">
      <div>
        <h3 id="privacy-settings-title">Privacy</h3>
        <p>Audio, screenshots, recognized text, captions, and translation remain local. Prollyglot does not save raw audio or captured frames.</p>
      </div>
      <span class="settings-value"><span class="status-dot"></span>Local processing</span>
    </section>
    <section class="general-settings-section" aria-labelledby="window-settings-title">
      <div>
        <h3 id="window-settings-title">Window layout</h3>
        <p>Use the full workspace for setup and management, or switch to the compact utility for everyday Start and Stop controls.</p>
      </div>
      <button id="settings-view-mode" class="secondary-button" type="button">${currentViewMode === "full" ? icons.compact : icons.fullView}<span>${currentViewMode === "full" ? "Use compact view" : "Open full view"}</span></button>
    </section>
  `;
  const refresh = requireElement<HTMLButtonElement>("#refresh-audio-sources");
  const result = requireElement<HTMLElement>("#refresh-audio-result");
  refresh.addEventListener("click", () => {
    refresh.disabled = true;
    result.textContent = "Refreshing audio sources…";
    void refreshSources().then((next) => {
      if (!next.ok) throw new Error(next.message);
      result.textContent = `Found ${next.snapshot.playbackDevices.length} playback ${next.snapshot.playbackDevices.length === 1 ? "device" : "devices"} and ${next.snapshot.applications.length} ${next.snapshot.applications.length === 1 ? "application" : "applications"}.`;
    }).catch((error) => {
      result.textContent = error instanceof Error ? error.message : String(error);
    }).finally(() => {
      refresh.disabled = false;
    });
  });
  requireElement<HTMLButtonElement>("#settings-view-mode").addEventListener("click", () => {
    void changeViewMode(currentViewMode === "full" ? "compact" : "full");
  });
}

function renderVisualPanel(): void {
  visualPanel.render(dialogContent(), {
    capabilities: appState().visualCapabilities,
    sources: appState().visualSources,
    models: appState().visualModels,
    translations: appState().translations,
    status: appState().visualStatus,
    audioActive: audioActive()
  }, {
    refreshSources: refreshVisualSources,
    pickRegion: pickVisualRegion,
    installVisualModel,
    installTranslationModel: (modelId) => translationService.install(modelId),
    start: async (selection, sourceLanguage, targetLanguage, detectionMode) => {
      const supportedSource = supportedTranslationLanguage(sourceLanguage);
      const supportedTarget = supportedTranslationLanguage(targetLanguage);
      if (!supportedSource || !supportedTarget) {
        throw new Error("Visual translation requires a supported source and target language.");
      }
      captionOutput.setTranslationActive(false);
      visualTranslation.begin(sourceLanguage, targetLanguage, detectionMode);
      try {
        await startVisualTranslation(selection, sourceLanguage, targetLanguage, detectionMode);
      } catch (error) {
        visualTranslation.clear();
        captionOutput.setTranslationActive(true);
        if (isApplicationError(error) && error.code === "startupCancelled") return;
        void stopVisualTranslation().catch(() => undefined);
        throw error;
      }
    },
    stop: async () => {
      visualTranslation.clear();
      captionOutput.setTranslationActive(true);
      await stopVisualTranslation();
    },
    stopAudio: async () => {
      await stopCapture();
      await waitForRuntimeStopped();
    },
    openSettings: () => openDialogPanel("models"),
    report: (message) => {
      void reportFrontendDiagnostic("visual-translation", message);
    }
  });
}

function renderAppearancePanel(): void {
  appearancePanel.render(dialogContent(), {
    settings: structuredClone(configuration.snapshot().config.overlay),
    onChange: (settings) => configuration.update((config) => {
      config.overlay = structuredClone(settings);
    }).then(() => undefined)
  });
}

function renderSettingsNotice() {
  const status = document.querySelector<HTMLElement>("#settings-action-status");
  if (!status) return;
  const settingsNotice = appState().notices.settings;
  status.textContent = settingsNotice?.message ?? "";
  status.dataset.tone = settingsNotice?.tone ?? "neutral";
  status.hidden = !settingsNotice || dialog.dataset.panel !== "models";
  dialog.dataset.hasNotice = String(!status.hidden);
}

function setSettingsNotice(message: string, tone: SettingsNoticeTone) {
  appStore.dispatch({ type: "notice/settings", notice: { message, tone } });
  renderSettingsNotice();
}

function setCaptureNotice(
  message: string | undefined,
  tone: SettingsNoticeTone = "neutral"
): void {
  appStore.dispatch({
    type: "notice/capture",
    notice: message ? { message, tone } : undefined
  });
  captureMessage.textContent = message ?? "";
  captureMessage.dataset.tone = tone;
}

async function updateSpokenLanguage() {
  const language = spokenLanguage.value;
  renderLanguageGuidance();
  populateTranslationTargets();
  renderCaptionOutputControl();
  setCaptureNotice(undefined);
  const current = selectedModel();
  if (modelSupportsLanguage(current, language)) {
    appStore.dispatch({ type: "preferences/spoken-language", language });
    void persistConfiguration((config) => { config.captions.spokenLanguage = language; });
    renderModelStatus(appState().speechModels);
    return;
  }

  const candidates = appState().speechModels.models.filter(
    (model) => modelSupportsLanguage(model, language)
  );
  const candidate = candidates.find(({ phase }) => phase === "ready") ?? candidates[0];
  if (!candidate) {
    spokenLanguage.value = appState().preferences.acceptedSpokenLanguage;
    populateTranslationTargets();
    setCaptureNotice(`No installed model catalog supports ${languageLabel(language)}.`, "error");
    renderCaptionOutputControl();
    renderModelStatus(appState().speechModels);
    return;
  }

  try {
    await selectSpeechModel(candidate.modelId);
    appStore.dispatch({ type: "preferences/spoken-language", language });
    await persistConfiguration((config) => { config.captions.spokenLanguage = language; });
    renderModelStatus(await modelStatus());
  } catch (error) {
    spokenLanguage.value = appState().preferences.acceptedSpokenLanguage;
    populateTranslationTargets();
    setCaptureNotice(error instanceof Error ? error.message : String(error), "error");
    renderCaptionOutputControl();
    renderModelStatus(appState().speechModels);
  }
}

sourceSelect.addEventListener("change", updateSourceMode);
deviceSelect.addEventListener("change", () => {
  const deviceId = deviceSelect.value;
  void persistConfiguration((config) => {
    config.captions.audioSource = deviceId === FOLLOW_SYSTEM_DEFAULT
      ? { kind: "followSystemDefault" }
      : { kind: "playbackDevice", deviceId };
  });
});
spokenLanguage.addEventListener("change", () => void updateSpokenLanguage());
translationTarget.addEventListener("change", () => {
  const requestedTarget = translationTarget.value;
  appStore.dispatch({
    type: "preferences/translation-target",
    language: requestedTarget
  });
  if (requestedTarget === "off") {
    appStore.dispatch({ type: "preferences/caption-mode", mode: "original" });
    captionOutput.setOutputMode("original");
  }
  void persistConfiguration((config) => {
    if (requestedTarget === "off") {
      delete config.captions.translationTarget;
      config.captions.outputMode = "original";
    } else {
      config.captions.translationTarget = requestedTarget;
    }
  });
  const targetLanguage = supportedTranslationLanguage(requestedTarget);
  if (targetLanguage) captionOutput.setTranslationTarget(targetLanguage);
  requireElement<HTMLElement>("#translation-target-help").textContent = targetLanguage
    ? `Translation to ${languageLabel(targetLanguage)} runs locally.`
    : "Recognition stays local and captions remain in the spoken language.";
  renderCaptionOutputControl();
  if (dialog.open && dialog.dataset.panel === "transcript") renderTranscriptPanel();
});
captionLanguage.addEventListener("change", () => {
  const mode = captionLanguage.value as CaptionOutputMode;
  if (mode !== "original" && mode !== "translated" && mode !== "both") return;
  appStore.dispatch({ type: "preferences/caption-mode", mode });
  void persistConfiguration((config) => { config.captions.outputMode = mode; });
  captionOutput.setOutputMode(mode);
  renderCaptionOutputControl();
  if (dialog.open && dialog.dataset.panel === "transcript") renderTranscriptPanel();
});
modelAction.addEventListener("click", async () => {
  setCaptureNotice(undefined);
  try {
    await installSpeechModel(selectedModel().modelId);
  } catch (error) {
    setCaptureNotice(error instanceof Error ? error.message : String(error), "error");
  }
});
translationAction.addEventListener("click", async () => {
  setCaptureNotice(undefined);
  const model = selectedTranslationModel();
  if (!model) return;
  try {
    await translationService.install(model.modelId);
  } catch (error) {
    setCaptureNotice(error instanceof Error ? error.message : String(error), "error");
  }
});
captureToggle.addEventListener("click", async () => {
  setCaptureNotice(undefined);
  try {
    const status = appState().captureStatus;
    if (
      status.state === "starting"
      || status.state === "capturing"
      || status.state === "waiting"
    ) {
      await stopCapture();
    } else {
      if (visualEngaged()) throw new Error("Stop screen translation before starting audio captions.");
      captionOutput.setTranslationActive(true);
      const model = selectedModel();
      if (model.phase !== "ready") throw new Error("Install the selected speech model first.");
      if (!modelSupportsLanguage(model)) {
        throw new Error(`${model.displayName} does not support ${languageLabel(spokenLanguage.value)}.`);
      }
      await startCapture(selectedCapture(), spokenLanguage.value);
    }
  } catch (error) {
    if (isApplicationError(error) && error.code === "startupCancelled") return;
    renderStatus({
      state: "failed",
      peak: 0,
      droppedFrames: appState().captureStatus.droppedFrames,
      message: errorMessage(error)
    });
  }
});

visualToggle.addEventListener("click", () => openDialogPanel("visual"));

for (const button of document.querySelectorAll<HTMLButtonElement>("[data-appearance]")) {
  button.addEventListener("click", () => void showAppearance());
}

viewModeToggle.addEventListener("click", () => {
  void changeViewMode(appState().navigation.viewMode === "full" ? "compact" : "full");
});

function reportWindowControlError(action: string, error: unknown): void {
  const message = error instanceof Error ? error.message : String(error);
  setCaptureNotice(`Window ${action} failed: ${message}`, "error");
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

function openDialogPanel(panel: DialogPanel): void {
  const copy: Record<DialogPanel, { title: string; subtitle: string }> = {
    transcript: {
      title: "Transcript",
      subtitle: "Follow the newest caption by default or scroll back without losing your place."
    },
    models: {
      title: "Models",
      subtitle: "Manage installed packs and choose compatible local models by language."
    },
    settings: {
      title: "Settings",
      subtitle: "Application, source, and privacy controls."
    },
    visual: {
      title: "Screen translation",
      subtitle: "Continuously recognize and translate text in a window, display, or selected region."
    },
    appearance: {
      title: "Appearance",
      subtitle: "Customize readable captions and preview changes as you make them."
    }
  };
  dialog.dataset.panel = panel;
  requireElement<HTMLElement>("#dialog-title").textContent = copy[panel].title;
  requireElement<HTMLElement>("#dialog-subtitle").textContent = copy[panel].subtitle;
  if (panel === "models") {
    appStore.dispatch({ type: "notice/settings", notice: undefined });
    settingsPanel.resetView();
  } else {
    renderSettingsNotice();
  }
  if (!dialog.open) {
    if (appState().navigation.viewMode === "full") dialog.show();
    else dialog.showModal();
  }
  captionWorkspace.inert = true;
  captionWorkspace.setAttribute("aria-hidden", "true");
  setActiveNavigation(panel);
  if (panel === "models") renderSettingsPanel();
  else if (panel === "settings") renderGeneralSettingsPanel();
  else if (panel === "visual") renderVisualPanel();
  else if (panel === "appearance") renderAppearancePanel();
  else renderTranscriptPanel(true);
}

function setActiveNavigation(destination: DialogPanel | "captions"): void {
  appStore.dispatch({ type: "navigation/destination", destination });
  for (const button of document.querySelectorAll<HTMLButtonElement>(".desktop-nav-action")) {
    const selected = destination === "captions"
      ? button.dataset.workspace === "captions"
      : button.dataset.panel === destination;
    button.classList.toggle("is-active", selected);
    if (selected) button.setAttribute("aria-current", "page");
    else button.removeAttribute("aria-current");
  }
}

for (const button of document.querySelectorAll<HTMLButtonElement>("[data-panel]")) {
  button.addEventListener("click", () => {
    const panel = button.dataset.panel;
    if (panel === "transcript" || panel === "models" || panel === "settings" || panel === "visual" || panel === "appearance") {
      openDialogPanel(panel);
    }
  });
}

for (const button of document.querySelectorAll<HTMLButtonElement>("[data-workspace='captions']")) {
  button.addEventListener("click", () => {
    if (dialog.open) dialog.close();
    setActiveNavigation("captions");
  });
}

requireElement<HTMLButtonElement>(".dialog-close").addEventListener("click", () => dialog.close());
dialog.addEventListener("click", (event) => {
  if (appState().navigation.viewMode === "compact" && event.target === dialog) dialog.close();
});
dialog.addEventListener("close", () => {
  captionWorkspace.inert = false;
  captionWorkspace.removeAttribute("aria-hidden");
  setActiveNavigation("captions");
});

translationService.subscribe(renderTranslationStatus);
translationService.subscribeTelemetry((telemetry) => {
  const timing = telemetry.inferenceMs === undefined
    ? ""
    : ` inference=${telemetry.inferenceMs}ms queue=${telemetry.queueWaitMs ?? 0}ms`;
  const route = telemetry.sourceLanguage && telemetry.targetLanguage
    ? ` ${telemetry.sourceLanguage}->${telemetry.targetLanguage}`
    : "";
  void reportFrontendDiagnostic(
    "translation-scheduler",
    `${telemetry.event}${route} session=${telemetry.sessionId} queued=${telemetry.queuedJobs}${timing}`
      + (telemetry.reason ? ` reason=${telemetry.reason}` : ""),
    "info"
  );
});
renderViewMode();
if (appState().navigation.viewMode === "compact") {
  void setWindowLayout("compact").catch((error) => reportWindowControlError("restore compact view", error));
}
populateSpokenLanguageOptions();
populateTranslationTargets();
renderLanguageGuidance();
renderCaptionOutputControl();

async function initializeSessionRuntime(): Promise<void> {
  let bootstrapping = true;
  let newestEvent: RuntimeSnapshot | undefined;
  await onRuntimeState((next) => {
    if (bootstrapping) {
      if (!newestEvent || next.revision > newestEvent.revision) newestEvent = next;
      return;
    }
    applyRuntimeSnapshot(next);
  });
  await Promise.all([
    onCaptureStatus(renderStatus),
    onVisualStatus(renderVisualStatus)
  ]);
  const [bootstrap, audioStatus, screenStatus] = await Promise.all([
    runtimeBootstrap(),
    captureStatus(),
    visualStatus()
  ]);
  renderStatus(audioStatus);
  renderVisualStatus(screenStatus);
  bootstrapping = false;
  const newest = newestEvent && newestEvent.revision > bootstrap.snapshot.revision
    ? newestEvent
    : bootstrap.snapshot;
  applyRuntimeSnapshot(newest);
}

function acceptsVisualSessionEvent(
  sessionId: number,
  runtimeRevision: number,
  allowTerminal = false
): boolean {
  return acceptsVisualRuntimeEvent(
    appState().runtime,
    sessionId,
    runtimeRevision,
    allowTerminal
  );
}

void Promise.all([
  refreshSources(),
  visualCapabilities().then((capabilities) => {
    appStore.dispatch({ type: "visual/capabilities", capabilities });
    if (dialog.open && dialog.dataset.panel === "visual") renderVisualPanel();
  }),
  refreshVisualSources().catch((error) => {
    const capabilities = {
      ...appState().visualCapabilities,
      message: error instanceof Error ? error.message : String(error)
    };
    appStore.dispatch({ type: "visual/capabilities", capabilities });
  }),
  initializeSessionRuntime(),
  modelStatus().then(renderModelStatus),
  visualModelStatus().then(renderVisualModelStatus),
  translationService.initialize().then(renderTranslationStatus).catch((error) => {
    const message = error instanceof Error ? error.message : String(error);
    setCaptureNotice(message, "error");
    void reportFrontendDiagnostic("translation-model", message);
  }),
  transcriptSnapshot().then(renderTranscript),
  onModelStatus(renderModelStatus),
  onTranscriptUpdate(renderTranscript),
  onVisualModelStatus(renderVisualModelStatus),
  onVisualTextUpdate((update) => {
    if (acceptsVisualSessionEvent(update.sessionId, update.runtimeRevision)) {
      visualTranslation.update(update);
    }
  }),
  onVisualTextClear((event) => {
    if (!acceptsVisualSessionEvent(event.sessionId, event.runtimeRevision, true)) return;
    const status = appState().visualStatus;
    if (status.active && status.state === "capturing") {
      visualTranslation.rescan();
    } else {
      visualTranslation.clear();
    }
  })
]);
