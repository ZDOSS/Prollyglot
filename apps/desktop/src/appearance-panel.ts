import { updateOverlaySettings } from "./bridge";
import { icons } from "./icons";
import { DEFAULT_OVERLAY_SETTINGS, type OverlaySettings } from "./types";

export interface AppearancePanelOptions {
  showHeading?: boolean;
  doneLabel?: string;
  onDone?: () => void | Promise<void>;
}

export class AppearancePanel {
  private container?: HTMLElement;
  private options: AppearancePanelOptions = {};

  render(container: HTMLElement, options: AppearancePanelOptions = {}): void {
    this.container = container;
    this.options = options;
    container.className = "appearance-panel-host";
    container.innerHTML = `
      <div class="appearance-layout">
        <section class="appearance-controls" aria-label="Caption appearance controls">
          ${options.showHeading ? '<h1 id="appearance-title">Appearance</h1>' : ""}
          <h2>Caption style</h2>

          ${this.selectRow("Font", "font-family", [
            ['"Segoe UI Variable", "Segoe UI", sans-serif', "Segoe UI"],
            ['Inter, "Segoe UI", sans-serif', "Inter"],
            ["Arial, sans-serif", "Arial"]
          ])}
          ${this.selectRow("Size", "font-size", [
            ["28", "28 px"], ["36", "36 px"], ["44", "44 px"], ["56", "56 px"]
          ])}

          <label class="setting-row">
            <span>Original color</span>
            <input id="text-color" class="color-control" type="color" aria-label="Original caption color" />
          </label>
          <label class="setting-row">
            <span>Translation color</span>
            <input id="translated-text-color" class="color-control" type="color" aria-label="Translated caption color" />
          </label>

          ${this.selectRow("Bilingual layout", "bilingual-layout", [
            ["stacked", "Stacked"], ["sideBySide", "Side by side"]
          ])}
          <p class="appearance-help">Choose <strong>Original + translation</strong> on the Captions page to use this layout.</p>

          <label class="range-setting">
            <span class="range-label"><span>Background opacity</span><output id="opacity-output">75%</output></span>
            <input id="background-opacity" type="range" min="0" max="100" step="5" aria-label="Background opacity" />
          </label>

          ${this.selectRow("Width", "overlay-width", [
            ["520", "520 px"], ["720", "720 px"], ["920", "920 px"]
          ])}
          ${this.selectRow("Caption history", "maximum-lines", [
            ["1", "Current only"], ["2", "1 previous line"], ["3", "2 previous lines"], ["4", "3 previous lines"]
          ])}
          <p class="appearance-help">Recent finalized captions fade above the current line. Long current captions can use the extra wrapping space first.</p>

          ${this.selectRow("Keep after speech", "reading-time", [
            ["6", "6 seconds"], ["10", "10 seconds"], ["15", "15 seconds"], ["30", "30 seconds"]
          ])}
          ${this.selectRow("Fade out", "fade-duration", [
            ["0", "Instant"], ["400", "Quick · 0.4 sec"], ["800", "Gentle · 0.8 sec"], ["1500", "Slow · 1.5 sec"]
          ])}
          <p class="appearance-help">Reading time restarts when a delayed translation arrives, so it receives the full selected time.</p>

          ${this.selectRow("Position", "overlay-position", [
            ["bottomCenter", "Bottom center"], ["topCenter", "Top center"],
            ["bottomLeft", "Bottom left"], ["bottomRight", "Bottom right"]
          ])}

          <label class="setting-row toggle-row">
            <span>Click-through</span>
            <input id="click-through" class="toggle-input" type="checkbox" aria-label="Click-through" />
            <span class="toggle-visual" aria-hidden="true"><span></span></span>
          </label>
        </section>

        <section class="preview-canvas" aria-label="Original and translated caption appearance preview">
          <div class="preview-desktop" id="preview-desktop">
            <div class="preview-caption" id="preview-caption">
              ${this.previewEntry("昨日から雨が続いています。", "It has been raining since yesterday.")}
              ${this.previewEntry("午後には晴れる見込みです。", "It should clear this afternoon.")}
              ${this.previewEntry("電車は通常どおり運行しています。", "Trains are running normally.")}
              ${this.previewEntry("今日は何をする予定ですか？", "What are you planning to do today?")}
            </div>
            <div class="preview-taskbar" aria-hidden="true"><span class="windows-mark">⊞</span><span class="taskbar-spacer"></span><span>10:28 AM</span></div>
          </div>
        </section>
      </div>

      <footer class="appearance-actions">
        <button data-appearance-reset class="secondary-button" type="button">Reset to defaults</button>
        ${options.onDone ? `<button data-appearance-done class="primary-button compact-primary" type="button">${options.doneLabel ?? "Done"}</button>` : '<span class="appearance-live-note">Changes apply immediately</span>'}
      </footer>
    `;

    for (const control of this.all<HTMLInputElement | HTMLSelectElement>(
      ".appearance-controls input, .appearance-controls select"
    )) {
      control.addEventListener("input", () => this.apply(this.readSettings()));
    }
    this.required<HTMLButtonElement>("[data-appearance-reset]").addEventListener("click", () => {
      this.writeSettings({ ...DEFAULT_OVERLAY_SETTINGS });
    });
    this.query<HTMLButtonElement>("[data-appearance-done]")?.addEventListener("click", () => {
      void this.finish();
    });
    this.writeSettings(this.readStoredSettings());
  }

