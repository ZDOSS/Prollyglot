import { icons } from "./icons";
import {
  SPOKEN_LANGUAGES,
  languageLabel,
  supportedTranslationLanguage
} from "./language-catalog";
import { translationStatusForRoute } from "./translation";
import type {
  PixelRect,
  TranslationCatalogStatus,
  VisualCaptureCapabilities,
  VisualDetectionMode,
  VisualCaptureSelection,
  VisualModelCatalogStatus,
  VisualSource,
  VisualSourceSnapshot,
  VisualStatus
} from "./types";

type VisualSourceMode = "applicationWindow" | "display" | "region";

export interface VisualPanelState {
  capabilities: VisualCaptureCapabilities;
  sources: VisualSourceSnapshot;
  models: VisualModelCatalogStatus;
  translations: TranslationCatalogStatus;
  status: VisualStatus;
  audioActive: boolean;
}

export interface VisualPanelActions {
  refreshSources: () => Promise<VisualSourceSnapshot>;
  pickRegion: (displayId: string) => Promise<PixelRect | undefined>;
  installVisualModel: (modelId: string) => Promise<void>;
  installTranslationModel: (modelId: string) => Promise<void>;
  start: (
    selection: VisualCaptureSelection,
    sourceLanguage: string,
    targetLanguage: string,
    detectionMode: VisualDetectionMode
  ) => Promise<void>;
  stop: () => Promise<void>;
  stopAudio: () => Promise<void>;
  openSettings: () => void;
  report: (message: string) => void;
}

interface StoredVisualPreferences {
  mode?: VisualSourceMode;
  sourceLanguage?: string;
  targetLanguage?: string;
  windowId?: string;
  displayId?: string;
  detectionMode?: VisualDetectionMode;
}

const STORAGE_KEY = "prollyglot.visual-translation";

function create<K extends keyof HTMLElementTagNameMap>(
  tagName: K,
  className?: string
): HTMLElementTagNameMap[K] {
  const element = document.createElement(tagName);
  if (className) element.className = className;
  return element;
}

function formatBytes(bytes: number): string {
  if (bytes <= 0) return "Unknown size";
  return `${(bytes / (1024 * 1024)).toFixed(1)} MiB`;
}

function option(value: string, label: string, selected = false): HTMLOptionElement {
  const element = create("option");
  element.value = value;
  element.textContent = label;
  element.selected = selected;
  return element;
}

function selectField(label: string, select: HTMLSelectElement, help?: string): HTMLElement {
  const field = create("div", "field-group visual-field");
  const fieldLabel = create("label", "field-label");
  fieldLabel.htmlFor = select.id;
  fieldLabel.textContent = label;
  const wrap = create("div", "select-wrap");
  select.className = "select-control";
  wrap.append(select);
  const icon = create("span", "visual-select-icon");
  icon.innerHTML = icons.chevronDown;
  wrap.append(icon);
  field.append(fieldLabel, wrap);
  if (help) {
    const copy = create("span", "field-help visual-field-help");
    copy.textContent = help;
    field.append(copy);
  }
  return field;
}

function readPreferences(): StoredVisualPreferences {
  try {
    return JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "{}") as StoredVisualPreferences;
  } catch {
    return {};
  }
}

export class VisualPanel {
  private mode: VisualSourceMode;
  private sourceLanguage: string;
  private targetLanguage: string;
  private windowId?: string;
  private displayId?: string;
  private detectionMode: VisualDetectionMode;
  private region?: PixelRect;
  private notice = "";
  private busy = false;
  private container?: HTMLElement;
  private state?: VisualPanelState;
  private actions?: VisualPanelActions;

  constructor() {
    const stored = readPreferences();
    this.mode = stored.mode === "display" || stored.mode === "region"
      ? stored.mode
      : "applicationWindow";
    this.sourceLanguage = supportedTranslationLanguage(stored.sourceLanguage ?? "ja") ?? "ja";
    this.targetLanguage = supportedTranslationLanguage(stored.targetLanguage ?? "en") ?? "en";
    if (this.sourceLanguage === this.targetLanguage) this.targetLanguage = "en";
    this.windowId = stored.windowId;
    this.displayId = stored.displayId;
    this.detectionMode = stored.detectionMode === "allText" ? "allText" : "focused";
  }

