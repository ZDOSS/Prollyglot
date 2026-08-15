import "./styles.css";

import { mountAppShell } from "./app-shell";
import { desktopBridge, isTauri } from "./bridge";
import { CaptionOutputController, supportedSourceLanguage } from "./caption-output";
import {
  CaptionForm,
  FOLLOW_SYSTEM_DEFAULT,
  captionActionCopy,
  planCaptionOutput
} from "./caption-form";
import { AppearancePanel } from "./appearance-panel";
import {
  initializeConfiguration,
  type ConfigurationMutation
} from "./configuration";
import {
  AppStore,
  FALLBACK_SPEECH_MODEL,
  createInitialAppState,
  type AppViewMode
} from "./app-store";
import { errorMessage, isApplicationError } from "./errors";
import { GeneralSettingsPanel } from "./general-settings-panel";
import { icons } from "./icons";
import {
  languageLabel,
  supportedTranslationLanguage
} from "./language-catalog";
import { SettingsPanel, type SettingsNoticeTone } from "./settings";
import {
  acceptsVisualSessionEvent as acceptsVisualRuntimeEvent
} from "./runtime-state";
import { initializeRuntimeBootstrap } from "./runtime-bootstrap";
import { TranslationService, translationStatusForRoute } from "./translation";
import { TranscriptPanel } from "./transcript-panel";
import { VisualPanel } from "./visual-panel";
import { VisualTranslationController } from "./visual-translation";
import { bindWindowControls } from "./window-controls";
import {
  WorkspaceNavigation,
  destinationFrom,
  type WorkspacePanel,
  type WorkspaceRenderContext
} from "./workspace-navigation";
import type {
  CaptionOutputMode,
  CaptureStatus,
  ModelCatalogStatus,
  ModelStatus,
  SourceSnapshot,
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
  clearTranscript,
  installSpeechModel,
  installVisualModel,
  modelStatus,
  onModelStatus,
  onTranscriptUpdate,
  onVisualModelStatus,
  onVisualTextClear,
  onVisualTextUpdate,
  pickVisualRegion,
  reportFrontendDiagnostic,
  removeSpeechModel,
  removeVisualModel,
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
  windowAction
} = desktopBridge;

const root = document.querySelector<HTMLElement>("#app");
if (!root) throw new Error("missing app root");

const {
  appWindow,
  captionLanguage,
  captureMessage,
  captureToggle,
  deviceField,
  deviceSelect,
  dialog,
  dialogClose,
  dialogContent: compactDialogContent,
  dialogSubtitle,
  dialogTitle,
  modelAction,
  modelMessage,
  modelProgress,
  modelSetup,
  modelSetupTitle,
  sessionPreviewContent,
  sourceSelect,
  spokenLanguage,
  statusLabel,
  statusText,
  titlebar,
  translationAction,
  translationMessage,
  translationProgress,
  translationSetup,
  translationSetupTitle,
  translationTarget,
  viewModeToggle,
  visualToggle
} = mountAppShell(root, __PROLLYGLOT_VERSION__);