  settings(): OverlaySettings {
    return this.readSettings();
  }

  private selectRow(label: string, id: string, options: Array<[string, string]>): string {
    return `<label class="setting-row">
      <span>${label}</span>
      <span class="compact-select-wrap">
        <select id="${id}">${options.map(([value, copy]) => `<option value='${value}'>${copy}</option>`).join("")}</select>
        ${icons.chevronDown}
      </span>
    </label>`;
  }

  private previewEntry(original: string, translation: string): string {
    return `<span class="preview-caption-entry">
      <span class="preview-caption-original" lang="ja">${original}</span>
      <span class="preview-caption-translation" lang="en">${translation}</span>
    </span>`;
  }

  private readStoredSettings(): OverlaySettings {
    const stored = localStorage.getItem("prollyglot.overlay");
    if (!stored) return { ...DEFAULT_OVERLAY_SETTINGS };
    try {
      return { ...DEFAULT_OVERLAY_SETTINGS, ...(JSON.parse(stored) as Partial<OverlaySettings>) };
    } catch {
      return { ...DEFAULT_OVERLAY_SETTINGS };
    }
  }

  private readSettings(): OverlaySettings {
    return {
      fontFamily: this.required<HTMLSelectElement>("#font-family").value,
      fontSize: Number(this.required<HTMLSelectElement>("#font-size").value),
      textColor: this.required<HTMLInputElement>("#text-color").value,
      translatedTextColor: this.required<HTMLInputElement>("#translated-text-color").value,
      bilingualLayout: this.required<HTMLSelectElement>("#bilingual-layout").value as OverlaySettings["bilingualLayout"],
      backgroundOpacity: Number(this.required<HTMLInputElement>("#background-opacity").value) / 100,
      width: Number(this.required<HTMLSelectElement>("#overlay-width").value),
      maximumLines: Number(this.required<HTMLSelectElement>("#maximum-lines").value),
      readingTimeSeconds: Number(this.required<HTMLSelectElement>("#reading-time").value),
      fadeDurationMs: Number(this.required<HTMLSelectElement>("#fade-duration").value),
      position: this.required<HTMLSelectElement>("#overlay-position").value as OverlaySettings["position"],
      clickThrough: this.required<HTMLInputElement>("#click-through").checked
    };
  }