  render(
    container: HTMLElement,
    state: VisualPanelState,
    actions: VisualPanelActions
  ): void {
    this.container = container;
    this.state = state;
    this.actions = actions;
    container.className = "visual-panel";
    container.replaceChildren();
    if (state.status.active) this.renderActive(container, state, actions);
    else this.renderSetup(container, state, actions);
  }

  updateStatus(status: VisualStatus): void {
    if (!this.state) return;
    this.state = { ...this.state, status };
    if (!this.container || !status.active) return;
    const values: Record<string, number> = {
      received: status.framesReceived,
      analyzed: status.framesAnalyzed,
      visible: status.visibleRegions,
      replaced: status.replacedFrames
    };
    for (const [key, value] of Object.entries(values)) {
      const output = this.container.querySelector<HTMLElement>(`[data-visual-stat="${key}"]`);
      if (output) output.textContent = String(value);
    }
  }

  private rerender(): void {
    if (this.container && this.state && this.actions) {
      this.render(this.container, this.state, this.actions);
    }
  }

  private renderActive(
    container: HTMLElement,
    state: VisualPanelState,
    actions: VisualPanelActions
  ): void {
    const hero = create("section", "visual-active-card");
    const kicker = create("span", "visual-kicker");
    kicker.textContent = state.status.state === "failed" ? "Needs attention" : "Screen translation is on";
    const title = create("h3");
    title.textContent = state.status.sourceLabel ?? "Selected screen source";
    const message = create("p");
    message.textContent = state.status.message
      ?? `Visible ${languageLabel(this.sourceLanguage)} text is translated to ${languageLabel(this.targetLanguage)} locally.`;
    hero.append(kicker, title, message);

    const stats = create("dl", "visual-stats");
    this.appendStat(stats, "Live samples", String(state.status.framesReceived), "received");
    this.appendStat(stats, "OCR passes", String(state.status.framesAnalyzed), "analyzed");
    this.appendStat(stats, "Visible labels", String(state.status.visibleRegions), "visible");
    this.appendStat(stats, "Stale frames skipped", String(state.status.replacedFrames), "replaced");
    hero.append(stats);

    const stop = create("button", "primary-button stop visual-stop-button");
    stop.type = "button";
    stop.textContent = this.busy || state.status.state === "stopping"
      ? "Stopping…"
      : "Stop Screen Translation";
    stop.disabled = this.busy || state.status.state === "stopping";
    stop.addEventListener("click", () => {
      void this.run(async () => actions.stop(), "Could not stop screen translation.");
    });
    container.append(hero, stop);
  }

