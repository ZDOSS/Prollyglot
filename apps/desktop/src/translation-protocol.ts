import type { TranslationCatalogStatus } from "./types";

export type TranslationSourceLanguage = "es" | "ja";

export type TranslationWorkerRequest =
  | { type: "status"; requestId: number }
  | { type: "install"; requestId: number; sourceLanguage: TranslationSourceLanguage }
  | { type: "remove"; requestId: number; sourceLanguage: TranslationSourceLanguage }
  | {
      type: "translate";
      requestId: number;
      sourceLanguage: TranslationSourceLanguage;
      text: string;
    };

export type TranslationWorkerCommand = TranslationWorkerRequest extends infer Request
  ? Request extends { requestId: number }
    ? Omit<Request, "requestId">
    : never
  : never;

export type TranslationWorkerResponse =
  | { type: "catalog"; catalog: TranslationCatalogStatus }
  | { type: "reply"; requestId: number; ok: true; result?: unknown }
  | { type: "reply"; requestId: number; ok: false; error: string };
