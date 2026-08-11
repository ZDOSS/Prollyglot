/// <reference lib="webworker" />

import { env, pipeline } from "@huggingface/transformers";
import { sha256 as sha256Hash } from "@noble/hashes/sha2.js";
import { bytesToHex } from "@noble/hashes/utils.js";
import wasmModuleUrl from "onnxruntime-web/ort-wasm-simd-threaded.mjs?url";
import wasmUrl from "onnxruntime-web/ort-wasm-simd-threaded.wasm?url";

import {
  TRANSLATION_CACHE_KEY,
  TRANSLATION_MODELS,
  translationArtifactUrl,
  translationModelById,
  translationModelsForRoute,
  translationValidationUrl,
  type TranslationArtifact,
  type TranslationModelManifest
} from "./translation-catalog";
import { m2m100LanguageCode, type TranslationLanguage } from "./language-catalog";
import type {
  TranslationWorkerRequest,
  TranslationWorkerResponse
} from "./translation-protocol";
import { openTranslationCache, type TranslationCache } from "./translation-cache";
import type { TranslationCatalogStatus, TranslationModelStatus } from "./types";

type Translator = ((text: string, options?: Record<string, unknown>) => Promise<Array<{
  translation_text: string;
}>>) & { dispose: () => void | Promise<void> };

class IntegrityError extends Error {}

const workerScope = self as unknown as DedicatedWorkerGlobalScope;
const CACHE_ONLY_LOCAL_PATH = "/__prollyglot_translation_cache_only__/";
const platformFetch = workerScope.fetch.bind(workerScope);
const statuses = new Map<string, TranslationModelStatus>();
const translators = new Map<string, Translator>();
let operationQueue = Promise.resolve();
let lastProgressPublishedAt = 0;
let cachePromise: Promise<TranslationCache> | undefined;

for (const model of TRANSLATION_MODELS) {
  statuses.set(model.modelId, {
    phase: "checking",
    kind: model.kind,
    sourceLanguages: [...model.sourceLanguages],
    targetLanguages: [...model.targetLanguages],
    modelId: model.modelId,
    displayName: model.displayName,
    license: model.license,
    downloadedBytes: 0,
    totalBytes: model.totalBytes,
    message: "Checking local model files…"
  });
}

env.cacheKey = TRANSLATION_CACHE_KEY;
// Transformers.js requires the local-model path to be enabled even when every
// artifact is served from our verified custom cache. Remote loading stays off,
// and pipeline creation also sets local_files_only so a cache miss cannot fetch.
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

function catalogSnapshot(): TranslationCatalogStatus {
  return {
    models: TRANSLATION_MODELS.map((model) => ({ ...requiredStatus(model.modelId) }))
  };
}

function publishCatalog(force = true): void {
  const now = performance.now();
  if (!force && now - lastProgressPublishedAt < 120) return;
  lastProgressPublishedAt = now;
  post({ type: "catalog", catalog: catalogSnapshot() });
}

function post(message: TranslationWorkerResponse): void {
  workerScope.postMessage(message);
}

function requiredStatus(modelId: string): TranslationModelStatus {
  const status = statuses.get(modelId);
  if (!status) throw new Error(`Unknown local translation model ${modelId}.`);
  return status;
}

function setStatus(
  modelId: string,
  patch: Partial<TranslationModelStatus>,
  force = true
): void {
  Object.assign(requiredStatus(modelId), patch);
  publishCatalog(force);
}

async function modelCache(): Promise<TranslationCache> {
  cachePromise ??= openTranslationCache().then((cache) => {
    env.customCache = cache;
    env.useCustomCache = true;
    return cache;
  });
  return cachePromise;
}

async function hasVerifiedInstall(
  cache: TranslationCache,
  model: TranslationModelManifest
): Promise<boolean> {
  if (!(await cache.match(translationValidationUrl(model)))) return false;
  for (const artifact of model.artifacts) {
    if (!(await cache.match(translationArtifactUrl(model, artifact)))) return false;
  }
  return true;
}

async function inspectCatalog(): Promise<TranslationCatalogStatus> {
  try {
    const cache = await modelCache();
    for (const model of TRANSLATION_MODELS) {
      const ready = await hasVerifiedInstall(cache, model);
      Object.assign(requiredStatus(model.modelId), {
        phase: ready ? "ready" : "notInstalled",
        downloadedBytes: ready ? model.totalBytes : 0,
        message: undefined
      } satisfies Partial<TranslationModelStatus>);
    }
  } catch (error) {
    const message = errorMessage(error);
    for (const model of TRANSLATION_MODELS) {
      Object.assign(requiredStatus(model.modelId), {
        phase: "failed",
        downloadedBytes: 0,
        message
      } satisfies Partial<TranslationModelStatus>);
    }
  }
  publishCatalog();
  return catalogSnapshot();
}

