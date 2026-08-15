import type { TranslationLanguage } from "./language-catalog";
import type { TranslationCatalogStatus } from "./types";

export type TranslationControlRequest =
  | { type: "status"; requestId: number }
  | { type: "install"; requestId: number; modelId: string }
  | { type: "remove"; requestId: number; modelId: string };

export type TranslationControlCommand = TranslationControlRequest extends infer Request
  ? Request extends { requestId: number }
    ? Omit<Request, "requestId">
    : never
  : never;

export type TranslationControlResponse =
  | { type: "catalog"; catalog: TranslationCatalogStatus }
  | { type: "reply"; requestId: number; ok: true; result?: unknown }
  | { type: "reply"; requestId: number; ok: false; error: string };

export type TranslationInferenceRequest =
  | {
      type: "prepare";
      requestId: number;
      sourceLanguage: TranslationLanguage;
      targetLanguage: TranslationLanguage;
      nativeModelBaseUrl?: string;
    }
  | {
      type: "translate";
      requestId: number;
      sourceLanguage: TranslationLanguage;
      targetLanguage: TranslationLanguage;
      text: string;
      nativeModelBaseUrl?: string;
    };

export type TranslationInferenceCommand = TranslationInferenceRequest extends infer Request
  ? Request extends { requestId: number }
    ? Omit<Request, "requestId">
    : never
  : never;

export type TranslationInferenceResponse =
  | {
      type: "ready";
      requestId: number;
      ok: true;
      modelId: string;
      coldStartMs: number;
    }
  | { type: "reply"; requestId: number; ok: true; result?: unknown }
  | { type: "reply" | "ready"; requestId: number; ok: false; error: string };
