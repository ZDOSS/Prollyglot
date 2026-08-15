/// <reference lib="webworker" />

import { sha256 as sha256Hash } from "@noble/hashes/sha2.js";
import { bytesToHex } from "@noble/hashes/utils.js";

import {
  TRANSLATION_MODELS,
  translationArtifactUrl,
  translationModelById,
  translationValidationUrl,
  type TranslationArtifact,
  type TranslationModelManifest
} from "./translation-catalog";
import type {
  TranslationControlRequest,
  TranslationControlResponse
} from "./translation-protocol";
import { openTranslationCache, type TranslationCache } from "./translation-cache";
import type { TranslationCatalogStatus, TranslationModelStatus } from "./types";

class IntegrityError extends Error {}

const workerScope = self as unknown as DedicatedWorkerGlobalScope;
const statuses = new Map<string, TranslationModelStatus>();
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

function post(message: TranslationControlResponse): void {
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
  cachePromise ??= openTranslationCache();
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

async function remove(modelId: string): Promise<void> {
  const model = translationModelById(modelId);
  if (!model) throw new Error(`Unknown local translation model ${modelId}.`);
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

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

async function handleRequest(request: TranslationControlRequest): Promise<void> {
  try {
    let result: unknown;
    if (request.type === "status") result = await inspectCatalog();
    if (request.type === "install") result = await install(request.modelId);
    if (request.type === "remove") result = await remove(request.modelId);
    post({ type: "reply", requestId: request.requestId, ok: true, result });
  } catch (error) {
    post({ type: "reply", requestId: request.requestId, ok: false, error: errorMessage(error) });
  }
}

workerScope.addEventListener("message", ({ data }: MessageEvent<TranslationControlRequest>) => {
  operationQueue = operationQueue.then(() => handleRequest(data));
});
