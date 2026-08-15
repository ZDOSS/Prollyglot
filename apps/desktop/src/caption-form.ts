import {
  SPOKEN_LANGUAGES,
  languageLabel,
  supportedTranslationLanguage
} from "./language-catalog.ts";
import type {
  ApplicationSource,
  AudioSourcePreference,
  CaptionOutputMode,
  CaptureSelection,
  SourceSnapshot
} from "./types.ts";

export const FOLLOW_SYSTEM_DEFAULT = "__follow-system-default__";

export interface CaptionFormElements {
  source: HTMLSelectElement;
  device: HTMLSelectElement;
  deviceField: HTMLElement;
  spokenLanguage: HTMLSelectElement;
  spokenLanguageHelp: HTMLElement;
  translationTarget: HTMLSelectElement;
  translationTargetHelp: HTMLElement;
  captionOutput: HTMLSelectElement;
  captionOutputHelp: HTMLElement;
}

export interface TranslationTargetPlan {
  disabled: boolean;
  options: Array<{ value: string; label: string }>;
  selected: string;
  help: string;
}

export interface CaptionOutputPlan {
  disabled: boolean;
  options: Array<{ value: CaptionOutputMode; label: string }>;
  selected: CaptionOutputMode;
  help: string;
}

export interface ApplicationSourceOptionPlan {
  value: string;
  label: string;
  disabled: boolean;
}

export function planApplicationSourceOptions(
  applications: ApplicationSource[]
): ApplicationSourceOptionPlan[] {
  return applications.map((application) => ({
    value: `application:${application.id}`,
    label: application.instanceCount > 1
      ? `${application.name} · close duplicate instances`
      : `Only ${application.name}`,
    disabled: application.instanceCount > 1
  }));
}

export function planTranslationTargets(
  source: string,
  preferredTarget: string
): TranslationTargetPlan {
  const sourceLanguage = supportedTranslationLanguage(source);
  if (!sourceLanguage) {
    return {
      disabled: true,
      options: [{ value: "off", label: "Off · original language" }],
      selected: "off",
      help: "Automatic recognition does not yet report a stable source language for translation."
    };
  }
  const options = [
    { value: "off", label: "Off · original language" },
    ...SPOKEN_LANGUAGES
      .filter(({ code }) => code !== sourceLanguage)
      .map(({ code, label }) => ({ value: code, label }))
  ];
  const selected = options.some(({ value }) => value === preferredTarget)
    ? preferredTarget
    : sourceLanguage === "en" ? "off" : "en";
  return {
    disabled: false,
    options,
    selected,
    help: selected === "off"
      ? "Recognition stays local and captions remain in the spoken language."
      : `Translation to ${languageLabel(selected)} runs locally.`
  };
}

export function planCaptionOutput(
  source: string,
  target: string,
  preferredMode: CaptionOutputMode,
  translatorPhase: string | undefined
): CaptionOutputPlan {
  const sourceLanguage = supportedTranslationLanguage(source);
  const targetLanguage = supportedTranslationLanguage(target);
  const routeAvailable = Boolean(
    sourceLanguage && targetLanguage && sourceLanguage !== targetLanguage
  );
  if (!routeAvailable || !targetLanguage) {
    return {
      disabled: true,
      options: [{ value: "original", label: "Original language" }],
      selected: "original",
      help: !sourceLanguage
        ? "Choose a specific spoken language to enable local translation."
        : "Choose a Translate to language to enable translated captions."
    };
  }

  const targetLabel = languageLabel(targetLanguage);
  const selected = preferredMode;
  let help: string;
  if (selected === "original") {
    help = translatorPhase === "ready"
      ? `Translation is off. Choose ${targetLabel} only or Original + ${targetLabel} to use the installed translator.`
      : `Translation is off. Choose ${targetLabel} only or Original + ${targetLabel} to install a translator.`;
  } else if (translatorPhase === "ready") {
    help = `${targetLabel} starts from live partial speech and is corrected again when each caption finalizes.`;
  } else if (translatorPhase === "loading") {
    help = `Original captions stay live while the local ${targetLabel} translator loads.`;
  } else {
    help = `Original captions stay live until the optional ${targetLabel} translator is installed.`;
  }
  return {
    disabled: false,
    options: [
      { value: "original", label: "Original only · translation off" },
      { value: "translated", label: `${targetLabel} only · translated` },
      { value: "both", label: `Original + ${targetLabel}` }
    ],
    selected,
    help
  };
}

