export const SPOKEN_LANGUAGES = [
  { code: "en", label: "English", tier: "ready", m2m100Code: "en" },
  { code: "ar", label: "Arabic", tier: "ready", m2m100Code: "ar" },
  { code: "bn", label: "Bengali", tier: "dedicated", m2m100Code: "bn" },
  { code: "bg", label: "Bulgarian", tier: "broad", m2m100Code: "bg" },
  { code: "zh", label: "Chinese", tier: "broad", m2m100Code: "zh" },
  { code: "hr", label: "Croatian", tier: "broad", m2m100Code: "hr" },
  { code: "cs", label: "Czech", tier: "broad", m2m100Code: "cs" },
  { code: "da", label: "Danish", tier: "broad", m2m100Code: "da" },
  { code: "nl", label: "Dutch", tier: "ready", m2m100Code: "nl" },
  { code: "et", label: "Estonian", tier: "broad", m2m100Code: "et" },
  { code: "fi", label: "Finnish", tier: "broad", m2m100Code: "fi" },
  { code: "fr", label: "French", tier: "ready", m2m100Code: "fr" },
  { code: "de", label: "German", tier: "ready", m2m100Code: "de" },
  { code: "hi", label: "Hindi", tier: "ready", m2m100Code: "hi" },
  { code: "hu", label: "Hungarian", tier: "broad", m2m100Code: "hu" },
  { code: "it", label: "Italian", tier: "ready", m2m100Code: "it" },
  { code: "ja", label: "Japanese", tier: "ready", m2m100Code: "ja" },
  { code: "ko", label: "Korean", tier: "ready", m2m100Code: "ko" },
  { code: "nb", label: "Norwegian Bokmål", tier: "broad", m2m100Code: "no" },
  { code: "pl", label: "Polish", tier: "broad", m2m100Code: "pl" },
  { code: "pt", label: "Portuguese", tier: "ready", m2m100Code: "pt" },
  { code: "ro", label: "Romanian", tier: "broad", m2m100Code: "ro" },
  { code: "ru", label: "Russian", tier: "ready", m2m100Code: "ru" },
  { code: "sk", label: "Slovak", tier: "broad", m2m100Code: "sk" },
  { code: "es", label: "Spanish", tier: "ready", m2m100Code: "es" },
  { code: "sv", label: "Swedish", tier: "broad", m2m100Code: "sv" },
  { code: "tr", label: "Turkish", tier: "ready", m2m100Code: "tr" },
  { code: "uk", label: "Ukrainian", tier: "ready", m2m100Code: "uk" },
  { code: "vi", label: "Vietnamese", tier: "ready", m2m100Code: "vi" }
] as const;

export type TranslationLanguage = (typeof SPOKEN_LANGUAGES)[number]["code"];

const LANGUAGE_BY_CODE = new Map<string, (typeof SPOKEN_LANGUAGES)[number]>(
  SPOKEN_LANGUAGES.map((language) => [language.code, language])
);

export function languageLabel(language: string): string {
  if (language === "auto") return "Automatic detection";
  return LANGUAGE_BY_CODE.get(language)?.label ?? language;
}

export function supportedTranslationLanguage(
  language: string
): TranslationLanguage | undefined {
  return LANGUAGE_BY_CODE.has(language) ? language as TranslationLanguage : undefined;
}

export function m2m100LanguageCode(language: TranslationLanguage): string {
  return LANGUAGE_BY_CODE.get(language)?.m2m100Code ?? language;
}