  private renderSetup(
    container: HTMLElement,
    state: VisualPanelState,
    actions: VisualPanelActions
  ): void {
    const intro = create("div", "visual-panel-intro");
    const copy = create("p");
    copy.textContent = "Continuously watch a window, display, or selected region and translate visible text as it changes. Pixels and recognition stay on this PC.";
    const settings = create("button", "text-button");
    settings.type = "button";
    settings.textContent = "Models & languages";
    settings.addEventListener("click", actions.openSettings);
    intro.append(copy, settings);
    container.append(intro);

    if (state.status.state === "failed" && state.status.message) {
      const failure = create("div", "visual-readiness-card");
      failure.dataset.tone = "error";
      const title = create("strong");
      title.textContent = "Last screen-translation attempt failed";
      const message = create("p");
      message.textContent = state.status.message;
      failure.append(title, message);
      container.append(failure);
    }

    if (!state.capabilities.windowsGraphicsCapture) {
      const unavailable = create("div", "visual-readiness-card");
      unavailable.dataset.tone = "error";
      const title = create("strong");
      title.textContent = "Windows screen capture is unavailable";
      const message = create("p");
      message.textContent = state.capabilities.message
        ?? "Visual translation currently requires Windows 11 and Windows Graphics Capture.";
      unavailable.append(title, message);
      container.append(unavailable);
    }

    const setupGrid = create("div", "visual-setup-grid");
    const sourceColumn = create("section", "visual-setup-column");
    const sourceHeading = create("div", "visual-setup-column-heading");
    const sourceTitle = create("h3");
    sourceTitle.textContent = "Capture source";
    const sourceDescription = create("p");
    sourceDescription.textContent = "Choose what Prollyglot watches and how selective recognition should be.";
    sourceHeading.append(sourceTitle, sourceDescription);
    sourceColumn.append(sourceHeading);

    const outputColumn = create("section", "visual-setup-column");
    const outputHeading = create("div", "visual-setup-column-heading");
    const outputTitle = create("h3");
    outputTitle.textContent = "Language & output";
    const outputDescription = create("p");
    outputDescription.textContent = "Route detected text through an installed local translator.";
    outputHeading.append(outputTitle, outputDescription);
    outputColumn.append(outputHeading);

    const mode = create("select");
    mode.id = "visual-source-mode";
    mode.append(
      option("applicationWindow", "Application window", this.mode === "applicationWindow"),
      option("display", "Whole display", this.mode === "display"),
      option("region", "Selected region", this.mode === "region")
    );
    mode.disabled = this.busy;
    mode.addEventListener("change", () => {
      this.mode = mode.value as VisualSourceMode;
      this.region = undefined;
      this.persist();
      this.rerender();
    });
    sourceColumn.append(selectField(
      "Screen source",
      mode,
      this.mode === "applicationWindow"
        ? "Continuously watches only the selected top-level window."
        : this.mode === "display"
          ? "Continuously watches the selected display, matching OBS-style monitor capture."
          : "Draw a smaller live area to watch around subtitles, signs, or a HUD."
    ));

    const sourceField = this.sourceField(state.sources, actions);
    sourceColumn.append(sourceField);

    const detectionMode = create("select");
    detectionMode.id = "visual-detection-mode";
    detectionMode.append(
      option("focused", "Prominent text · recommended", this.detectionMode === "focused"),
      option("allText", "All detected text", this.detectionMode === "allText")
    );
    detectionMode.disabled = this.busy;
    detectionMode.addEventListener("change", () => {
      this.detectionMode = detectionMode.value as VisualDetectionMode;
      this.persist();
      this.rerender();
    });
    sourceColumn.append(selectField(
      "Detection detail",
      detectionMode,
      this.detectionMode === "focused"
        ? "Filters low-confidence and small interface text so video captions, signs, and prominent HUD text stay useful."
        : "Includes small interface text. Use a selected region when the source contains unrelated controls."
    ));

    const languages = create("div", "visual-language-grid");
    const sourceLanguage = create("select");
    sourceLanguage.id = "visual-source-language";
    sourceLanguage.append(...SPOKEN_LANGUAGES.map(({ code, label }) =>
      option(code, label, code === this.sourceLanguage)));
    sourceLanguage.disabled = this.busy;
    sourceLanguage.addEventListener("change", () => {
      this.sourceLanguage = sourceLanguage.value;
      if (this.targetLanguage === this.sourceLanguage) {
        this.targetLanguage = this.sourceLanguage === "en" ? "es" : "en";
      }
      this.persist();
      this.rerender();
    });
    languages.append(selectField(
      "Text on screen",
      sourceLanguage,
      "Choose the language used to route recognized text into translation."
    ));

    const targetLanguage = create("select");
    targetLanguage.id = "visual-target-language";
    targetLanguage.append(...SPOKEN_LANGUAGES
      .filter(({ code }) => code !== this.sourceLanguage)
      .map(({ code, label }) => option(code, label, code === this.targetLanguage)));
    if (!targetLanguage.value) {
      this.targetLanguage = targetLanguage.options[0]?.value ?? "en";
      targetLanguage.value = this.targetLanguage;
    }
    targetLanguage.disabled = this.busy;
    targetLanguage.addEventListener("change", () => {
      this.targetLanguage = targetLanguage.value;
      this.persist();
      this.rerender();
    });
    languages.append(selectField(
      "Translate to",
      targetLanguage,
      "The original remains visible while its local translation appears nearby."
    ));
    outputColumn.append(languages);

    this.renderReadiness(outputColumn, state, actions);

    const notice = create("p", "capture-message visual-panel-notice");
    notice.setAttribute("role", "status");
    notice.setAttribute("aria-live", "polite");
    notice.textContent = this.notice;
    outputColumn.append(notice);

    const start = create("button", "primary-button visual-start-button");
    start.type = "button";
    const ready = this.readyToStart(state);
    start.disabled = this.busy || !ready.ok;
    start.textContent = this.busy
      ? "Starting…"
      : state.audioActive
        ? "Stop Captions & Translate Screen"
        : "Start Screen Translation";
    if (!ready.ok && !this.notice) notice.textContent = ready.reason;
    start.addEventListener("click", () => {
      void this.run(async () => {
        if (state.audioActive) await actions.stopAudio();
        const selection = this.selection(state.sources);
        await actions.start(selection, this.sourceLanguage, this.targetLanguage, this.detectionMode);
      }, "Could not start screen translation.");
    });
    outputColumn.append(start);
    setupGrid.append(sourceColumn, outputColumn);
    container.append(setupGrid);
  }

