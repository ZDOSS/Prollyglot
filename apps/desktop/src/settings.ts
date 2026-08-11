import { icons } from "./icons";
import {
  SPOKEN_LANGUAGES,
  languageLabel,
  supportedTranslationLanguage
} from "./language-catalog";
import type {
  ModelCatalogStatus,
  ModelStatus,
  TranslationCatalogStatus,
  TranslationModelStatus,
  VisualModelCatalogStatus
} from "./types";

export type SettingsNoticeTone = "neutral" | "success" | "error";

export interface SettingsNotice {
  message: string;
  tone: SettingsNoticeTone;
}

export interface SettingsPanelState {
  speechCatalog: ModelCatalogStatus;
  translationCatalog: TranslationCatalogStatus;
  visualCatalog: VisualModelCatalogStatus;
  spokenLanguage: string;
  modelChangesBlocked: boolean;
  translationRequested: boolean;
  activeTranslationModelId?: string;
  visualRequested: boolean;
}

export interface SettingsPanelActions {
  announce: (message: string, tone: SettingsNoticeTone) => void;
  installSpeech: (modelId: string) => Promise<void>;
  selectSpeech: (modelId: string) => Promise<void>;
  removeSpeech: (modelId: string) => Promise<void>;
  installTranslation: (modelId: string) => Promise<void>;
  removeTranslation: (modelId: string) => Promise<void>;
  installVisual: (modelId: string) => Promise<void>;
  removeVisual: (modelId: string) => Promise<void>;
  refreshSources: () => Promise<{ playbackDevices: number; applications: number }>;
}

type ModelCategory = "speech" | "translation" | "visual";

const SETTINGS_FOCUS_ATTRIBUTE = "data-settings-focus-key";

function element<K extends keyof HTMLElementTagNameMap>(
  tagName: K,
  className?: string
): HTMLElementTagNameMap[K] {
  const result = document.createElement(tagName);
  if (className) result.className = className;
  return result;
}