async function sha256(bytes: Uint8Array): Promise<string> {
  if (workerScope.crypto?.subtle) {
    const digest = await workerScope.crypto.subtle.digest("SHA-256", bytes.buffer as ArrayBuffer);
    return bytesToHex(new Uint8Array(digest));
  }
  return bytesToHex(sha256Hash(bytes));
}

async function verifiedBytes(
  response: Response,
  artifact: TranslationArtifact,
  onProgress?: (loaded: number) => void
): Promise<Uint8Array> {
  if (!response.body) {
    const bytes = new Uint8Array(await response.arrayBuffer());
    onProgress?.(bytes.byteLength);
    await verifyArtifact(bytes, artifact);
    return bytes;
  }

  const reader = response.body.getReader();
  const bytes = new Uint8Array(artifact.size);
  let offset = 0;
  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    if (offset + value.byteLength > bytes.byteLength) {
      await reader.cancel();
      throw new IntegrityError(`${artifact.path} is larger than its signed manifest entry.`);
    }
    bytes.set(value, offset);
    offset += value.byteLength;
    onProgress?.(offset);
  }
  if (offset !== bytes.byteLength) {
    throw new IntegrityError(
      `${artifact.path} is incomplete (${offset.toLocaleString()} of ${artifact.size.toLocaleString()} bytes).`
    );
  }
  await verifyArtifact(bytes, artifact);
  return bytes;
}

async function verifyArtifact(bytes: Uint8Array, artifact: TranslationArtifact): Promise<void> {
  if (bytes.byteLength !== artifact.size) {
    throw new IntegrityError(
      `${artifact.path} has an unexpected size (${bytes.byteLength.toLocaleString()} bytes).`
    );
  }
  const digest = await sha256(bytes);
  if (digest !== artifact.sha256) {
    throw new IntegrityError(`${artifact.path} failed its SHA-256 integrity check.`);
  }
}

async function cachedArtifactIsValid(
  cache: TranslationCache,
  model: TranslationModelManifest,
  artifact: TranslationArtifact
): Promise<boolean> {
  const url = translationArtifactUrl(model, artifact);
  const cached = await cache.match(url);
  if (!cached) return false;
  try {
    await verifiedBytes(cached, artifact);
    return true;
  } catch {
    await cache.delete(url);
    return false;
  }
}

async function install(modelId: string): Promise<void> {
  const model = translationModelById(modelId);
  if (!model) throw new Error(`Unknown local translation model ${modelId}.`);
  const cache = await modelCache();
  await cache.delete(translationValidationUrl(model));
  setStatus(modelId, {
    phase: "downloading",
    downloadedBytes: 0,
    message: "Checking cached model files…"
  });

  let completedBytes = 0;
  try {
    for (const artifact of model.artifacts) {
      if (await cachedArtifactIsValid(cache, model, artifact)) {
        completedBytes += artifact.size;
        setStatus(modelId, {
          downloadedBytes: completedBytes,
          message: `Verified ${artifact.path}`
        });
        continue;
      }

      const url = translationArtifactUrl(model, artifact);
      setStatus(modelId, {
        downloadedBytes: completedBytes,
        message: `Downloading ${artifact.path}…`
      });
      const response = await fetch(url);
      if (!response.ok) {
        throw new Error(`Could not download ${artifact.path} (HTTP ${response.status}).`);
      }
      const bytes = await verifiedBytes(response, artifact, (loaded) => {
        setStatus(modelId, {
          downloadedBytes: completedBytes + loaded,
          message: `Downloading and verifying ${artifact.path}…`
        }, false);
      });
      const contentType = response.headers.get("content-type") ?? "application/octet-stream";
      await cache.put(url, new Response(bytes.buffer as ArrayBuffer, {
        headers: {
          "content-length": String(bytes.byteLength),
          "content-type": contentType
        }
      }));
      completedBytes += artifact.size;
      setStatus(modelId, {
        downloadedBytes: completedBytes,
        message: `Verified ${artifact.path}`
      });
    }

    await cache.put(translationValidationUrl(model), new Response(JSON.stringify({
      modelId: model.modelId,
      revision: model.revision,
      verifiedAt: new Date().toISOString()
    }), { headers: { "content-type": "application/json" } }));
    setStatus(modelId, {
      phase: "ready",
      downloadedBytes: model.totalBytes,
      message: undefined
    });
  } catch (error) {
    const corrupt = error instanceof IntegrityError;
    setStatus(modelId, {
      phase: corrupt ? "corrupt" : "failed",
      downloadedBytes: completedBytes,
      message: errorMessage(error)
    });
    throw error;
  }
}

