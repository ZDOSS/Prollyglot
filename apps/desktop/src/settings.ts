import { icons } from "./icons";
import { languageLabel } from "./language-catalog";
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

interface ModelGroup<Model> {
  title: string;
  models: Model[];
}

const SETTINGS_FOCUS_ATTRIBUTE = "data-settings-focus-key";

function formatBytes(bytes: number): string {
  if (bytes <= 0) return "Unknown size";
  return `${(bytes / (1024 * 1024)).toFixed(1)} MiB`;
}

function percentage(downloadedBytes: number, totalBytes: number): number {
  if (totalBytes <= 0) return 0;
  return Math.max(0, Math.min(100, Math.round((downloadedBytes / totalBytes) * 100)));
}

function explicitLanguageLabels(languages: readonly string[]): string[] {
  return languages.filter((language) => language !== "auto").map(languageLabel);
}

function speechLanguageSummary(model: ModelStatus): string {
  const labels = explicitLanguageLabels(model.languages);
  if (labels.length > 6) {
    return `${labels.length} languages${model.languages.includes("auto") ? " + automatic detection" : ""}`;
  }
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
    return "Translates directly between supported languages locally. This flexible route is larger and may add more delay on CPU.";
  }
  if (model.kind === "toEnglish") {
    return "One compact local model covers the additional supported spoken languages when the output is English.";
  }
  return "A compact language-specific route for responsive local translation into English.";
}

function installedSpeechModels(catalog: ModelCatalogStatus): number {
  return catalog.models.filter(({ phase }) => phase === "ready").length;
}

function installedTranslationModels(catalog: TranslationCatalogStatus): number {
  return catalog.models.filter(({ phase }) => phase === "ready" || phase === "loading").length;
}

function installedVisualModels(catalog: VisualModelCatalogStatus): number {
  return catalog.models.filter(({ phase }) => phase === "ready").length;
}

function speechGroups(models: ModelStatus[]): ModelGroup<ModelStatus>[] {
  const groups: ModelGroup<ModelStatus>[] = [
    {
      title: "English quality",
      models: models.filter((model) => {
        const languages = model.languages.filter((language) => language !== "auto");
        return languages.length === 1 && languages[0] === "en";
      })
    },
    {
      title: "Dedicated languages",
      models: models.filter((model) => {
        const languages = model.languages.filter((language) => language !== "auto");
        return languages.length === 1 && languages[0] !== "en";
      })
    },
    {
      title: "Multilingual",
      models: models.filter((model) => model.languages.filter((language) => language !== "auto").length > 1)
    }
  ];
  return groups.filter(({ models: groupModels }) => groupModels.length > 0);
}

function translationGroups(models: TranslationModelStatus[]): ModelGroup<TranslationModelStatus>[] {
  const groups: ModelGroup<TranslationModelStatus>[] = [
    {
      title: "Translation into English",
      models: models.filter(({ kind }) => kind !== "manyToMany")
    },
    {
      title: "Any supported language",
      models: models.filter(({ kind }) => kind === "manyToMany")
    }
  ];
  return groups.filter(({ models: groupModels }) => groupModels.length > 0);
}

function element<K extends keyof HTMLElementTagNameMap>(tagName: K, className?: string): HTMLElementTagNameMap[K] {
  const result = document.createElement(tagName);
  if (className) result.className = className;
  return result;
}

function appendFact(list: HTMLDListElement, termText: string, detailText: string): void {
  const row = element("div", "model-fact");
  const term = element("dt");
  term.textContent = termText;
  const detail = element("dd");
  detail.textContent = detailText;
  row.append(term, detail);
  list.append(row);
}