  private sourceField(sources: VisualSourceSnapshot, actions: VisualPanelActions): HTMLElement {
    const wrapper = create("section", "visual-source-field");
    const list = this.mode === "applicationWindow" ? sources.windows : sources.displays;
    const storedId = this.mode === "applicationWindow" ? this.windowId : this.displayId;
    const selectedId = list.some(({ id }) => id === storedId) ? storedId : list[0]?.id;
    if (this.mode === "applicationWindow") this.windowId = selectedId;
    else this.displayId = selectedId;

    const source = create("select");
    source.id = "visual-source";
    if (list.length === 0) source.append(option("", "No sources available", true));
    else source.append(...list.map(({ id, label }) => option(id, label, id === selectedId)));
    source.disabled = this.busy || list.length === 0;
    source.addEventListener("change", () => {
      if (this.mode === "applicationWindow") this.windowId = source.value;
      else {
        this.displayId = source.value;
        this.region = undefined;
      }
      this.persist();
      this.rerender();
    });
    wrapper.append(selectField(
      this.mode === "applicationWindow" ? "Window" : "Display",
      source
    ));

    const sourceActions = create("div", "visual-source-actions");
    const refresh = create("button", "secondary-button");
    refresh.type = "button";
    refresh.textContent = "Refresh sources";
    refresh.disabled = this.busy;
    refresh.addEventListener("click", () => {
      void this.run(async () => {
        await actions.refreshSources();
        this.notice = "Screen sources refreshed.";
      }, "Could not refresh screen sources.");
    });
    sourceActions.append(refresh);

    if (this.mode === "region") {
      const choose = create("button", "secondary-button visual-region-button");
      choose.type = "button";
      choose.textContent = this.region ? "Choose region again" : "Select region on screen";
      choose.disabled = this.busy || !this.displayId;
      choose.addEventListener("click", () => {
        if (!this.displayId) return;
        void this.run(async () => {
          const selected = await actions.pickRegion(this.displayId ?? "");
          if (selected) {
            this.region = selected;
            this.notice = `Selected ${selected.width} × ${selected.height} px region.`;
          }
        }, "Could not select a screen region.");
      });
      sourceActions.append(choose);
    }
    wrapper.append(sourceActions);
    if (this.mode === "region" && this.region) {
      const selected = create("p", "visual-region-summary");
      selected.textContent = `${this.region.width} × ${this.region.height} px selected at ${this.region.x}, ${this.region.y}`;
      wrapper.append(selected);
    }
    return wrapper;
  }

  private renderReadiness(
    container: HTMLElement,
    state: VisualPanelState,
    actions: VisualPanelActions
  ): void {
    const list = create("section", "visual-readiness-list");
    list.setAttribute("aria-label", "Required local models");
    const visualModel = state.models.models[0];
    if (visualModel) {
      const ready = visualModel.phase === "ready";
      list.append(this.readinessCard(
        "Text recognition",
        visualModel.displayName,
        ready ? "Ready" : visualModel.phase === "downloading"
          ? `${Math.round((visualModel.downloadedBytes / Math.max(visualModel.totalBytes, 1)) * 100)}%`
          : formatBytes(visualModel.totalBytes),
        ready,
        visualModel.phase === "downloading" || visualModel.phase === "checking" || this.busy,
        ready ? undefined : visualModel.phase === "corrupt" ? "Repair" : visualModel.phase === "failed" ? "Retry" : "Download",
        () => actions.installVisualModel(visualModel.modelId)
      ));
    }

    const source = supportedTranslationLanguage(this.sourceLanguage);
    const target = supportedTranslationLanguage(this.targetLanguage);
    const route = source && target
      ? translationStatusForRoute(state.translations, source, target)
      : undefined;
    if (route) {
      const ready = route.phase === "ready" || route.phase === "loading";
      list.append(this.readinessCard(
        "Translation",
        route.displayName,
        ready ? "Ready" : route.phase === "downloading"
          ? `${Math.round((route.downloadedBytes / Math.max(route.totalBytes, 1)) * 100)}%`
          : formatBytes(route.totalBytes),
        ready,
        route.phase === "downloading" || route.phase === "checking" || route.phase === "loading" || this.busy,
        ready ? undefined : route.phase === "corrupt" ? "Repair" : route.phase === "failed" ? "Retry" : "Download",
        () => actions.installTranslationModel(route.modelId)
      ));
    }
    container.append(list);
  }

