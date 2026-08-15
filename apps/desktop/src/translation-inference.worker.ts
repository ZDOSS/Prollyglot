/// <reference lib="webworker" />

import { env, pipeline } from "@huggingface/transformers";
import wasmModuleUrl from "onnxruntime-web/ort-wasm-simd-threaded.mjs?url";
import wasmUrl from "onnxruntime-web/ort-wasm-simd-threaded.wasm?url";

import {
  TRANSLATION_CACHE_KEY,
  translationArtifactUrl,
  translationModelsForRoute,
  translationValidationUrl,
  type TranslationModelManifest
} from "./translation-catalog";
import { m2m100LanguageCode, type TranslationLanguage } from "./language-catalog";
import type {
  TranslationInferenceRequest,
  TranslationInferenceResponse
} from "./translation-protocol";
import type { TranslationCache } from "./translation-cache";
import { openTranslationReadCache } from "./translation-read-cache";

type Translator = ((text: string, options?: Record<string, unknown>) => Promise<Array<{
  translation_text: string;
}>>) & { dispose: () => void | Promise<void> };

const workerScope = self as unknown as DedicatedWorkerGlobalScope;
const CACHE_ONLY_LOCAL_PATH = "/__prollyglot_translation_cache_only__/";
const platformFetch = workerScope.fetch.bind(workerScope);
let operationQueue = Promise.resolve();
let cachePromise: Promise<TranslationCache> | undefined;
let configuredNativeBaseUrl: string | undefined;
let loaded: { model: TranslationModelManifest; translator: Translator } | undefined;

env.cacheKey = TRANSLATION_CACHE_KEY;
env.allowLocalModels = true;
env.allowRemoteModels = false;
env.localModelPath = CACHE_ONLY_LOCAL_PATH;
env.fetch = (input, init) => {
  if (typeof input === "string" && input.startsWith(CACHE_ONLY_LOCAL_PATH)) {
    return Promise.resolve(new Response(null, { status: 404, statusText: "Not Found" }));
  }
  return platformFetch(input, init);
};
env.useBrowserCache = false;
env.useFSCache = false;
env.useWasmCache = false;
const wasmBackend = env.backends.onnx.wasm;
if (!wasmBackend) throw new Error("The bundled ONNX WebAssembly runtime is unavailable.");
Object.assign(wasmBackend, {
  numThreads: 1,
  proxy: false,
  wasmPaths: {
    mjs: new URL(wasmModuleUrl, workerScope.location.href).href,
    wasm: new URL(wasmUrl, workerScope.location.href).href
  }
});

function post(message: TranslationInferenceResponse): void {
  workerScope.postMessage(message);
}

async function modelCache(nativeBaseUrl?: string): Promise<TranslationCache> {
  if (nativeBaseUrl && configuredNativeBaseUrl && nativeBaseUrl !== configuredNativeBaseUrl) {
    throw new Error("The native translation model origin changed during an inference session.");
  }
  configuredNativeBaseUrl ??= nativeBaseUrl;
  cachePromise ??= openTranslationReadCache(configuredNativeBaseUrl).then((cache) => {
    env.customCache = cache;
    env.useCustomCache = true;
    return cache;
  });
  return cachePromise;
}

async function installedModelForRoute(
  sourceLanguage: TranslationLanguage,
  targetLanguage: TranslationLanguage,
  nativeModelBaseUrl?: string
): Promise<TranslationModelManifest> {
  const cache = await modelCache(nativeModelBaseUrl);
  const candidates = translationModelsForRoute(sourceLanguage, targetLanguage);
  for (const model of candidates) {
    const verification = await cache.match(translationValidationUrl(model));
    if (!verification) continue;
    // The native verification endpoint revalidates the manifest marker before
    // answering. Avoid opening every artifact merely to prove that it exists;
    // the bounded protocol will still reject a changed or missing file when
    // Transformers.js requests it. Legacy cache entries retain the explicit
    // per-artifact completeness check below.
    if (verification.headers.get("x-prollyglot-storage") === "native") return model;
    let complete = true;
    for (const artifact of model.artifacts) {
      if (!(await cache.match(translationArtifactUrl(model, artifact)))) {
        complete = false;
        break;
      }
    }
    if (complete) return model;
  }
  const preferred = candidates[0];
  if (!preferred) {
    throw new Error(`No local translator supports ${sourceLanguage} to ${targetLanguage}.`);
  }
  throw new Error(`${preferred.displayName} is not installed and ready.`);
}