function actionError(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export class SettingsPanel {
  private readonly expanded = new Set<string>();
  private readonly autoOpened = new Set<string>();
  private query = "";
  private pendingFocusKey?: string;
  private renderVersion = 0;
  private resetScrollOnNextRender = false;

  resetView(): void {
    this.expanded.clear();
    this.autoOpened.clear();
    this.query = "";
    this.pendingFocusKey = undefined;
    this.resetScrollOnNextRender = true;
  }

  render(content: HTMLElement, state: SettingsPanelState, actions: SettingsPanelActions): void {
    this.renderVersion += 1;
    const previousScrollTop = this.resetScrollOnNextRender ? 0 : content.scrollTop;
    this.resetScrollOnNextRender = false;
    const focused = document.activeElement instanceof HTMLElement && content.contains(document.activeElement)
      ? document.activeElement
      : undefined;
    const focusKey = focused?.dataset.settingsFocusKey ?? this.pendingFocusKey;
    const fallbackFocusKey = focused?.dataset.settingsFallbackFocusKey;
    this.pendingFocusKey = undefined;

    this.openInitialRow(`speech:${state.speechCatalog.selectedModelId}`);
    if (state.translationRequested && state.activeTranslationModelId) {
      this.openInitialRow(`translation:${state.activeTranslationModelId}`);
    }
    if (state.visualRequested && state.visualCatalog.models[0]) {
      this.openInitialRow(`visual:${state.visualCatalog.models[0].modelId}`);
    }

    content.replaceChildren();
    content.className = "settings-content";
    content.append(this.createOverview(state));
    content.append(this.createEmptySearchState());
    content.append(this.createSpeechSection(state, actions));
    content.append(this.createVisualSection(state, actions));
    content.append(this.createTranslationSection(state, actions));
    content.append(this.createAudioSection(actions));

    const search = content.querySelector<HTMLInputElement>("#model-search");
    if (search) {
      search.value = this.query;
      search.addEventListener("input", () => {
        this.query = search.value;
        this.applyFilter(content);
      });
    }
    this.applyFilter(content);
    content.scrollTop = previousScrollTop;
    this.restoreFocus(content, focusKey, fallbackFocusKey);
  }

  private openInitialRow(key: string): void {
    if (this.autoOpened.has(key)) return;
    this.autoOpened.add(key);
    this.expanded.add(key);
  }

  private createOverview(state: SettingsPanelState): HTMLElement {
    const section = element("section", "settings-overview");
    section.setAttribute("aria-labelledby", "model-library-title");
    const heading = element("h3");
    heading.id = "model-library-title";
    heading.textContent = "Models & language packs";
    const copy = element("p", "settings-copy");
    copy.textContent = "Download only what you use. Models stay on this PC and load into memory only when needed.";
    const summary = element("p", "model-library-summary");
    const speechInstalled = installedSpeechModels(state.speechCatalog);
    const translationInstalled = installedTranslationModels(state.translationCatalog);
    const visualInstalled = installedVisualModels(state.visualCatalog);
    summary.textContent = `${speechInstalled} of ${state.speechCatalog.models.length} speech · ${visualInstalled} of ${state.visualCatalog.models.length} visual · ${translationInstalled} of ${state.translationCatalog.models.length} translation installed`;

    const label = element("label", "sr-only");
    label.htmlFor = "model-search";
    label.textContent = "Search models and languages";
    const searchWrap = element("div", "settings-search");
    searchWrap.innerHTML = icons.search;
    const search = element("input");
    search.id = "model-search";
    search.type = "search";
    search.setAttribute("aria-label", "Search models and languages");
    search.placeholder = "Search models or languages";
    search.autocomplete = "off";
    search.dataset.settingsFocusKey = "model-search";
    searchWrap.append(search);
    section.append(heading, copy, summary, label, searchWrap);
    return section;
  }

  private createSpeechSection(state: SettingsPanelState, actions: SettingsPanelActions): HTMLElement {
    const section = element("section", "settings-section model-library-section");
    section.setAttribute("aria-labelledby", "speech-models-title");
    section.dataset.modelSection = "speech";
    section.append(this.createSectionHeading(
      "speech-models-title",
      "Speech recognition",
      `${installedSpeechModels(state.speechCatalog)} installed`
    ));
    const copy = element("p", "settings-copy");
    copy.textContent = "Select one installed model for the next caption session. Language-specific choices avoid loading the larger multilingual model.";
    section.append(copy);

    const anotherDownloadRunning = state.speechCatalog.models.some(({ phase }) => phase === "downloading");
    for (const [groupIndex, group] of speechGroups(state.speechCatalog.models).entries()) {
      const groupElement = this.createGroup(group.title, `speech-group-${groupIndex}`);
      const list = element("div", "model-disclosure-list");
      for (const [modelIndex, model] of group.models.entries()) {
        list.append(this.createSpeechRow(
          model,
          `speech-${groupIndex}-${modelIndex}`,
          state,
          actions,
          anotherDownloadRunning
        ));
      }
      groupElement.append(list);
      section.append(groupElement);
    }
    return section;
  }

  private createTranslationSection(state: SettingsPanelState, actions: SettingsPanelActions): HTMLElement {
    const section = element("section", "settings-section settings-section-divided model-library-section");
    section.setAttribute("aria-labelledby", "translation-models-title");
    section.dataset.modelSection = "translation";
    section.append(this.createSectionHeading(
      "translation-models-title",
      "Translation",
      `${installedTranslationModels(state.translationCatalog)} installed`
    ));
    const copy = element("p", "settings-copy");
    copy.textContent = "Compact routes translate into English. The universal route handles any supported language pair. Installing one does not turn translation on.";
    section.append(copy);

    const anotherDownloadRunning = state.translationCatalog.models.some(({ phase }) => phase === "downloading");
    for (const [groupIndex, group] of translationGroups(state.translationCatalog.models).entries()) {
      const groupElement = this.createGroup(group.title, `translation-group-${groupIndex}`);
      const list = element("div", "model-disclosure-list");
      for (const [modelIndex, model] of group.models.entries()) {
        list.append(this.createTranslationRow(
          model,
          `translation-${groupIndex}-${modelIndex}`,
          state,
          actions,
          anotherDownloadRunning
        ));
      }
      groupElement.append(list);
      section.append(groupElement);
    }
    return section;
  }

  private createVisualSection(state: SettingsPanelState, actions: SettingsPanelActions): HTMLElement {
    const section = element("section", "settings-section settings-section-divided model-library-section");
    section.setAttribute("aria-labelledby", "visual-models-title");
    section.dataset.modelSection = "visual";
    section.append(this.createSectionHeading(
      "visual-models-title",
      "Visual text recognition",
      `${installedVisualModels(state.visualCatalog)} installed`
    ));
    const copy = element("p", "settings-copy");
    copy.textContent = "This optional OCR pack reads text already visible on a selected window, display, or region. It is independent from speech recognition.";
    section.append(copy);

    const list = element("div", "model-disclosure-list");
    const anotherDownloadRunning = state.visualCatalog.models.some(({ phase }) => phase === "downloading");
    for (const [modelIndex, model] of state.visualCatalog.models.entries()) {
      list.append(this.createVisualRow(
        model,
        `visual-${modelIndex}`,
        state,
        actions,
        anotherDownloadRunning
      ));
    }
    section.append(list);
    return section;
  }

  private createSectionHeading(id: string, title: string, summaryText: string): HTMLElement {
    const row = element("div", "settings-section-heading");
    const heading = element("h3");
    heading.id = id;
    heading.textContent = title;
    const summary = element("span", "settings-section-summary");
    summary.textContent = summaryText;
    row.append(heading, summary);
    return row;
  }

  private createGroup(title: string, id: string): HTMLElement {
    const group = element("section", "model-group");
    group.dataset.modelGroup = "true";
    group.setAttribute("aria-labelledby", id);
    const heading = element("h4", "model-group-title");
    heading.id = id;
    heading.textContent = title;
    group.append(heading);
    return group;
  }

  private createSpeechRow(
    model: ModelStatus,
    domId: string,
    state: SettingsPanelState,
    actions: SettingsPanelActions,
    anotherDownloadRunning: boolean
  ): HTMLElement {
    const key = `speech:${model.modelId}`;
    const selected = model.modelId === state.speechCatalog.selectedModelId;
    const compatible = model.languages.includes(state.spokenLanguage);
    const row = this.createDisclosureShell(
      key,
      domId,
      model.profile,
      model.displayName,
      `${speechLanguageSummary(model)} · ${formatBytes(model.totalBytes)}`,
      this.speechStateLabel(model, selected),
      model.phase,
      selected
    );
    row.dataset.modelSearch = [
      "speech recognition",
      model.profile,
      model.displayName,
      model.description,
      model.modelId,
      ...model.languages.map(languageLabel)
    ].join(" ").toLocaleLowerCase();

    const panel = row.querySelector<HTMLElement>(".model-disclosure-panel");
    if (!panel) return row;
    const description = element("p", "model-option-description");
    description.textContent = model.description;
    panel.append(description);

    const facts = element("dl", "model-facts");
    appendFact(facts, "Languages", speechLanguageSummary(model));
    appendFact(facts, "Runtime", "Streaming · local CPU");
    appendFact(facts, "Download", formatBytes(model.totalBytes));
    panel.append(facts);
    this.appendCoverage(panel, model.languages);
    this.appendProgressAndMessage(panel, model);

    const actionsRow = element("div", "model-option-actions");
    if (model.phase === "ready") {
      const stateCopy = element("span", "model-action-state");
      stateCopy.textContent = selected
        ? "Selected for the next caption session"
        : compatible
          ? `Available for ${languageLabel(state.spokenLanguage)}`
          : `Choose ${speechLanguageSummary(model)} as the spoken language to use it`;
      actionsRow.append(stateCopy);
      if (!selected && compatible) {
        const use = this.actionButton("Use model", `${key}:use`, key);
        use.disabled = state.modelChangesBlocked;
        use.setAttribute("aria-label", `Use ${model.displayName}`);
        use.addEventListener("click", () => {
          void this.runAction(
            use,
            actions,
            `Selecting ${model.displayName}…`,
            async () => {
              await actions.selectSpeech(model.modelId);
              actions.announce(`${model.displayName} will be used for the next caption session.`, "success");
            }
          );
        });
        actionsRow.append(use);
      }
      const remove = this.textActionButton("Remove", `${key}:remove`, key);
      remove.classList.add("danger-text");
      remove.disabled = state.modelChangesBlocked || anotherDownloadRunning;
      remove.setAttribute("aria-label", `Remove ${model.displayName}`);
      remove.addEventListener("click", () => {
        void this.runAction(
          remove,
          actions,
          `Removing ${model.displayName}…`,
          async () => {
            await actions.removeSpeech(model.modelId);
            actions.announce(`${model.displayName} was removed from this PC.`, "success");
          }
        );
      });
      actionsRow.append(remove);
    } else {
      const download = this.actionButton(this.speechActionLabel(model), `${key}:install`, key);
      download.disabled = model.phase === "checking"
        || model.phase === "downloading"
        || state.modelChangesBlocked
        || anotherDownloadRunning;
      download.dataset.busy = String(model.phase === "checking" || model.phase === "downloading");
      download.setAttribute("aria-label", `${this.speechActionLabel(model)} ${model.displayName}`);
      if (model.phase !== "checking" && model.phase !== "downloading") {
        download.addEventListener("click", () => {
          this.expanded.add(key);
          void this.runAction(
            download,
            actions,
            `Starting ${model.displayName} download…`,
            () => actions.installSpeech(model.modelId)
          );
        });
      }
      actionsRow.append(download);
    }
    panel.append(actionsRow);
    this.appendBlockedReason(panel, state.modelChangesBlocked, anotherDownloadRunning, model.phase);
    return row;
  }

  private createTranslationRow(
    model: TranslationModelStatus,
    domId: string,
    state: SettingsPanelState,
    actions: SettingsPanelActions,
    anotherDownloadRunning: boolean
  ): HTMLElement {
    const key = `translation:${model.modelId}`;
    const activeRoute = state.translationRequested && state.activeTranslationModelId === model.modelId;
    const row = this.createDisclosureShell(
      key,
      domId,
      translationScope(model),
      model.displayName,
      `${formatBytes(model.totalBytes)} · ${model.license}`,
      this.translationStateLabel(model, activeRoute),
      model.phase,
      activeRoute
    );
    row.dataset.modelSearch = [
      "translation translator",
      translationScope(model),
      model.displayName,
      translationDescription(model),
      model.modelId,
      ...model.sourceLanguages.map(languageLabel),
      ...model.targetLanguages.map(languageLabel)
    ].join(" ").toLocaleLowerCase();

    const panel = row.querySelector<HTMLElement>(".model-disclosure-panel");
    if (!panel) return row;
    const description = element("p", "model-option-description");
    description.textContent = translationDescription(model);
    panel.append(description);

    const facts = element("dl", "model-facts");
    appendFact(facts, "Route", translationScope(model));
    appendFact(facts, "Runtime", `Local CPU · ${model.license}`);
    appendFact(facts, "Download", formatBytes(model.totalBytes));
    panel.append(facts);
    this.appendTranslationCoverage(panel, model);
    this.appendProgressAndMessage(panel, model);

    const actionsRow = element("div", "model-option-actions");
    if (model.phase === "ready") {
      const stateCopy = element("span", "model-action-state");
      stateCopy.textContent = activeRoute
        ? "Ready for the current caption route"
        : "Installed and available when this route is selected";
      actionsRow.append(stateCopy);
      const remove = this.textActionButton("Remove", `${key}:remove`, key);
      remove.classList.add("danger-text");
      remove.disabled = state.modelChangesBlocked || anotherDownloadRunning;
      remove.setAttribute("aria-label", `Remove ${model.displayName}`);
      remove.addEventListener("click", () => {
        void this.runAction(
          remove,
          actions,
          `Removing ${model.displayName}…`,
          async () => {
            await actions.removeTranslation(model.modelId);
            actions.announce(`${model.displayName} was removed from this PC.`, "success");
          }
        );
      });
      actionsRow.append(remove);
    } else {
      const download = this.actionButton(this.translationActionLabel(model), `${key}:install`, key);
      download.disabled = model.phase === "checking"
        || model.phase === "loading"
        || model.phase === "downloading"
        || state.modelChangesBlocked
        || anotherDownloadRunning;
      download.dataset.busy = String(
        model.phase === "checking" || model.phase === "loading" || model.phase === "downloading"
      );
      download.setAttribute("aria-label", `${this.translationActionLabel(model)} ${model.displayName}`);
      if (model.phase !== "checking" && model.phase !== "loading" && model.phase !== "downloading") {
        download.addEventListener("click", () => {
          this.expanded.add(key);
          void this.runAction(
            download,
            actions,
            `Starting ${model.displayName} download…`,
            () => actions.installTranslation(model.modelId)
          );
        });
      }
      actionsRow.append(download);
    }
    panel.append(actionsRow);
    this.appendBlockedReason(panel, state.modelChangesBlocked, anotherDownloadRunning, model.phase);
    return row;
  }

  private createVisualRow(
    model: ModelStatus,
    domId: string,
    state: SettingsPanelState,
    actions: SettingsPanelActions,
    anotherDownloadRunning: boolean
  ): HTMLElement {
    const key = `visual:${model.modelId}`;
    const active = state.visualRequested;
    const row = this.createDisclosureShell(
      key,
      domId,
      model.profile,
      model.displayName,
      `${speechLanguageSummary(model)} · ${formatBytes(model.totalBytes)}`,
      this.visualStateLabel(model, active),
      model.phase,
      active
    );
    row.dataset.modelSearch = [
      "visual screen OCR text recognition",
      model.profile,
      model.displayName,
      model.description,
      model.modelId,
      ...model.languages.map(languageLabel)
    ].join(" ").toLocaleLowerCase();

    const panel = row.querySelector<HTMLElement>(".model-disclosure-panel");
    if (!panel) return row;
    const description = element("p", "model-option-description");
    description.textContent = model.description;
    panel.append(description);

    const facts = element("dl", "model-facts");
    appendFact(facts, "Coverage", speechLanguageSummary(model));
    appendFact(facts, "Runtime", "PP-OCRv6 · local CPU");
    appendFact(facts, "Download", formatBytes(model.totalBytes));
    panel.append(facts);
    this.appendCoverage(panel, model.languages);
    this.appendProgressAndMessage(panel, model);

    const actionsRow = element("div", "model-option-actions");
    if (model.phase === "ready") {
      const stateCopy = element("span", "model-action-state");
      stateCopy.textContent = active
        ? "In use by visual translation"
        : "Installed and available for Translate Screen";
      actionsRow.append(stateCopy);
      const remove = this.textActionButton("Remove", `${key}:remove`, key);
      remove.classList.add("danger-text");
      remove.disabled = state.modelChangesBlocked || anotherDownloadRunning;
      remove.setAttribute("aria-label", `Remove ${model.displayName}`);
      remove.addEventListener("click", () => {
        void this.runAction(
          remove,
          actions,
          `Removing ${model.displayName}…`,
          async () => {
            await actions.removeVisual(model.modelId);
            actions.announce(`${model.displayName} was removed from this PC.`, "success");
          }
        );
      });
      actionsRow.append(remove);
    } else {
      const download = this.actionButton(this.speechActionLabel(model), `${key}:install`, key);
      download.disabled = model.phase === "checking"
        || model.phase === "downloading"
        || state.modelChangesBlocked
        || anotherDownloadRunning;
      download.dataset.busy = String(model.phase === "checking" || model.phase === "downloading");
      download.setAttribute("aria-label", `${this.speechActionLabel(model)} ${model.displayName}`);
      if (model.phase !== "checking" && model.phase !== "downloading") {
        download.addEventListener("click", () => {
          this.expanded.add(key);
          void this.runAction(
            download,
            actions,
            `Starting ${model.displayName} download…`,
            () => actions.installVisual(model.modelId)
          );
        });
      }
      actionsRow.append(download);
    }
    panel.append(actionsRow);
    this.appendBlockedReason(panel, state.modelChangesBlocked, anotherDownloadRunning, model.phase);
    return row;
  }

  private createDisclosureShell(
    key: string,
    domId: string,
    profile: string,
    displayName: string,
    metadata: string,
    stateLabel: string,
    phase: string,
    selected: boolean
  ): HTMLElement {
    const expanded = this.expanded.has(key);
    const row = element("article", "model-disclosure");
    row.dataset.expanded = String(expanded);
    row.dataset.phase = phase;
    row.dataset.selected = String(selected);

    const heading = element("h5", "model-disclosure-heading");
    const toggle = element("button", "model-disclosure-toggle");
    toggle.type = "button";
    toggle.id = `${domId}-toggle`;
    toggle.setAttribute("aria-expanded", String(expanded));
    toggle.setAttribute("aria-controls", `${domId}-panel`);
    toggle.dataset.settingsFocusKey = `${key}:toggle`;

    const summary = element("span", "model-summary");
    const profileElement = element("span", "model-profile");
    profileElement.textContent = profile;
    const name = element("span", "model-name");
    name.textContent = displayName;
    const meta = element("span", "model-summary-meta");
    meta.textContent = metadata;
    summary.append(profileElement, name, meta);

    const end = element("span", "model-summary-end");
    const stateElement = element("span", "model-state-label");
    stateElement.dataset.phase = phase;
    stateElement.textContent = stateLabel;
    const caret = element("span", "model-caret-wrap");
    caret.innerHTML = icons.disclosure;
    end.append(stateElement, caret);
    toggle.append(summary, end);
    heading.append(toggle);

    const panel = element("div", "model-disclosure-panel");
    panel.id = `${domId}-panel`;
    panel.setAttribute("role", "region");
    panel.setAttribute("aria-labelledby", toggle.id);
    panel.hidden = !expanded;
    toggle.addEventListener("click", () => {
      const nextExpanded = !this.expanded.has(key);
      if (nextExpanded) this.expanded.add(key);
      else this.expanded.delete(key);
      row.dataset.expanded = String(nextExpanded);
      toggle.setAttribute("aria-expanded", String(nextExpanded));
      panel.hidden = !nextExpanded;
    });
    row.append(heading, panel);
    return row;
  }

  private appendCoverage(panel: HTMLElement, languages: readonly string[]): void {
    const labels = explicitLanguageLabels(languages);
    if (labels.length <= 6) return;
    const details = element("details", "model-coverage-details");
    const summary = element("summary");
    summary.textContent = `View all ${labels.length} supported languages`;
    const copy = element("p");
    copy.textContent = labels.join(", ");
    details.append(summary, copy);
    panel.append(details);
  }

  private appendTranslationCoverage(panel: HTMLElement, model: TranslationModelStatus): void {
    const labels = model.kind === "manyToMany"
      ? [...new Set([...model.sourceLanguages, ...model.targetLanguages])].map(languageLabel)
      : model.sourceLanguages.map(languageLabel);
    if (labels.length <= 6) return;
    const details = element("details", "model-coverage-details");
    const summary = element("summary");
    summary.textContent = `View all ${labels.length} source languages`;
    const copy = element("p");
    copy.textContent = labels.join(", ");
    details.append(summary, copy);
    panel.append(details);
  }

  private appendProgressAndMessage(
    panel: HTMLElement,
    model: Pick<ModelStatus, "phase" | "downloadedBytes" | "totalBytes" | "message">
      | Pick<TranslationModelStatus, "phase" | "downloadedBytes" | "totalBytes" | "message">
  ): void {
    if (model.phase === "downloading") {
      const progress = element("progress", "model-progress model-option-progress");
      progress.max = Math.max(model.totalBytes, 1);
      progress.value = Math.min(model.downloadedBytes, progress.max);
      progress.setAttribute("aria-label", `Model download ${percentage(model.downloadedBytes, model.totalBytes)} percent`);
      panel.append(progress);
    }
    if (model.message && model.phase !== "ready") {
      const message = element("p", "model-option-message");
      message.dataset.tone = model.phase === "failed" || model.phase === "corrupt" ? "error" : "neutral";
      message.textContent = model.message;
      panel.append(message);
    }
  }

  private appendBlockedReason(
    panel: HTMLElement,
    modelChangesBlocked: boolean,
    anotherDownloadRunning: boolean,
    phase: string
  ): void {
    if (!modelChangesBlocked && (!anotherDownloadRunning || phase === "downloading")) return;
    const note = element("p", "model-option-message");
    note.textContent = modelChangesBlocked
      ? "Stop captions or screen translation before installing, selecting, or removing models."
      : "Another model in this section is downloading.";
    panel.append(note);
  }

  private speechStateLabel(model: ModelStatus, selected: boolean): string {
    if (model.phase === "checking") return "Checking";
    if (model.phase === "downloading") return `${percentage(model.downloadedBytes, model.totalBytes)}%`;
    if (model.phase === "corrupt") return "Needs repair";
    if (model.phase === "failed") return "Download failed";
    if (model.phase === "ready") return selected ? "In use" : "Installed";
    return selected ? "Selected" : "Available";
  }

  private translationStateLabel(model: TranslationModelStatus, activeRoute: boolean): string {
    if (model.phase === "checking") return "Checking";
    if (model.phase === "downloading") return `${percentage(model.downloadedBytes, model.totalBytes)}%`;
    if (model.phase === "loading") return "Loading";
    if (model.phase === "corrupt") return "Needs repair";
    if (model.phase === "failed") return "Download failed";
    if (activeRoute) return model.phase === "ready" ? "Current route" : "Needed now";
    return model.phase === "ready" ? "Installed" : "Available";
  }

  private visualStateLabel(model: ModelStatus, active: boolean): string {
    if (model.phase === "checking") return "Checking";
    if (model.phase === "downloading") return `${percentage(model.downloadedBytes, model.totalBytes)}%`;
    if (model.phase === "corrupt") return "Needs repair";
    if (model.phase === "failed") return "Download failed";
    if (model.phase === "ready") return active ? "In use" : "Installed";
    return "Available";
  }

  private speechActionLabel(model: ModelStatus): string {
    if (model.phase === "checking") return "Checking local files…";
    if (model.phase === "downloading") return `Downloading ${percentage(model.downloadedBytes, model.totalBytes)}%`;
    if (model.phase === "corrupt") return `Repair · ${formatBytes(model.totalBytes)}`;
    if (model.phase === "failed") return `Retry · ${formatBytes(model.totalBytes)}`;
    return `Download · ${formatBytes(model.totalBytes)}`;
  }

  private translationActionLabel(model: TranslationModelStatus): string {
    if (model.phase === "checking") return "Checking local files…";
    if (model.phase === "loading") return "Loading translator…";
    if (model.phase === "downloading") return `Downloading ${percentage(model.downloadedBytes, model.totalBytes)}%`;
    if (model.phase === "corrupt") return `Repair · ${formatBytes(model.totalBytes)}`;
    if (model.phase === "failed") return `Retry · ${formatBytes(model.totalBytes)}`;
    return `Download · ${formatBytes(model.totalBytes)}`;
  }

  private actionButton(label: string, focusKey: string, fallbackKey: string): HTMLButtonElement {
    const button = element("button", "secondary-button model-option-action");
    button.type = "button";
    button.textContent = label;
    button.dataset.settingsFocusKey = focusKey;
    button.dataset.settingsFallbackFocusKey = `${fallbackKey}:toggle`;
    return button;
  }

  private textActionButton(label: string, focusKey: string, fallbackKey: string): HTMLButtonElement {
    const button = element("button", "text-button");
    button.type = "button";
    button.textContent = label;
    button.dataset.settingsFocusKey = focusKey;
    button.dataset.settingsFallbackFocusKey = `${fallbackKey}:toggle`;
    return button;
  }

  private async runAction(
    control: HTMLButtonElement,
    actions: SettingsPanelActions,
    pendingMessage: string,
    operation: () => Promise<void>
  ): Promise<void> {
    const startingRenderVersion = this.renderVersion;
    const returnFocusKey = control.dataset.settingsFallbackFocusKey
      ?? control.dataset.settingsFocusKey;
    this.pendingFocusKey = returnFocusKey;
    control.disabled = true;
    actions.announce(pendingMessage, "neutral");
    try {
      await operation();
    } catch (error) {
      actions.announce(actionError(error), "error");
      control.disabled = false;
    } finally {
      if (this.renderVersion === startingRenderVersion && control.isConnected) {
        this.pendingFocusKey = undefined;
        control.disabled = false;
        control.focus({ preventScroll: true });
      }
    }
  }

  private createAudioSection(actions: SettingsPanelActions): HTMLElement {
    const section = element("section", "settings-section settings-section-divided");
    section.setAttribute("aria-labelledby", "audio-settings-title");
    const heading = element("h3");
    heading.id = "audio-settings-title";
    heading.textContent = "Audio sources";
    const copy = element("p", "settings-copy");
    copy.textContent = "Refresh after opening or closing an audio-producing application or changing playback devices.";
    const refresh = element("button", "secondary-button settings-wide-action");
    refresh.type = "button";
    refresh.innerHTML = `${icons.refresh}<span>Refresh audio sources</span>`;
    refresh.dataset.settingsFocusKey = "refresh-sources";
    refresh.addEventListener("click", () => {
      void this.runAction(refresh, actions, "Refreshing audio sources…", async () => {
        const result = await actions.refreshSources();
        actions.announce(
          `Found ${result.playbackDevices} playback ${result.playbackDevices === 1 ? "device" : "devices"} and ${result.applications} ${result.applications === 1 ? "application" : "applications"}.`,
          "success"
        );
        refresh.disabled = false;
      });
    });
    section.append(heading, copy, refresh);
    return section;
  }

  private createEmptySearchState(): HTMLElement {
    const empty = element("p", "model-search-empty");
    empty.hidden = true;
    empty.textContent = "No speech, visual, or translation models match that search.";
    return empty;
  }

  private applyFilter(content: HTMLElement): void {
    const query = this.query.trim().toLocaleLowerCase();
    let visibleRows = 0;
    for (const row of content.querySelectorAll<HTMLElement>("[data-model-search]")) {
      const visible = !query || row.dataset.modelSearch?.includes(query) === true;
      row.hidden = !visible;
      if (visible) visibleRows += 1;
    }
    for (const group of content.querySelectorAll<HTMLElement>("[data-model-group]")) {
      group.hidden = !group.querySelector<HTMLElement>("[data-model-search]:not([hidden])");
    }
    for (const section of content.querySelectorAll<HTMLElement>("[data-model-section]")) {
      section.hidden = !section.querySelector<HTMLElement>("[data-model-search]:not([hidden])");
    }
    const empty = content.querySelector<HTMLElement>(".model-search-empty");
    if (empty) empty.hidden = visibleRows > 0;
  }

  private restoreFocus(content: HTMLElement, focusKey?: string, fallbackFocusKey?: string): void {
    if (!focusKey) return;
    queueMicrotask(() => {
      const controls = [...content.querySelectorAll<HTMLElement>(`[${SETTINGS_FOCUS_ATTRIBUTE}]`)];
      const exact = controls.find((control) => control.dataset.settingsFocusKey === focusKey);
      const fallback = fallbackFocusKey
        ? controls.find((control) => control.dataset.settingsFocusKey === fallbackFocusKey)
        : undefined;
      const target = exact instanceof HTMLButtonElement && exact.disabled ? fallback : exact ?? fallback;
      target?.focus({ preventScroll: true });
    });
  }
}