async function disposeTranslator(modelId: string): Promise<void> {
  const translator = translators.get(modelId);
  if (!translator) return;
  translators.delete(modelId);
  await translator.dispose();
}

async function remove(modelId: string): Promise<void> {
  const model = translationModelById(modelId);
  if (!model) throw new Error(`Unknown local translation model ${modelId}.`);
  await disposeTranslator(modelId);
  const cache = await modelCache();
  const deletions = model.artifacts.map((artifact) =>
    cache.delete(translationArtifactUrl(model, artifact))
  );
  await Promise.all([...deletions, cache.delete(translationValidationUrl(model))]);
  setStatus(modelId, {
    phase: "notInstalled",
    downloadedBytes: 0,
    message: undefined
  });
}

function modelForRoute(
  sourceLanguage: TranslationLanguage,
  targetLanguage: TranslationLanguage
): TranslationModelManifest {
  const candidates = translationModelsForRoute(sourceLanguage, targetLanguage);
  const installed = candidates.find((model) => {
    const phase = requiredStatus(model.modelId).phase;
    return phase === "ready" || phase === "loading";
  });
  const model = installed ?? candidates[0];
  if (!model) {
    throw new Error(`No local translator supports ${sourceLanguage} to ${targetLanguage}.`);
  }
  return model;
}

async function loadTranslator(
  sourceLanguage: TranslationLanguage,
  targetLanguage: TranslationLanguage
): Promise<{ model: TranslationModelManifest; translator: Translator }> {
  const model = modelForRoute(sourceLanguage, targetLanguage);
  const existing = translators.get(model.modelId);
  if (existing) return { model, translator: existing };
  const status = requiredStatus(model.modelId);
  if (status.phase !== "ready") {
    throw new Error(`${model.displayName} is not installed and ready.`);
  }

  for (const loadedModelId of [...translators.keys()]) {
    if (loadedModelId !== model.modelId) await disposeTranslator(loadedModelId);
  }
  setStatus(model.modelId, {
    phase: "loading",
    message: `Loading ${model.displayName} locally…`
  });
  try {
    const translator = await pipeline("translation", model.modelId, {
      revision: model.revision,
      dtype: "q8",
      device: "wasm",
      local_files_only: true,
      // ORT 1.26's extended QDQ rewrite rejects these Marian q8 exports even
      // though the original graph is valid. Keep the model graph unchanged.
      session_options: { graphOptimizationLevel: "disabled" }
    }) as unknown as Translator;
    translators.set(model.modelId, translator);
    setStatus(model.modelId, { phase: "ready", message: undefined });
    return { model, translator };
  } catch (error) {
    setStatus(model.modelId, {
      phase: "failed",
      message: `The local translator could not load: ${errorMessage(error)}`
    });
    throw error;
  }
}

async function translate(
  sourceLanguage: TranslationLanguage,
  targetLanguage: TranslationLanguage,
  text: string
): Promise<string> {
  const trimmed = text.trim();
  if (!trimmed) return "";
  const { model, translator } = await loadTranslator(sourceLanguage, targetLanguage);
  const routeOptions = model.kind === "manyToMany"
    ? {
        src_lang: m2m100LanguageCode(sourceLanguage),
        tgt_lang: m2m100LanguageCode(targetLanguage)
      }
    : {};
  const output = await translator(trimmed, {
    // OCR frequently produces very short labels. A fixed 192-token ceiling
    // lets a bad end-of-sequence prediction spend seconds generating nonsense
    // while every other live label waits behind it. Keep enough expansion room
    // for real translation while bounding work to the size of this input.
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
  const approximateSourceTokens = compactScript
    ? characters
    : Math.ceil(characters / 3);
  return Math.max(24, Math.min(192, approximateSourceTokens * 3 + 12));
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

async function handleRequest(request: TranslationWorkerRequest): Promise<void> {
  try {
    let result: unknown;
    if (request.type === "status") result = await inspectCatalog();
    if (request.type === "install") result = await install(request.modelId);
    if (request.type === "remove") result = await remove(request.modelId);
    if (request.type === "prepare") {
      await loadTranslator(request.sourceLanguage, request.targetLanguage);
      result = undefined;
    }
    if (request.type === "translate") {
      result = await translate(request.sourceLanguage, request.targetLanguage, request.text);
    }
    post({ type: "reply", requestId: request.requestId, ok: true, result });
  } catch (error) {
    post({ type: "reply", requestId: request.requestId, ok: false, error: errorMessage(error) });
  }
}

workerScope.addEventListener("message", ({ data }: MessageEvent<TranslationWorkerRequest>) => {
  operationQueue = operationQueue.then(() => handleRequest(data));
});
