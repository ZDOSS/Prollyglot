import type { TranslationCatalogStatus } from "./types";
import type { TranslationLanguage } from "./language-catalog";

export type TranslationSourceLanguage = TranslationLanguage;

export type TranslationWorkerRequest =
  | { type: "status"; requestId: number }
  | { type: "install"; requestId: number; modelId: string }
  | { type: "remove"; requestId: number; modelId: string }
  | {
      type: "prepare";
      requestId: number;
      sourceLanguage: TranslationLanguage;
      targetLanguage: TranslationLanguage;
    }
  | {
      type: "translate";
      requestId: number;
      sourceLanguage: TranslationLanguage;
      targetLanguage: TranslationLanguage;
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
