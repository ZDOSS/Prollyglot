import type { AppViewMode } from "./app-store";
import { icons } from "./icons";

export interface AudioSourceCounts {
  playbackDevices: number;
  applications: number;
}

export interface GeneralSettingsActions {
  refreshAudioSources: () => Promise<AudioSourceCounts>;
  changeViewMode: (viewMode: AppViewMode) => Promise<void>;
}

export class GeneralSettingsPanel {
  private readonly idPrefix: string;

  constructor(idPrefix = "settings") {
    this.idPrefix = idPrefix;
  }

  render(
    content: HTMLElement,
    viewMode: AppViewMode,
    actions: GeneralSettingsActions
  ): void {
    const audioTitleId = `${this.idPrefix}-audio-title`;
    const privacyTitleId = `${this.idPrefix}-privacy-title`;
    const windowTitleId = `${this.idPrefix}-window-title`;
    content.className = "general-settings-content";
    content.innerHTML = `
      <section class="general-settings-section" aria-labelledby="${audioTitleId}">
        <div>
          <h3 id="${audioTitleId}">Audio sources</h3>
          <p>Refresh after opening or closing an audio-producing application or changing playback devices.</p>
        </div>
        <button class="secondary-button" data-refresh-audio-sources type="button">${icons.refresh}<span>Refresh sources</span></button>
        <p class="settings-inline-status" data-refresh-audio-result role="status" aria-live="polite"></p>
      </section>
      <section class="general-settings-section" aria-labelledby="${privacyTitleId}">
        <div>
          <h3 id="${privacyTitleId}">Privacy</h3>
          <p>Audio, screenshots, recognized text, captions, and translation remain local. Prollyglot does not save raw audio or captured frames.</p>
        </div>
        <span class="settings-value"><span class="status-dot"></span>Local processing</span>
      </section>
      <section class="general-settings-section" aria-labelledby="${windowTitleId}">
        <div>
          <h3 id="${windowTitleId}">Window layout</h3>
          <p>Use the full workspace for setup and management, or switch to the compact utility for everyday Start and Stop controls.</p>
        </div>
        <button class="secondary-button" data-settings-view-mode type="button">${viewMode === "full" ? icons.compact : icons.fullView}<span>${viewMode === "full" ? "Use compact view" : "Open full view"}</span></button>
      </section>
    `;

    const refresh = this.requireElement<HTMLButtonElement>(content, "[data-refresh-audio-sources]");
    const result = this.requireElement<HTMLElement>(content, "[data-refresh-audio-result]");
    refresh.addEventListener("click", () => {
      refresh.disabled = true;
      result.textContent = "Refreshing audio sources…";
      void actions.refreshAudioSources().then((counts) => {
        result.textContent = `Found ${counts.playbackDevices} playback ${counts.playbackDevices === 1 ? "device" : "devices"} and ${counts.applications} ${counts.applications === 1 ? "application" : "applications"}.`;
      }).catch((error: unknown) => {
        result.textContent = error instanceof Error ? error.message : String(error);
      }).finally(() => {
        refresh.disabled = false;
      });
    });
    this.requireElement<HTMLButtonElement>(content, "[data-settings-view-mode]")
      .addEventListener("click", () => {
        void actions.changeViewMode(viewMode === "full" ? "compact" : "full");
      });
  }

  private requireElement<T extends Element>(root: ParentNode, selector: string): T {
    const element = root.querySelector<T>(selector);
    if (!element) throw new Error(`missing general settings control: ${selector}`);
    return element;
  }
}