function formatBytes(bytes: number): string {
  if (bytes <= 0) return "Unknown size";
  if (bytes >= 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GiB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MiB`;
}

function percentage(downloadedBytes: number, totalBytes: number): number {
  if (totalBytes <= 0) return 0;
  return Math.max(0, Math.min(100, Math.round((downloadedBytes / totalBytes) * 100)));
}

function speechLanguages(model: ModelStatus): string[] {
  return model.languages.filter((language) => language !== "auto");
}

function speechScope(model: ModelStatus): string {
  const labels = speechLanguages(model).map(languageLabel);
  if (labels.length > 5) return `${labels.length} languages${model.languages.includes("auto") ? " + automatic detection" : ""}`;
  if (model.languages.includes("auto")) labels.push("Automatic detection");
  return labels.join(", ");
}

function translationScope(model: TranslationModelStatus): string {
  if (model.kind === "direct") {
    return `${languageLabel(model.sourceLanguages[0] ?? "")} → ${languageLabel(model.targetLanguages[0] ?? "")}`;
  }
  if (model.kind === "toEnglish") return `${model.sourceLanguages.length} languages → English`;
  return `${model.sourceLanguages.length} languages ↔ ${model.targetLanguages.length} languages`;
}

function translationDescription(model: TranslationModelStatus): string {
  if (model.kind === "manyToMany") {
    return "The flexible local route for direct translation between any two supported languages. It is larger and may add more CPU delay.";
  }
  if (model.kind === "toEnglish") {
    return "One compact local model covers the additional supported source languages when the output is English.";
  }
  return "A compact language-specific route designed for responsive local translation into English.";
}

function actionError(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function selectOption(value: string, label: string, selected = false): HTMLOptionElement {
  const result = element("option");
  result.value = value;
  result.textContent = label;
  result.selected = selected;
  return result;
}

function selectField(labelText: string, id: string, select: HTMLSelectElement): HTMLElement {
  const field = element("div", "model-picker-field");
  const label = element("label", "field-label");
  label.htmlFor = id;
  label.textContent = labelText;
  const wrap = element("div", "select-wrap");
  select.id = id;
  select.className = "select-control";
  wrap.append(select);
  const chevron = element("span");
  chevron.innerHTML = icons.chevronDown;
  wrap.append(chevron);
  field.append(label, wrap);
  return field;
}

function installedSpeech(catalog: ModelCatalogStatus): ModelStatus[] {
  return catalog.models.filter(({ phase }) => phase === "ready");
}

function installedTranslation(catalog: TranslationCatalogStatus): TranslationModelStatus[] {
  return catalog.models.filter(({ phase }) => phase === "ready" || phase === "loading");
}

function installedVisual(catalog: VisualModelCatalogStatus): ModelStatus[] {
  return catalog.models.filter(({ phase }) => phase === "ready");
}

export class SettingsPanel {
  private category: ModelCategory = "speech";
  private installedExpanded = false;
  private query = "";
  private speechLanguage = "";
  private speechModelId = "";
  private translationSource = "";
  private translationTarget = "";
  private translationModelId = "";
  private visualModelId = "";
  private pendingFocusKey?: string;
  private container?: HTMLElement;
  private state?: SettingsPanelState;
  private actions?: SettingsPanelActions;

  resetView(): void {
    this.category = "speech";
    this.installedExpanded = false;
    this.query = "";
    this.pendingFocusKey = undefined;
  }

  render(content: HTMLElement, state: SettingsPanelState, actions: SettingsPanelActions): void {
    this.container = content;
    this.state = state;
    this.actions = actions;
    this.normalizeSelections(state);

    const previousScrollTop = content.scrollTop;
    const focused = document.activeElement instanceof HTMLElement && content.contains(document.activeElement)
      ? document.activeElement
      : undefined;
    const focusKey = this.pendingFocusKey ?? focused?.dataset.settingsFocusKey;
    this.pendingFocusKey = undefined;

    content.className = "model-manager-content";
    content.replaceChildren(
      this.createOverview(state),
      this.createInstalledSection(state, actions),
      this.createAddSection(state, actions)
    );
    content.scrollTop = previousScrollTop;
    if (focusKey) {
      requestAnimationFrame(() => {
        content.querySelector<HTMLElement>(`[${SETTINGS_FOCUS_ATTRIBUTE}="${CSS.escape(focusKey)}"]`)
          ?.focus({ preventScroll: true });
      });
    }
  }

  private normalizeSelections(state: SettingsPanelState): void {
    const speechLanguageCodes = new Set(
      state.speechCatalog.models.flatMap((model) => speechLanguages(model))
    );
    const preferredSpeech = state.spokenLanguage !== "auto" && speechLanguageCodes.has(state.spokenLanguage)
      ? state.spokenLanguage
      : "en";
    if (!speechLanguageCodes.has(this.speechLanguage)) this.speechLanguage = preferredSpeech;
    const speechCandidates = state.speechCatalog.models.filter((model) =>
      model.languages.includes(this.speechLanguage));
    if (!speechCandidates.some(({ modelId }) => modelId === this.speechModelId)) {
      this.speechModelId = speechCandidates.find(({ modelId }) => modelId === state.speechCatalog.selectedModelId)?.modelId
        ?? speechCandidates[0]?.modelId
        ?? "";
    }

    const preferredSource = supportedTranslationLanguage(state.spokenLanguage) ?? "ja";
    if (!supportedTranslationLanguage(this.translationSource)) this.translationSource = preferredSource;
    if (!supportedTranslationLanguage(this.translationTarget) || this.translationTarget === this.translationSource) {
      this.translationTarget = this.translationSource === "en" ? "es" : "en";
    }
    const translationCandidates = this.translationCandidates(state);
    if (!translationCandidates.some(({ modelId }) => modelId === this.translationModelId)) {
      this.translationModelId = translationCandidates.find(({ modelId }) => modelId === state.activeTranslationModelId)?.modelId
        ?? translationCandidates[0]?.modelId
        ?? "";
    }
    if (!state.visualCatalog.models.some(({ modelId }) => modelId === this.visualModelId)) {
      this.visualModelId = state.visualCatalog.models[0]?.modelId ?? "";
    }
  }

  private rerender(focusKey?: string): void {
    if (!this.container || !this.state || !this.actions) return;
    this.pendingFocusKey = focusKey;
    this.render(this.container, this.state, this.actions);
  }

  private createOverview(state: SettingsPanelState): HTMLElement {
    const section = element("section", "model-manager-overview");
    const copy = element("p");
    copy.textContent = "Models run locally. Nothing is downloaded until you explicitly choose Download.";
    const searchWrap = element("div", "settings-search model-manager-search");
    searchWrap.innerHTML = icons.search;
    const search = element("input");
    search.type = "search";
    search.value = this.query;
    search.placeholder = "Search models or languages";
    search.setAttribute("aria-label", "Search models or languages");
    search.autocomplete = "off";
    search.dataset.settingsFocusKey = "model-search";
    search.addEventListener("input", () => {
      this.query = search.value;
      this.rerender("model-search");
    });
    searchWrap.append(search);
    section.append(copy, searchWrap);
    return section;
  }

  private createInstalledSection(state: SettingsPanelState, actions: SettingsPanelActions): HTMLElement {
    const speech = installedSpeech(state.speechCatalog);
    const translation = installedTranslation(state.translationCatalog);
    const visual = installedVisual(state.visualCatalog);
    const count = speech.length + translation.length + visual.length;
    const bytes = [...speech, ...translation, ...visual]
      .reduce((total, model) => total + model.totalBytes, 0);

    const section = element("section", "installed-models");
    const toggle = element("button", "installed-models-toggle");
    toggle.type = "button";
    toggle.setAttribute("aria-expanded", String(this.installedExpanded));
    toggle.setAttribute("aria-controls", "installed-model-list");
    toggle.dataset.settingsFocusKey = "installed-toggle";
    const heading = element("span", "installed-models-heading");
    const title = element("strong");
    title.textContent = "Installed on this PC";
    const summary = element("span");
    summary.textContent = `${count} ${count === 1 ? "model" : "models"}${bytes > 0 ? ` · ${formatBytes(bytes)}` : ""}`;
    heading.append(title, summary);
    const caret = element("span", "model-caret-wrap");
    caret.innerHTML = icons.disclosure;
    toggle.append(heading, caret);
    toggle.addEventListener("click", () => {
      this.installedExpanded = !this.installedExpanded;
      this.rerender("installed-toggle");
    });
    section.append(toggle);

    const list = element("div", "installed-model-list");
    list.id = "installed-model-list";
    list.hidden = !this.installedExpanded;
    if (count === 0) {
      const empty = element("p", "installed-model-empty");
      empty.textContent = "No models are installed yet. Choose a language and model below to add only what you need.";
      list.append(empty);
    } else {
      const header = element("div", "installed-model-row installed-model-header");
      header.innerHTML = "<span>Model</span><span>Purpose / language</span><span>Size</span><span>Status</span><span></span>";
      list.append(header);
      for (const model of speech) {
        list.append(this.createInstalledRow(
          model.displayName,
          `Speech · ${speechScope(model)}`,
          model.totalBytes,
          model.modelId === state.speechCatalog.selectedModelId ? "In use" : "Ready",
          `speech:${model.modelId}`,
          state.modelChangesBlocked,
          () => actions.removeSpeech(model.modelId)
        ));
      }
      for (const model of translation) {
        list.append(this.createInstalledRow(
          model.displayName,
          `Translation · ${translationScope(model)}`,
          model.totalBytes,
          state.activeTranslationModelId === model.modelId && state.translationRequested ? "Current route" : "Ready",
          `translation:${model.modelId}`,
          state.modelChangesBlocked,
          () => actions.removeTranslation(model.modelId)
        ));
      }
      for (const model of visual) {
        list.append(this.createInstalledRow(
          model.displayName,
          "Screen text · multilingual OCR",
          model.totalBytes,
          state.visualRequested ? "In use" : "Ready",
          `visual:${model.modelId}`,
          state.modelChangesBlocked,
          () => actions.removeVisual(model.modelId)
        ));
      }
    }
    section.append(list);
    return section;
  }

  private createInstalledRow(
    name: string,
    purpose: string,
    bytes: number,
    status: string,
    key: string,
    blocked: boolean,
    remove: () => Promise<void>
  ): HTMLElement {
    const row = element("div", "installed-model-row");
    const nameCell = element("strong");
    nameCell.textContent = name;
    const purposeCell = element("span");
    purposeCell.textContent = purpose;
    const sizeCell = element("span");
    sizeCell.textContent = formatBytes(bytes);
    const statusCell = element("span", "installed-model-status");
    statusCell.innerHTML = '<span class="status-dot"></span>';
    statusCell.append(document.createTextNode(status));
    const button = element("button", "text-button danger-text installed-remove");
    button.type = "button";
    button.textContent = "Remove";
    button.disabled = blocked;
    button.setAttribute("aria-label", `Remove ${name}`);
    button.dataset.settingsFocusKey = `${key}:remove`;
    button.addEventListener("click", () => {
      void this.runAction(button, `Removing ${name}…`, async () => {
        await remove();
        this.actions?.announce(`${name} was removed from this PC.`, "success");
      });
    });
    row.append(nameCell, purposeCell, sizeCell, statusCell, button);
    return row;
  }

  private createAddSection(state: SettingsPanelState, actions: SettingsPanelActions): HTMLElement {
    const section = element("section", "add-model-section");
    const heading = element("div", "add-model-heading");
    const copy = element("div");
    const title = element("h3");
    title.textContent = "Add a model";
    const description = element("p");
    description.textContent = "Choose a purpose and language first; Prollyglot shows only compatible downloads.";
    copy.append(title, description);
    const explicit = element("span", "explicit-download-note");
    explicit.textContent = "Downloads start only when you click Download.";
    heading.append(copy, explicit);

    const tabs = element("div", "model-category-tabs");
    tabs.setAttribute("role", "tablist");
    const categoryCopy: Array<[ModelCategory, string, number]> = [
      ["speech", "Speech recognition", state.speechCatalog.models.length],
      ["translation", "Translation", state.translationCatalog.models.length],
      ["visual", "Screen text", state.visualCatalog.models.length]
    ];
    for (const [category, label, count] of categoryCopy) {
      const tab = element("button", "model-category-tab");
      tab.type = "button";
      tab.setAttribute("role", "tab");
      tab.setAttribute("aria-selected", String(this.category === category));
      tab.dataset.settingsFocusKey = `category:${category}`;
      tab.textContent = `${label} (${count})`;
      tab.addEventListener("click", () => {
        this.category = category;
        this.rerender(`category:${category}`);
      });
      tabs.append(tab);
    }
    section.append(heading, tabs);
    if (this.category === "speech") section.append(this.createSpeechPicker(state, actions));
    if (this.category === "translation") section.append(this.createTranslationPicker(state, actions));
    if (this.category === "visual") section.append(this.createVisualPicker(state, actions));
    return section;
  }

  private createSpeechPicker(state: SettingsPanelState, actions: SettingsPanelActions): HTMLElement {
    const panel = element("div", "model-picker-panel");
    const controls = element("div", "model-picker-controls");
    const availableLanguages = SPOKEN_LANGUAGES.filter(({ code }) =>
      state.speechCatalog.models.some((model) => model.languages.includes(code)));
    const language = element("select");
    language.dataset.settingsFocusKey = "speech-language";
    language.append(...availableLanguages.map(({ code, label }) =>
      selectOption(code, label, code === this.speechLanguage)));
    language.addEventListener("change", () => {
      this.speechLanguage = language.value;
      this.speechModelId = "";
      this.rerender("speech-language");
    });

    const candidates = state.speechCatalog.models.filter((model) =>
      model.languages.includes(this.speechLanguage) && this.matchesSearch([
        model.displayName,
        model.profile,
        model.description,
        ...model.languages.map(languageLabel)
      ]));
    if (!candidates.some(({ modelId }) => modelId === this.speechModelId)) {
      this.speechModelId = candidates[0]?.modelId ?? "";
    }
    const modelSelect = element("select");
    modelSelect.dataset.settingsFocusKey = "speech-model";
    if (candidates.length === 0) modelSelect.append(selectOption("", "No matching model", true));
    else modelSelect.append(...candidates.map((model) =>
      selectOption(model.modelId, `${model.profile} · ${formatBytes(model.totalBytes)}`, model.modelId === this.speechModelId)));
    modelSelect.disabled = candidates.length === 0;
    modelSelect.addEventListener("change", () => {
      this.speechModelId = modelSelect.value;
      this.rerender("speech-model");
    });
    controls.append(
      selectField("Language", "speech-model-language", language),
      selectField("Compatible model", "speech-model-choice", modelSelect)
    );
    panel.append(controls);
    const selected = candidates.find(({ modelId }) => modelId === this.speechModelId);
    panel.append(selected
      ? this.createSpeechDetail(selected, state, actions)
      : this.createNoMatch());
    return panel;
  }

  private createSpeechDetail(
    model: ModelStatus,
    state: SettingsPanelState,
    actions: SettingsPanelActions
  ): HTMLElement {
    const selected = model.modelId === state.speechCatalog.selectedModelId;
    return this.createModelDetail({
      eyebrow: model.profile,
      name: model.displayName,
      description: model.description,
      facts: [
        ["Language", speechScope(model)],
        ["Download", formatBytes(model.totalBytes)],
        ["Runtime", "Streaming · local CPU"],
        ["State", this.speechState(model, selected)]
      ],
      phase: model.phase,
      downloadedBytes: model.downloadedBytes,
      totalBytes: model.totalBytes,
      message: model.message,
      actionLabel: model.phase === "ready" ? selected ? "Selected" : "Use model" : this.installLabel(model),
      actionDisabled: (selected && model.phase === "ready")
        || state.modelChangesBlocked
        || model.phase === "checking"
        || model.phase === "downloading",
      actionKey: `speech:${model.modelId}:action`,
      action: model.phase === "ready"
        ? async () => {
            await actions.selectSpeech(model.modelId);
            actions.announce(`${model.displayName} will be used for the next caption session.`, "success");
          }
        : () => actions.installSpeech(model.modelId)
    });
  }

  private createTranslationPicker(state: SettingsPanelState, actions: SettingsPanelActions): HTMLElement {
    const panel = element("div", "model-picker-panel");
    const controls = element("div", "model-picker-controls translation-picker-controls");
    const source = element("select");
    source.dataset.settingsFocusKey = "translation-source";
    source.append(...SPOKEN_LANGUAGES.map(({ code, label }) =>
      selectOption(code, label, code === this.translationSource)));
    source.addEventListener("change", () => {
      this.translationSource = source.value;
      if (this.translationTarget === source.value) this.translationTarget = source.value === "en" ? "es" : "en";
      this.translationModelId = "";
      this.rerender("translation-source");
    });
    const target = element("select");
    target.dataset.settingsFocusKey = "translation-target";
    target.append(...SPOKEN_LANGUAGES
      .filter(({ code }) => code !== this.translationSource)
      .map(({ code, label }) => selectOption(code, label, code === this.translationTarget)));
    target.addEventListener("change", () => {
      this.translationTarget = target.value;
      this.translationModelId = "";
      this.rerender("translation-target");
    });

    const candidates = this.translationCandidates(state).filter((model) =>
      this.matchesSearch([
        model.displayName,
        translationScope(model),
        translationDescription(model),
        ...model.sourceLanguages.map(languageLabel),
        ...model.targetLanguages.map(languageLabel)
      ]));
    if (!candidates.some(({ modelId }) => modelId === this.translationModelId)) {
      this.translationModelId = candidates[0]?.modelId ?? "";
    }
    const modelSelect = element("select");
    modelSelect.dataset.settingsFocusKey = "translation-model";
    if (candidates.length === 0) modelSelect.append(selectOption("", "No matching route", true));
    else modelSelect.append(...candidates.map((model) =>
      selectOption(model.modelId, `${model.displayName} · ${formatBytes(model.totalBytes)}`, model.modelId === this.translationModelId)));
    modelSelect.disabled = candidates.length === 0;
    modelSelect.addEventListener("change", () => {
      this.translationModelId = modelSelect.value;
      this.rerender("translation-model");
    });
    controls.append(
      selectField("From", "translation-model-source", source),
      selectField("To", "translation-model-target", target),
      selectField("Compatible route", "translation-model-choice", modelSelect)
    );
    panel.append(controls);
    const selected = candidates.find(({ modelId }) => modelId === this.translationModelId);
    panel.append(selected
      ? this.createTranslationDetail(selected, state, actions)
      : this.createNoMatch("No installed or downloadable model matches this route and search."));
    return panel;
  }

  private translationCandidates(state: SettingsPanelState): TranslationModelStatus[] {
    return state.translationCatalog.models.filter((model) =>
      model.sourceLanguages.includes(this.translationSource)
      && model.targetLanguages.includes(this.translationTarget));
  }

  private createTranslationDetail(
    model: TranslationModelStatus,
    state: SettingsPanelState,
    actions: SettingsPanelActions
  ): HTMLElement {
    const active = state.translationRequested && state.activeTranslationModelId === model.modelId;
    return this.createModelDetail({
      eyebrow: model.kind === "manyToMany" ? "Flexible route" : "Compact route",
      name: model.displayName,
      description: translationDescription(model),
      facts: [
        ["Route", `${languageLabel(this.translationSource)} → ${languageLabel(this.translationTarget)}`],
        ["Download", formatBytes(model.totalBytes)],
        ["Runtime", `Local CPU · ${model.license}`],
        ["State", active ? "Current caption route" : model.phase === "ready" ? "Installed" : this.phaseLabel(model.phase)]
      ],
      phase: model.phase,
      downloadedBytes: model.downloadedBytes,
      totalBytes: model.totalBytes,
      message: model.message,
      actionLabel: model.phase === "ready" || model.phase === "loading" ? "Installed" : this.installLabel(model),
      actionDisabled: model.phase === "ready" || model.phase === "loading" || state.modelChangesBlocked || model.phase === "checking" || model.phase === "downloading",
      actionKey: `translation:${model.modelId}:action`,
      action: () => actions.installTranslation(model.modelId)
    });
  }

  private createVisualPicker(state: SettingsPanelState, actions: SettingsPanelActions): HTMLElement {
    const panel = element("div", "model-picker-panel");
    const candidates = state.visualCatalog.models.filter((model) => this.matchesSearch([
      model.displayName,
      model.profile,
      model.description,
      ...model.languages.map(languageLabel)
    ]));
    if (!candidates.some(({ modelId }) => modelId === this.visualModelId)) {
      this.visualModelId = candidates[0]?.modelId ?? "";
    }
    const modelSelect = element("select");
    modelSelect.dataset.settingsFocusKey = "visual-model";
    if (candidates.length === 0) modelSelect.append(selectOption("", "No matching OCR model", true));
    else modelSelect.append(...candidates.map((model) =>
      selectOption(model.modelId, `${model.displayName} · ${formatBytes(model.totalBytes)}`, model.modelId === this.visualModelId)));
    modelSelect.disabled = candidates.length === 0;
    modelSelect.addEventListener("change", () => {
      this.visualModelId = modelSelect.value;
      this.rerender("visual-model");
    });
    const controls = element("div", "model-picker-controls one-column");
    controls.append(selectField("Screen-text model", "visual-model-choice", modelSelect));
    panel.append(controls);
    const selected = candidates.find(({ modelId }) => modelId === this.visualModelId);
    panel.append(selected
      ? this.createVisualDetail(selected, state, actions)
      : this.createNoMatch());
    return panel;
  }

  private createVisualDetail(
    model: ModelStatus,
    state: SettingsPanelState,
    actions: SettingsPanelActions
  ): HTMLElement {
    return this.createModelDetail({
      eyebrow: model.profile,
      name: model.displayName,
      description: model.description,
      facts: [
        ["Coverage", speechScope(model)],
        ["Download", formatBytes(model.totalBytes)],
        ["Runtime", "PP-OCRv6 · local CPU"],
        ["State", state.visualRequested ? "In use" : model.phase === "ready" ? "Installed" : this.phaseLabel(model.phase)]
      ],
      phase: model.phase,
      downloadedBytes: model.downloadedBytes,
      totalBytes: model.totalBytes,
      message: model.message,
      actionLabel: model.phase === "ready" ? "Installed" : this.installLabel(model),
      actionDisabled: model.phase === "ready" || state.modelChangesBlocked || model.phase === "checking" || model.phase === "downloading",
      actionKey: `visual:${model.modelId}:action`,
      action: () => actions.installVisual(model.modelId)
    });
  }

  private createModelDetail(options: {
    eyebrow: string;
    name: string;
    description: string;
    facts: Array<[string, string]>;
    phase: string;
    downloadedBytes: number;
    totalBytes: number;
    message?: string;
    actionLabel: string;
    actionDisabled: boolean;
    actionKey: string;
    action: () => Promise<void>;
  }): HTMLElement {
    const detail = element("article", "model-picker-detail");
    detail.dataset.phase = options.phase;
    const copy = element("div", "model-picker-copy");
    const eyebrow = element("span", "model-profile");
    eyebrow.textContent = options.eyebrow;
    const title = element("h4");
    title.textContent = options.name;
    const description = element("p");
    description.textContent = options.description;
    copy.append(eyebrow, title, description);

    const facts = element("dl", "model-picker-facts");
    for (const [termText, valueText] of options.facts) {
      const row = element("div");
      const term = element("dt");
      term.textContent = termText;
      const value = element("dd");
      value.textContent = valueText;
      row.append(term, value);
      facts.append(row);
    }

    const actionArea = element("div", "model-picker-action");
    const state = element("span", "model-picker-state");
    state.textContent = this.phaseLabel(options.phase);
    const button = element("button", "secondary-button");
    button.type = "button";
    button.textContent = options.actionLabel;
    button.disabled = options.actionDisabled;
    button.dataset.busy = String(options.phase === "checking" || options.phase === "downloading" || options.phase === "loading");
    button.dataset.settingsFocusKey = options.actionKey;
    button.addEventListener("click", () => {
      void this.runAction(button, `Starting ${options.name}…`, options.action);
    });
    actionArea.append(state, button);
    detail.append(copy, facts, actionArea);
    if (options.phase === "downloading") {
      const progress = element("progress", "model-progress model-picker-progress");
      progress.max = Math.max(options.totalBytes, 1);
      progress.value = Math.min(options.downloadedBytes, progress.max);
      progress.setAttribute("aria-label", `Model download ${percentage(options.downloadedBytes, options.totalBytes)} percent`);
      detail.append(progress);
    }
    if (options.message && options.phase !== "ready") {
      const message = element("p", "model-picker-message");
      message.dataset.tone = options.phase === "failed" || options.phase === "corrupt" ? "error" : "neutral";
      message.textContent = options.message;
      detail.append(message);
    }
    return detail;
  }

  private createNoMatch(message = "No model matches the current language and search."): HTMLElement {
    const empty = element("div", "model-picker-empty");
    empty.textContent = message;
    return empty;
  }

  private matchesSearch(values: readonly string[]): boolean {
    const query = this.query.trim().toLocaleLowerCase();
    return !query || values.some((value) => value.toLocaleLowerCase().includes(query));
  }

  private phaseLabel(phase: string): string {
    if (phase === "ready") return "Installed";
    if (phase === "checking") return "Checking local files";
    if (phase === "downloading") return "Downloading";
    if (phase === "loading") return "Loading into memory";
    if (phase === "corrupt") return "Needs repair";
    if (phase === "failed") return "Download failed";
    return "Not installed";
  }

  private speechState(model: ModelStatus, selected: boolean): string {
    if (selected && model.phase === "ready") return "Selected for captions";
    return this.phaseLabel(model.phase);
  }

  private installLabel(model: Pick<ModelStatus, "phase" | "totalBytes" | "downloadedBytes">
    | Pick<TranslationModelStatus, "phase" | "totalBytes" | "downloadedBytes">): string {
    if (model.phase === "checking") return "Checking…";
    if (model.phase === "downloading") return `Downloading ${percentage(model.downloadedBytes, model.totalBytes)}%`;
    if (model.phase === "loading") return "Loading…";
    if (model.phase === "corrupt") return `Repair · ${formatBytes(model.totalBytes)}`;
    if (model.phase === "failed") return `Retry · ${formatBytes(model.totalBytes)}`;
    return `Download · ${formatBytes(model.totalBytes)}`;
  }

  private async runAction(
    button: HTMLButtonElement,
    pendingMessage: string,
    operation: () => Promise<void>
  ): Promise<void> {
    button.disabled = true;
    this.actions?.announce(pendingMessage, "neutral");
    try {
      await operation();
    } catch (error) {
      this.actions?.announce(actionError(error), "error");
      button.disabled = false;
    }
  }
}