const captionForm = new CaptionForm({
  source: sourceSelect,
  device: deviceSelect,
  deviceField,
  spokenLanguage,
  spokenLanguageHelp: requireElement("#spoken-language-help"),
  translationTarget,
  translationTargetHelp: requireElement("#translation-target-help"),
  captionOutput: captionLanguage,
  captionOutputHelp: requireElement("#caption-language-help")
});

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
const settingsPanel = new SettingsPanel("workspace-models");
const compactSettingsPanel = new SettingsPanel("compact-models");
const generalSettingsPanel = new GeneralSettingsPanel("workspace-settings");
const compactGeneralSettingsPanel = new GeneralSettingsPanel("compact-settings");
const visualPanel = new VisualPanel(
  structuredClone(configuration.snapshot().config.visual),
  (preferences) => {
    void persistConfiguration((config) => {
      config.visual = structuredClone(preferences);
    });
  },
  "workspace-visual"
);
const compactVisualPanel = new VisualPanel(
  structuredClone(configuration.snapshot().config.visual),
  (preferences) => {
    void persistConfiguration((config) => {
      config.visual = structuredClone(preferences);
    });
  },
  "compact-visual"
);
const appearancePanel = new AppearancePanel();
const workspaceNavigation = new WorkspaceNavigation({
  root: appWindow,
  dialog,
  dialogContent: compactDialogContent,
  dialogTitle,
  dialogSubtitle,
  dialogClose
}, {
  renderPanel: renderWorkspacePanel,
  onDestinationChange: (destination) => {
    appStore.dispatch({ type: "navigation/destination", destination });
  }
}, appState().navigation.viewMode);
let transcriptPanel: TranscriptPanel;
const captionOutput = new CaptionOutputController(
  translationService,
  (frame) => {
    workspaceNavigation.refresh("transcript");
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
transcriptPanel = new TranscriptPanel(sessionPreviewContent, {
  outputMode: () => captionOutput.outputMode(),
  translationTarget: () => captionOutput.translationTarget(),
  translationFor: (segment) => captionOutput.translationFor(segment),
  isTranslationPending: (segment) => captionOutput.isTranslationPending(segment)
}, {
  clear: clearTranscript,
  reportError: (message) => setCaptureNotice(message, "error"),
  setFollowLatest: (follow) => {
    appStore.dispatch({ type: "transcript/follow-latest", follow });
  }
});

configuration.subscribe((snapshot) => {
  const previous = appState();
  appStore.dispatch({ type: "configuration/accepted", snapshot: structuredClone(snapshot) });
  const next = appState();
  const previousConfig = previous.configuration?.config;
  if (previous.navigation.viewMode !== next.navigation.viewMode) {
    renderViewMode();
    workspaceNavigation.setViewMode(next.navigation.viewMode);
    workspaceNavigation.refresh("settings");
  }
  if (
    !previousConfig
    || JSON.stringify(previousConfig.overlay) !== JSON.stringify(snapshot.config.overlay)
  ) appearancePanel.updateSettings(snapshot.config.overlay);
  if (
    !previousConfig
    || JSON.stringify(previousConfig.visual) !== JSON.stringify(snapshot.config.visual)
  ) {
    visualPanel.updatePreferences(snapshot.config.visual);
    compactVisualPanel.updatePreferences(snapshot.config.visual);
  }
  if (
    previous.preferences.acceptedSpokenLanguage
      !== next.preferences.acceptedSpokenLanguage
    || previous.preferences.translationTarget !== next.preferences.translationTarget
    || previous.preferences.captionMode !== next.preferences.captionMode
  ) {
    spokenLanguage.value = next.preferences.acceptedSpokenLanguage;
    populateTranslationTargets();
    captionOutput.setOutputMode(next.preferences.captionMode);
    const target = supportedTranslationLanguage(translationTarget.value);
    if (target) captionOutput.setTranslationTarget(target);
    renderLanguageGuidance();
    renderCaptionOutputControl();
  }
});

function requireElement<T extends Element>(selector: string, parent: ParentNode = document): T {
  const element = parent.querySelector<T>(selector);
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
  appStore.dispatch({ type: "navigation/view-mode", viewMode: next });
  workspaceNavigation.setViewMode(next);
  await persistConfiguration((config) => { config.viewMode = next; });
  renderViewMode();
  try {
    await setWindowLayout(next);
  } catch (error) {
    reportWindowControlError(`switch to ${next} view`, error);
  }
}

function populateSpokenLanguageOptions(): void {
  captionForm.populateSpokenLanguages(appState().preferences.acceptedSpokenLanguage);
}

function populateTranslationTargets(): void {
  captionForm.populateTranslationTargets(appState().preferences.translationTarget);
}

function populateSources(nextSnapshot: SourceSnapshot) {
  appStore.dispatch({ type: "sources/replaced", snapshot: nextSnapshot });
  captionForm.populateSources(
    nextSnapshot,
    configuration.snapshot().config.captions.audioSource
  );
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
  const targetLanguage = supportedTranslationLanguage(translationTarget.value);
  if (targetLanguage) captionOutput.setTranslationTarget(targetLanguage);
  const plan = planCaptionOutput(
    spokenLanguage.value,
    translationTarget.value,
    appState().preferences.captionMode,
    selectedTranslationModel()?.phase
  );
  captionForm.renderCaptionOutput(plan);
  if (captionOutput.outputMode() !== plan.selected) captionOutput.setOutputMode(plan.selected);
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
  workspaceNavigation.refresh("models");
  workspaceNavigation.refresh("visual");
}

function renderLanguageGuidance() {
  captionForm.renderLanguageGuidance();
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
  if (stateChanged) workspaceNavigation.refresh("models");
  workspaceNavigation.refresh("visual");
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
  if (changed) {
    workspaceNavigation.refresh("models");
    workspaceNavigation.refresh("visual");
  } else {
    visualPanel.updateStatus(status);
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
  workspaceNavigation.refresh("models");
  workspaceNavigation.refresh("visual");
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
    modelMessage.textContent = `Download ${status.displayName} once, then ${captionActionCopy(spokenLanguage.value)} offline.`;
  }
  updatePrimaryAvailability();
  workspaceNavigation.refresh("models");
}

function renderTranscript(snapshot: TranscriptSnapshot) {
  const accepted = appStore.dispatch({ type: "transcript/received", transcript: snapshot });
  if (!accepted.changed && snapshot.revision < appState().transcript.revision) return;
  captionOutput.updateTranscript(snapshot);
  renderSessionPreview();
  workspaceNavigation.refresh("transcript");
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
  workspaceNavigation.refresh("visual");
  return next;
}

function renderWorkspacePanel(
  panel: WorkspacePanel,
  content: HTMLElement,
  context: WorkspaceRenderContext
): void {
  if (panel === "models") renderSettingsPanel(content);
  else if (panel === "settings") renderGeneralSettingsPanel(content);
  else if (panel === "visual") renderVisualPanel(content);
  else if (panel === "appearance") renderAppearancePanel(content);
  else renderTranscriptPanel(content, context.forceLatest);
}

function renderSessionPreview(): void {
  transcriptPanel.renderPreview(appState().transcript);
}

function renderTranscriptPanel(content: HTMLElement, forceLatest = false): void {
  transcriptPanel.render(content, appState().transcript, {
    forceLatest,
    followLatest: appState().transcriptFollowLatest
  });
}

function renderSettingsPanel(content: HTMLElement) {
  const activeTranslationModel = selectedTranslationModel();
  const panel = content === compactDialogContent ? compactSettingsPanel : settingsPanel;
  panel.render(content, {
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

function renderGeneralSettingsPanel(content: HTMLElement): void {
  const currentViewMode = appState().navigation.viewMode;
  const panel = content === compactDialogContent
    ? compactGeneralSettingsPanel
    : generalSettingsPanel;
  panel.render(content, currentViewMode, {
    refreshAudioSources: async () => {
      const result = await refreshSources();
      if (!result.ok) throw new Error(result.message);
      return {
        playbackDevices: result.snapshot.playbackDevices.length,
        applications: result.snapshot.applications.length
      };
    },
    changeViewMode
  });
}

function renderVisualPanel(content: HTMLElement): void {
  const panel = content === compactDialogContent ? compactVisualPanel : visualPanel;
  panel.render(content, {
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
    openSettings: () => workspaceNavigation.navigate("models"),
    report: (message) => {
      void reportFrontendDiagnostic("visual-translation", message);
    }
  });
}

function renderAppearancePanel(content: HTMLElement): void {
  appearancePanel.render(content, {
    settings: structuredClone(configuration.snapshot().config.overlay),
    onChange: (settings) => configuration.update((config) => {
      config.overlay = structuredClone(settings);
    }).then(() => undefined)
  });
}

function renderSettingsNotice() {
  const settingsNotice = appState().notices.settings;
  for (const status of document.querySelectorAll<HTMLElement>("[data-settings-action-status]")) {
    const inCompactDialog = dialog.contains(status);
    const relevant = !inCompactDialog || (dialog.open && dialog.dataset.panel === "models");
    status.textContent = settingsNotice?.message ?? "";
    status.dataset.tone = settingsNotice?.tone ?? "neutral";
    status.hidden = !settingsNotice || !relevant;
  }
  dialog.dataset.hasNotice = String(
    Boolean(settingsNotice && dialog.open && dialog.dataset.panel === "models")
  );
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

sourceSelect.addEventListener("change", () => captionForm.updateSourceMode());
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
  workspaceNavigation.refresh("transcript");
});
captionLanguage.addEventListener("change", () => {
  const mode = captionLanguage.value as CaptionOutputMode;
  if (mode !== "original" && mode !== "translated" && mode !== "both") return;
  appStore.dispatch({ type: "preferences/caption-mode", mode });
  void persistConfiguration((config) => { config.captions.outputMode = mode; });
  captionOutput.setOutputMode(mode);
  renderCaptionOutputControl();
  workspaceNavigation.refresh("transcript");
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
      await startCapture(captionForm.selectedCapture(), spokenLanguage.value);
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

visualToggle.addEventListener("click", () => {
  workspaceNavigation.navigate("visual", { opener: visualToggle });
});

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

bindWindowControls({ root: appWindow, titlebar }, {
  startDrag: startWindowDrag,
  perform: windowAction,
  reportError: reportWindowControlError
});

for (const button of document.querySelectorAll<HTMLButtonElement>("[data-destination]")) {
  button.addEventListener("click", () => {
    const destination = destinationFrom(button.dataset.destination);
    if (destination) workspaceNavigation.navigate(destination, { opener: button });
  });
}

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
    workspaceNavigation.refresh("visual");
  }),
  refreshVisualSources().catch((error) => {
    const capabilities = {
      ...appState().visualCapabilities,
      message: error instanceof Error ? error.message : String(error)
    };
    appStore.dispatch({ type: "visual/capabilities", capabilities });
  }),
  initializeRuntimeBootstrap(desktopBridge, {
    applyRuntime: applyRuntimeSnapshot,
    renderCapture: renderStatus,
    renderVisual: renderVisualStatus
  }),
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
