import "./styles.css";

import {
  captureStatus,
  clearTranscript,
  installSpeechModel,
  modelStatus,
  onCaptureStatus,
  onModelStatus,
  onTranscriptUpdate,
  removeSpeechModel,
  selectSpeechModel,
  showAppearance,
  sourceSnapshot,
  startCapture,
  stopCapture,
  transcriptSnapshot,
  windowAction
} from "./bridge";
import { icons } from "./icons";
import type {
  CaptureSelection,
  CaptureStatus,
  ModelCatalogStatus,
  ModelStatus,
  SourceSnapshot,
  TranscriptSnapshot
} from "./types";

const root = document.querySelector<HTMLElement>("#app");
if (!root) throw new Error("missing app root");

root.innerHTML = `
  <section class="app-window main-window" aria-label="Prollyglot controls">
    <header class="titlebar" data-tauri-drag-region>
      <div class="brand" data-tauri-drag-region>
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
          <label class="field-label" for="caption-language">Captions</label>
          <div class="select-wrap">
            <select id="caption-language" class="select-control" disabled aria-describedby="caption-language-help">
              <option value="original">Original language</option>
            </select>
            ${icons.chevronDown}
          </div>
          <span id="caption-language-help" class="sr-only">Translation is not available in this build.</span>
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
const captureToggle = requireElement<HTMLButtonElement>("#capture-toggle");
const captureMessage = requireElement<HTMLElement>("#capture-message");
const statusLabel = requireElement<HTMLElement>(".status-label");
const statusText = requireElement<HTMLElement>("#status-text");
const modelSetup = requireElement<HTMLElement>("#model-setup");
const modelSetupTitle = requireElement<HTMLElement>("#model-setup-title");
const modelMessage = requireElement<HTMLElement>("#model-message");
const modelProgress = requireElement<HTMLProgressElement>("#model-progress");
const modelAction = requireElement<HTMLButtonElement>("#model-action");
const dialog = requireElement<HTMLDialogElement>("#utility-dialog");

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
let currentTranscript: TranscriptSnapshot = { revision: 0, committed: [] };
let settingsNotice: { message: string; tone: "neutral" | "success" | "error" } | undefined;
let acceptedSpokenLanguage = "en";
let transcriptFollowLatest = true;
const FOLLOW_SYSTEM_DEFAULT = "__follow-system-default__";
const TRANSCRIPT_BOTTOM_THRESHOLD = 48;
const LANGUAGE_LABELS: Record<string, string> = {
  auto: "Automatic detection",
  en: "English",
  es: "Spanish",
  ja: "Japanese"
};

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
  modelAction.disabled = status.phase === "downloading" || !compatible;

  if (!compatible) {
    modelAction.textContent = "Choose compatible model";
    modelMessage.textContent = `${status.displayName} does not support ${languageLabel(spokenLanguage.value)}.`;
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
    const text = document.createElement("span");
    text.textContent = segment.text;
    item.append(timestamp, text);
    list.append(item);
  }
  if (currentTranscript.provisional) {
    const item = document.createElement("li");
    item.className = "transcript-segment provisional";
    const timestamp = document.createElement("time");
    timestamp.textContent = "Live";
    const text = document.createElement("span");
    text.textContent = currentTranscript.provisional.text;
    item.append(timestamp, text);
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
    badge.textContent = selected
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
    renderModelStatus(currentModels);
  }
}

sourceSelect.addEventListener("change", updateSourceMode);
spokenLanguage.addEventListener("change", () => void updateSpokenLanguage());
modelAction.addEventListener("click", async () => {
  captureMessage.textContent = "";
  try {
    await installSpeechModel(selectedModel().modelId);
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

for (const button of document.querySelectorAll<HTMLButtonElement>("[data-window-action]")) {
  button.addEventListener("click", () => {
    const action = button.dataset.windowAction;
    if (action === "minimize" || action === "maximize" || action === "close") void windowAction(action);
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

renderLanguageGuidance();
void Promise.all([
  refreshSources(),
  captureStatus().then(renderStatus),
  modelStatus().then(renderModelStatus),
  transcriptSnapshot().then(renderTranscript),
  onCaptureStatus(renderStatus),
  onModelStatus(renderModelStatus),
  onTranscriptUpdate(renderTranscript)
]);
