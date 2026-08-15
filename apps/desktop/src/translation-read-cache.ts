import {
  TRANSLATION_MODELS,
  canonicalTranslationCacheKey,
  translationArtifactUrl,
  translationValidationUrl,
  type TranslationArtifact,
  type TranslationModelManifest
} from "./translation-catalog";
import { openTranslationCache, type TranslationCache } from "./translation-cache";
import { nativeArtifactResponse } from "./translation-native-stream";

/**
 * Reads verified native artifacts through bounded range responses and retains
 * the old WebView cache as a read-only migration fallback. Writes remain on
 * the legacy cache only for the explicit browser real-model development mode.
 */
export async function openTranslationReadCache(
  nativeBaseUrl?: string
): Promise<TranslationCache> {
  if (!nativeBaseUrl) return openTranslationCache();
  let legacy: TranslationCache | undefined;
  try {
    legacy = await openTranslationCache();
  } catch (error) {
    // Native inference must not depend on an older WebView storage facility.
    // A failed legacy store only removes the migration fallback.
    console.warn("Legacy translation storage is unavailable; using native models only.", error);
  }
  return new NativeFirstTranslationCache(nativeBaseUrl, legacy);
}

export class NativeFirstTranslationCache implements TranslationCache {
  private readonly baseUrl: string;
  private readonly legacy?: TranslationCache;

  constructor(
    baseUrl: string,
    legacy?: TranslationCache
  ) {
    this.baseUrl = baseUrl;
    this.legacy = legacy;
  }

  async match(request: string): Promise<Response | undefined> {
    const canonical = canonicalTranslationCacheKey(request);
    const native = await this.nativeMatch(canonical);
    return native ?? this.legacy?.match(canonical);
  }

  async put(_request: string, _response: Response): Promise<void> {
    // Desktop inference is read-only. Explicit browser development installs
    // receive the legacy cache directly instead of this native-first wrapper.
  }

  async delete(_request: string): Promise<boolean> {
    return false;
  }

  private async nativeMatch(request: string): Promise<Response | undefined> {
    for (const model of TRANSLATION_MODELS) {
      if (request === translationValidationUrl(model)) {
        const response = await fetch(this.resourceUrl(model, "verified"));
        if (nativeUnavailable(response.status)) return undefined;
        if (!response.ok) throw await nativeReadError(model, "verification", response);
        return new Response(await response.arrayBuffer(), {
          status: 200,
          headers: { "content-type": "application/json", "x-prollyglot-storage": "native" }
        });
      }
      const artifact = model.artifacts.find(
        (candidate) => request === translationArtifactUrl(model, candidate)
      );
      if (artifact) return this.nativeArtifact(model, artifact);
    }
    return undefined;
  }

  private async nativeArtifact(
    model: TranslationModelManifest,
    artifact: TranslationArtifact
  ): Promise<Response | undefined> {
    const url = this.resourceUrl(model, artifact.path);
    return nativeArtifactResponse(url, artifact.size, model.displayName);
  }

  private resourceUrl(model: TranslationModelManifest, resource: string): string {
    const encoded = resource.split("/").map(encodeURIComponent).join("/");
    return `${this.baseUrl}/${encodeURIComponent(model.storageId)}/${encoded}`;
  }
}

function nativeUnavailable(status: number): boolean {
  return status === 404 || status === 409;
}

async function nativeReadError(
  model: TranslationModelManifest,
  resource: string,
  response: Response
): Promise<Error> {
  const detail = (await response.text()).trim();
  return new Error(
    `${model.displayName} could not read its native ${resource} resource (HTTP ${response.status})`
    + (detail ? `: ${detail}` : ".")
  );
}