async function disposeLoaded(): Promise<void> {
  const current = loaded;
  loaded = undefined;
  await current?.translator.dispose();
}

async function prepare(
  sourceLanguage: TranslationLanguage,
  targetLanguage: TranslationLanguage,
  nativeModelBaseUrl?: string
): Promise<{ modelId: string; coldStartMs: number }> {
  const model = await installedModelForRoute(
    sourceLanguage,
    targetLanguage,
    nativeModelBaseUrl
  );
  if (loaded?.model.modelId === model.modelId) {
    return { modelId: model.modelId, coldStartMs: 0 };
  }
  await disposeLoaded();
  const startedAt = performance.now();
  const translator = await pipeline("translation", model.modelId, {
    revision: model.revision,
    dtype: "q8",
    device: "wasm",
    local_files_only: true,
    // ORT 1.26's extended QDQ rewrite rejects these Marian q8 exports even
    // though the original graph is valid. Keep the verified graph unchanged.
    session_options: { graphOptimizationLevel: "disabled" }
  }) as unknown as Translator;
  loaded = { model, translator };
  return {
    modelId: model.modelId,
    coldStartMs: Math.round(performance.now() - startedAt)
  };
}

async function translate(
  sourceLanguage: TranslationLanguage,
  targetLanguage: TranslationLanguage,
  text: string,
  nativeModelBaseUrl?: string
): Promise<string> {
  const trimmed = text.trim();
  if (!trimmed) return "";
  const prepared = await prepare(sourceLanguage, targetLanguage, nativeModelBaseUrl);
  if (!loaded || loaded.model.modelId !== prepared.modelId) {
    throw new Error("The local translator changed while preparing inference.");
  }
  const routeOptions = loaded.model.kind === "manyToMany"
    ? {
        src_lang: m2m100LanguageCode(sourceLanguage),
        tgt_lang: m2m100LanguageCode(targetLanguage)
      }
    : {};
  const output = await loaded.translator(trimmed, {
    max_new_tokens: maximumTranslationTokens(trimmed),
    num_beams: 1,
    ...routeOptions
  });
  const translated = output[0]?.translation_text?.trim();
  if (!translated) throw new Error("The local translator returned no text.");
  return translated;
}

function maximumTranslationTokens(text: string): number {
  const compactScript = /[\u3040-\u30ff\u3400-\u9fff\uac00-\ud7af]/u.test(text);
  const characters = [...text].filter((character) => !/\s/u.test(character)).length;
  const approximateSourceTokens = compactScript ? characters : Math.ceil(characters / 3);
  return Math.max(24, Math.min(192, approximateSourceTokens * 3 + 12));
}

async function handleRequest(request: TranslationInferenceRequest): Promise<void> {
  try {
    if (request.type === "prepare") {
      const result = await prepare(
        request.sourceLanguage,
        request.targetLanguage,
        request.nativeModelBaseUrl
      );
      post({ type: "ready", requestId: request.requestId, ok: true, ...result });
      return;
    }
    const result = await translate(
      request.sourceLanguage,
      request.targetLanguage,
      request.text,
      request.nativeModelBaseUrl
    );
    post({ type: "reply", requestId: request.requestId, ok: true, result });
  } catch (error) {
    post({
      type: request.type === "prepare" ? "ready" : "reply",
      requestId: request.requestId,
      ok: false,
      error: error instanceof Error ? error.message : String(error)
    });
  }
}

workerScope.addEventListener("message", ({ data }: MessageEvent<TranslationInferenceRequest>) => {
  operationQueue = operationQueue.then(() => handleRequest(data));
});