  private readinessCard(
    kickerText: string,
    modelName: string,
    stateText: string,
    ready: boolean,
    disabled: boolean,
    actionLabel: string | undefined,
    action: () => Promise<void>
  ): HTMLElement {
    const card = create("article", "visual-readiness-card");
    card.dataset.ready = String(ready);
    const copy = create("div");
    const kicker = create("span", "visual-readiness-kicker");
    kicker.textContent = kickerText;
    const name = create("strong");
    name.textContent = modelName;
    copy.append(kicker, name);
    const end = create("div", "visual-readiness-end");
    const status = create("span", "visual-readiness-status");
    status.textContent = stateText;
    end.append(status);
    if (actionLabel) {
      const button = create("button", "text-button");
      button.type = "button";
      button.textContent = actionLabel;
      button.disabled = disabled;
      button.addEventListener("click", () => {
        void this.run(action, `Could not ${actionLabel.toLocaleLowerCase()} ${modelName}.`);
      });
      end.append(button);
    }
    card.append(copy, end);
    return card;
  }

  private readyToStart(state: VisualPanelState): { ok: boolean; reason: string } {
    if (!state.capabilities.windowsGraphicsCapture) {
      return { ok: false, reason: "Windows screen capture is unavailable on this system." };
    }
    try {
      this.selection(state.sources);
    } catch (error) {
      return { ok: false, reason: error instanceof Error ? error.message : String(error) };
    }
    if (state.models.models[0]?.phase !== "ready") {
      return { ok: false, reason: "Download the visual text-recognition pack first." };
    }
    const source = supportedTranslationLanguage(this.sourceLanguage);
    const target = supportedTranslationLanguage(this.targetLanguage);
    const route = source && target
      ? translationStatusForRoute(state.translations, source, target)
      : undefined;
    if (!route || (route.phase !== "ready" && route.phase !== "loading")) {
      return { ok: false, reason: `Download the ${languageLabel(this.sourceLanguage)} to ${languageLabel(this.targetLanguage)} translator first.` };
    }
    return { ok: true, reason: "" };
  }

  private selection(sources: VisualSourceSnapshot): VisualCaptureSelection {
    if (this.mode === "applicationWindow") {
      const source = this.requireSource(sources.windows, this.windowId, "window");
      return { kind: "applicationWindow", sourceId: source.id };
    }
    const display = this.requireSource(sources.displays, this.displayId, "display");
    if (this.mode === "display") return { kind: "display", sourceId: display.id };
    if (!this.region) throw new Error("Select a region on screen before starting.");
    return { kind: "region", displayId: display.id, region: this.region };
  }

  private requireSource(list: VisualSource[], id: string | undefined, label: string): VisualSource {
    const source = list.find((candidate) => candidate.id === id) ?? list[0];
    if (!source) throw new Error(`No ${label} is available to capture.`);
    return source;
  }

  private appendStat(
    list: HTMLDListElement,
    label: string,
    value: string,
    key: string
  ): void {
    const row = create("div");
    const term = create("dt");
    term.textContent = label;
    const detail = create("dd");
    detail.dataset.visualStat = key;
    detail.textContent = value;
    row.append(term, detail);
    list.append(row);
  }

  private async run(operation: () => Promise<void>, fallback: string): Promise<void> {
    if (this.busy) return;
    this.busy = true;
    this.notice = "";
    this.rerender();
    try {
      await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
      await operation();
    } catch (error) {
      this.notice = error instanceof Error
        ? error.message
        : typeof error === "string" && error.trim()
          ? error
          : fallback;
      this.actions?.report(this.notice);
    } finally {
      this.busy = false;
      this.rerender();
    }
  }

  private persist(): void {
    localStorage.setItem(STORAGE_KEY, JSON.stringify({
      mode: this.mode,
      sourceLanguage: this.sourceLanguage,
      targetLanguage: this.targetLanguage,
      windowId: this.windowId,
      displayId: this.displayId,
      detectionMode: this.detectionMode
    } satisfies StoredVisualPreferences));
  }
}