function option(value: string, label: string, selected = false): HTMLOptionElement {
  const element = document.createElement("option");
  element.value = value;
  element.textContent = label;
  element.selected = selected;
  return element;
}

export class CaptionForm {
  private readonly elements: CaptionFormElements;

  constructor(elements: CaptionFormElements) {
    this.elements = elements;
  }

  populateSpokenLanguages(selected: string): void {
    this.elements.spokenLanguage.replaceChildren(
      ...SPOKEN_LANGUAGES.map(({ code, label }) => option(code, label, code === "en")),
      option("auto", "Automatic · mixed languages")
    );
    this.elements.spokenLanguage.value = selected;
  }

  populateTranslationTargets(preferredTarget: string): TranslationTargetPlan {
    const plan = planTranslationTargets(this.elements.spokenLanguage.value, preferredTarget);
    this.elements.translationTarget.disabled = plan.disabled;
    this.elements.translationTarget.replaceChildren(
      ...plan.options.map(({ value, label }) => option(value, label, value === plan.selected))
    );
    this.elements.translationTarget.value = plan.selected;
    this.elements.translationTargetHelp.textContent = plan.help;
    return plan;
  }

  renderCaptionOutput(plan: CaptionOutputPlan): void {
    this.elements.captionOutput.replaceChildren(
      ...plan.options.map(({ value, label }) => option(value, label, value === plan.selected))
    );
    this.elements.captionOutput.value = plan.selected;
    this.elements.captionOutput.disabled = plan.disabled;
    this.elements.captionOutputHelp.textContent = plan.help;
  }

  populateSources(snapshot: SourceSnapshot, configured: AudioSourcePreference): void {
    const previousSource = this.elements.source.value;
    const previousDevice = this.elements.device.value || (
      configured.kind === "playbackDevice" ? configured.deviceId : FOLLOW_SYSTEM_DEFAULT
    );
    this.elements.source.replaceChildren(option("system", "Everything I hear"));
    for (const application of planApplicationSourceOptions(snapshot.applications)) {
      const applicationOption = option(application.value, application.label);
      applicationOption.disabled = application.disabled;
      this.elements.source.append(applicationOption);
    }
    if ([...this.elements.source.options].some(({ value }) => value === previousSource)) {
      this.elements.source.value = previousSource;
    }

    const defaultDevice = snapshot.playbackDevices.find(({ isDefault }) => isDefault);
    const followLabel = defaultDevice
      ? `Follow system default — ${defaultDevice.name}`
      : "Follow system default";
    this.elements.device.replaceChildren(
      option(
        FOLLOW_SYSTEM_DEFAULT,
        followLabel,
        !previousDevice || previousDevice === FOLLOW_SYSTEM_DEFAULT
      )
    );
    for (const device of snapshot.playbackDevices) {
      const label = device.isDefault ? `${device.name} — Pin current default` : device.name;
      this.elements.device.append(option(device.id, label, device.id === previousDevice));
    }
    if (![...this.elements.device.options].some(({ value }) => value === previousDevice)) {
      this.elements.device.value = FOLLOW_SYSTEM_DEFAULT;
    }
    this.updateSourceMode();
  }

  updateSourceMode(): void {
    this.elements.deviceField.hidden = this.elements.source.value !== "system";
  }

  selectedCapture(): CaptureSelection {
    if (this.elements.source.value === "system") {
      if (!this.elements.device.value) throw new Error("No playback device is available.");
      if (this.elements.device.value === FOLLOW_SYSTEM_DEFAULT) return { kind: "systemDefault" };
      return { kind: "systemOutput", deviceId: this.elements.device.value };
    }
    const sourceId = this.elements.source.value.slice("application:".length);
    if (!sourceId) throw new Error("The selected application is unavailable.");
    return { kind: "application", sourceId };
  }

  renderLanguageGuidance(): void {
    const selected = this.elements.spokenLanguage.value;
    const language = SPOKEN_LANGUAGES.find(({ code }) => code === selected);
    this.elements.spokenLanguageHelp.textContent = selected === "auto"
      ? "For mixed-language audio. Detection can add delay or choose the wrong language."
      : language?.tier === "broad"
        ? "Supported by Nemotron's broad-coverage tier; accuracy can vary more than its primary languages."
        : "Choosing the language guides recognition and usually improves accuracy.";
  }
}

export function captionActionCopy(language: string): string {
  return language === "auto"
    ? "detect and caption the spoken language"
    : `caption ${languageLabel(language)} speech`;
}