  private writeSettings(settings: OverlaySettings): void {
    this.writeSelect("#font-family", settings.fontFamily, DEFAULT_OVERLAY_SETTINGS.fontFamily);
    this.writeSelect("#font-size", settings.fontSize, DEFAULT_OVERLAY_SETTINGS.fontSize);
    this.required<HTMLInputElement>("#text-color").value = settings.textColor;
    this.required<HTMLInputElement>("#translated-text-color").value = settings.translatedTextColor;
    this.writeSelect("#bilingual-layout", settings.bilingualLayout, DEFAULT_OVERLAY_SETTINGS.bilingualLayout);
    this.required<HTMLInputElement>("#background-opacity").value = String(settings.backgroundOpacity * 100);
    this.writeSelect("#overlay-width", settings.width, DEFAULT_OVERLAY_SETTINGS.width);
    this.writeSelect("#maximum-lines", settings.maximumLines, DEFAULT_OVERLAY_SETTINGS.maximumLines);
    this.writeSelect("#reading-time", settings.readingTimeSeconds, DEFAULT_OVERLAY_SETTINGS.readingTimeSeconds);
    this.writeSelect("#fade-duration", settings.fadeDurationMs, DEFAULT_OVERLAY_SETTINGS.fadeDurationMs);
    this.writeSelect("#overlay-position", settings.position, DEFAULT_OVERLAY_SETTINGS.position);
    this.required<HTMLInputElement>("#click-through").checked = settings.clickThrough;
    this.apply(this.readSettings());
  }

  private writeSelect(selector: string, value: string | number, fallback: string | number): void {
    const select = this.required<HTMLSelectElement>(selector);
    select.value = String(value);
    if (!select.value) select.value = String(fallback);
  }

  private apply(settings: OverlaySettings): void {
    const preview = this.required<HTMLElement>("#preview-caption");
    this.required<HTMLOutputElement>("#opacity-output").value = `${Math.round(settings.backgroundOpacity * 100)}%`;
    preview.style.fontFamily = settings.fontFamily;
    preview.style.fontSize = `${Math.max(18, settings.fontSize * 0.72)}px`;
    preview.style.backgroundColor = `rgba(11, 15, 18, ${settings.backgroundOpacity})`;
    preview.style.maxWidth = `${Math.min(720, settings.width * 0.78)}px`;
    preview.style.setProperty("--maximum-lines", String(settings.maximumLines));
    preview.style.setProperty("--source-caption-color", settings.textColor);
    preview.style.setProperty("--translated-caption-color", settings.translatedTextColor);
    preview.dataset.bilingualLayout = settings.bilingualLayout;
    const entries = [...preview.querySelectorAll<HTMLElement>(".preview-caption-entry")];
    const firstVisible = Math.max(0, entries.length - settings.maximumLines);
    entries.forEach((entry, index) => {
      entry.hidden = index < firstVisible;
      entry.dataset.historyDepth = String(entries.length - index - 1);
    });
    this.required<HTMLElement>("#preview-desktop").dataset.position = settings.position;
    void updateOverlaySettings(settings);
  }

  private async finish(): Promise<void> {
    const button = this.query<HTMLButtonElement>("[data-appearance-done]");
    if (button) button.disabled = true;
    try {
      await updateOverlaySettings(this.readSettings());
      await this.options.onDone?.();
    } finally {
      if (button) button.disabled = false;
    }
  }

  private query<T extends Element>(selector: string): T | null {
    return this.container?.querySelector<T>(selector) ?? null;
  }

  private required<T extends Element>(selector: string): T {
    const element = this.query<T>(selector);
    if (!element) throw new Error(`missing appearance element: ${selector}`);
    return element;
  }

  private all<T extends Element>(selector: string): NodeListOf<T> {
    if (!this.container) throw new Error("Appearance is not mounted.");
    return this.container.querySelectorAll<T>(selector);
  }
}
