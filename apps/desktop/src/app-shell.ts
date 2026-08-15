import { icons } from "./icons";

export interface AppShellElements {
  appWindow: HTMLElement;
  captionLanguage: HTMLSelectElement;
  captionWorkspace: HTMLElement;
  captureMessage: HTMLElement;
  captureToggle: HTMLButtonElement;
  deviceField: HTMLElement;
  deviceSelect: HTMLSelectElement;
  dialog: HTMLDialogElement;
  dialogClose: HTMLButtonElement;
  dialogContent: HTMLElement;
  dialogSubtitle: HTMLElement;
  dialogTitle: HTMLElement;
  modelAction: HTMLButtonElement;
  modelMessage: HTMLElement;
  modelProgress: HTMLProgressElement;
  modelSetup: HTMLElement;
  modelSetupTitle: HTMLElement;
  sessionPreviewContent: HTMLElement;
  sourceSelect: HTMLSelectElement;
  spokenLanguage: HTMLSelectElement;
  statusLabel: HTMLElement;
  statusText: HTMLElement;
  titlebar: HTMLElement;
  translationAction: HTMLButtonElement;
  translationMessage: HTMLElement;
  translationProgress: HTMLProgressElement;
  translationSetup: HTMLElement;
  translationSetupTitle: HTMLElement;
  translationTarget: HTMLSelectElement;
  viewModeToggle: HTMLButtonElement;
  visualToggle: HTMLButtonElement;
}

interface WorkspacePageCopy {
  destination: string;
  title: string;
  subtitle: string;
}

const WORKSPACE_PAGES: WorkspacePageCopy[] = [
  {
    destination: "visual",
    title: "Screen translation",
    subtitle: "Continuously recognize and translate text in a window, display, or selected region."
  },
  {
    destination: "transcript",
    title: "Transcript",
    subtitle: "Follow the newest caption by default or scroll back without losing your place."
  },
  {
    destination: "models",
    title: "Models",
    subtitle: "Manage installed packs and choose compatible local models by language."
  },
  {
    destination: "appearance",
    title: "Appearance",
    subtitle: "Customize readable captions and preview changes as you make them."
  },
  {
    destination: "settings",
    title: "Settings",
    subtitle: "Application, source, and privacy controls."
  }
];

function workspacePage({ destination, title, subtitle }: WorkspacePageCopy): string {
  return `
    <section
      id="${destination}-workspace"
      class="workspace-page utility-workspace-page"
      data-workspace-page="${destination}"
      aria-labelledby="${destination}-page-title"
      aria-hidden="true"
      hidden
      inert
    >
      <header class="workspace-heading">
        <div>
          <h1 id="${destination}-page-title" tabindex="-1">${title}</h1>
          <p>${subtitle}</p>
        </div>
      </header>
      <div class="workspace-page-content" data-workspace-content="${destination}"></div>
      ${destination === "models"
        ? '<p class="settings-action-status workspace-action-status" data-settings-action-status role="status" aria-live="polite" hidden></p>'
        : ""}
    </section>
  `;
}

function requireElement<T extends Element>(root: ParentNode, selector: string): T {
  const element = root.querySelector<T>(selector);
  if (!element) throw new Error(`missing element: ${selector}`);
  return element;
}

export function mountAppShell(root: HTMLElement, version: string): AppShellElements {
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
            <button type="button" class="desktop-nav-action is-active" data-destination="captions" aria-current="page">${icons.captions}<span>Captions</span></button>
            <button type="button" class="desktop-nav-action" data-destination="visual">${icons.screen}<span>Screen translation</span></button>
            <button type="button" class="desktop-nav-action" data-destination="transcript">${icons.transcript}<span>Transcript</span></button>
            <button type="button" class="desktop-nav-action" data-destination="models">${icons.models}<span>Models</span></button>
            <button type="button" class="desktop-nav-action" data-destination="appearance">${icons.appearance}<span>Appearance</span></button>
            <button type="button" class="desktop-nav-action" data-destination="settings">${icons.settings}<span>Settings</span></button>
          </div>
          <div class="desktop-nav-footer">
            <span class="privacy-state"><span class="status-dot"></span>Local processing</span>
            <span class="version-state">Pre-release · ${version}</span>
          </div>
        </nav>

        <main class="workspace">
          <section id="caption-workspace" class="workspace-page" data-workspace-page="captions" aria-labelledby="caption-page-title">
            <header class="workspace-heading">
              <div>
                <h1 id="caption-page-title" tabindex="-1">Captions</h1>
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
                  <button type="button" class="text-button" data-destination="transcript">Open transcript</button>
                </header>
                <div id="session-preview-content" class="session-preview-content"></div>
              </aside>
            </div>
          </section>

          ${WORKSPACE_PAGES.map(workspacePage).join("")}

          <dialog id="utility-dialog" class="utility-dialog" aria-labelledby="dialog-title">
            <div class="dialog-title-row">
              <div class="dialog-heading-copy">
                <h2 id="dialog-title"></h2>
                <p id="dialog-subtitle"></p>
              </div>
              <button type="button" class="dialog-close" aria-label="Close">${icons.close}</button>
            </div>
            <div id="dialog-content"></div>
            <p class="settings-action-status" data-settings-action-status role="status" aria-live="polite" hidden></p>
          </dialog>
        </main>
      </div>

      <nav class="utility-nav compact-nav" aria-label="Compact application views">
        <button type="button" class="utility-action" data-destination="transcript">${icons.transcript}<span>Transcript</span></button>
        <button type="button" class="utility-action" data-appearance>${icons.appearance}<span>Appearance</span></button>
        <button type="button" class="utility-action" data-destination="models">${icons.models}<span>Models</span></button>
      </nav>
    </section>
  `;

  return {
    appWindow: requireElement(root, ".main-window"),
    captionLanguage: requireElement(root, "#caption-language"),
    captionWorkspace: requireElement(root, "#caption-workspace"),
    captureMessage: requireElement(root, "#capture-message"),
    captureToggle: requireElement(root, "#capture-toggle"),
    deviceField: requireElement(root, "#device-field"),
    deviceSelect: requireElement(root, "#playback-device"),
    dialog: requireElement(root, "#utility-dialog"),
    dialogClose: requireElement(root, ".dialog-close"),
    dialogContent: requireElement(root, "#dialog-content"),
    dialogSubtitle: requireElement(root, "#dialog-subtitle"),
    dialogTitle: requireElement(root, "#dialog-title"),
    modelAction: requireElement(root, "#model-action"),
    modelMessage: requireElement(root, "#model-message"),
    modelProgress: requireElement(root, "#model-progress"),
    modelSetup: requireElement(root, "#model-setup"),
    modelSetupTitle: requireElement(root, "#model-setup-title"),
    sessionPreviewContent: requireElement(root, "#session-preview-content"),
    sourceSelect: requireElement(root, "#audio-source"),
    spokenLanguage: requireElement(root, "#spoken-language"),
    statusLabel: requireElement(root, ".status-label"),
    statusText: requireElement(root, "#status-text"),
    titlebar: requireElement(root, ".titlebar"),
    translationAction: requireElement(root, "#translation-action"),
    translationMessage: requireElement(root, "#translation-message"),
    translationProgress: requireElement(root, "#translation-progress"),
    translationSetup: requireElement(root, "#translation-setup"),
    translationSetupTitle: requireElement(root, "#translation-setup-title"),
    translationTarget: requireElement(root, "#translation-target"),
    viewModeToggle: requireElement(root, "#view-mode-toggle"),
    visualToggle: requireElement(root, "#visual-toggle")
  };
}
