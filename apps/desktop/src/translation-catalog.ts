export interface TranslationArtifact {
  path: string;
  size: number;
  sha256: string;
}

export interface TranslationModelManifest {
  sourceLanguage: "es" | "ja";
  targetLanguage: "en";
  modelId: string;
  displayName: string;
  revision: string;
  license: "Apache-2.0";
  artifacts: TranslationArtifact[];
  totalBytes: number;
}

export const TRANSLATION_CACHE_KEY = "prollyglot-translation-models-v1";

export const TRANSLATION_MODELS: readonly TranslationModelManifest[] = [
  {
    sourceLanguage: "ja",
    targetLanguage: "en",
    modelId: "Xenova/opus-mt-ja-en",
    displayName: "Japanese to English",
    revision: "1a906cfaaf7c8f4193f67f5885c082aa6dbd9d16",
    license: "Apache-2.0",
    totalBytes: 114_701_000,
    artifacts: [
      {
        path: "config.json",
        size: 1_376,
        sha256: "d35476f1d3858883feec67e5a63ddc387452a70fda9c8e26db99c71aa2ad6a34"
      },
      {
        path: "onnx/encoder_model_quantized.onnx",
        size: 50_705_822,
        sha256: "345262b16bcdda1468b0f3380c112b7ce79f731176b4b1d21f6edd5b2ae0d25c"
      },
      {
        path: "onnx/decoder_model_merged_quantized.onnx",
        size: 58_001_744,
        sha256: "b304d0014e4e1575437b6af95467b6cb54405d923732d8359113bd6dbbee93c0"
      },
      {
        path: "generation_config.json",
        size: 293,
        sha256: "ed1bc2193210ca2a6dc9c144ea789936fed8a27113e01010d8e591f45f238912"
      },
      {
        path: "tokenizer.json",
        size: 5_991_485,
        sha256: "770ff2855437cf44f1f110550c5a9dca773253a167aeac36076b2073d259aa3b"
      },
      {
        path: "tokenizer_config.json",
        size: 280,
        sha256: "414e9e7e2492b95fa80f24ccb295645d35c4dd0a74d7ef8b89820c0c7e8ec356"
      }
    ]
  },
  {
    sourceLanguage: "es",
    targetLanguage: "en",
    modelId: "Xenova/opus-mt-es-en",
    displayName: "Spanish to English",
    revision: "eadfd7c658a9d8929ac3b8e996b68a68e2c7d480",
    license: "Apache-2.0",
    totalBytes: 119_377_236,
    artifacts: [
      {
        path: "config.json",
        size: 1_433,
        sha256: "fab3a7f93185bc5aa7b419f6a1e6e74d98c8f2a506c94493d3019bf46da3478d"
      },
      {
        path: "onnx/encoder_model_quantized.onnx",
        size: 52_899_742,
        sha256: "c01e70f8455efc350831aa2af7ed187b16829241f0018ba07d1ba643a391bc18"
      },
      {
        path: "onnx/decoder_model_merged_quantized.onnx",
        size: 60_212_804,
        sha256: "4cd91ab30240d295ce907b5100826031838c672005e57e524d711122d75605fa"
      },
      {
        path: "generation_config.json",
        size: 293,
        sha256: "b743baabb7da4c1a2f19fe558bd6b4c0c7c3b0762fcb5ca7a48fe5a2c2219803"
      },
      {
        path: "tokenizer.json",
        size: 6_262_682,
        sha256: "285eb29e7155ee48851a77960797813f86a125f70d2c1a124f613f1fbd2b19c3"
      },
      {
        path: "tokenizer_config.json",
        size: 282,
        sha256: "e1fac15a910169d5b5ec07a13b0374273626a239b5142db10be229ca66cc52a9"
      }
    ]
  }
] as const;

export function translationModel(sourceLanguage: string): TranslationModelManifest | undefined {
  return TRANSLATION_MODELS.find((model) => model.sourceLanguage === sourceLanguage);
}

export function translationArtifactUrl(
  model: TranslationModelManifest,
  artifact: TranslationArtifact
): string {
  return `https://huggingface.co/${model.modelId}/resolve/${encodeURIComponent(model.revision)}/${artifact.path}`;
}

export function translationValidationUrl(model: TranslationModelManifest): string {
  return `https://prollyglot.invalid/translation/${encodeURIComponent(model.modelId)}/${model.revision}/verified`;
}

/**
 * Transformers.js 4.2 probes the default branch before it applies the pinned
 * pipeline revision. Resolve those probes to the verified pinned artifact so
 * model discovery cannot read the network or drift to a newer revision.
 */
export function canonicalTranslationCacheKey(request: string): string {
  for (const model of TRANSLATION_MODELS) {
    for (const artifact of model.artifacts) {
      const modelPath = `${model.modelId}/${artifact.path}`;
      if (
        request.endsWith(`/resolve/main/${artifact.path}`)
        && request.includes(`/${model.modelId}/`)
      ) {
        return translationArtifactUrl(model, artifact);
      }
      if (request.endsWith(modelPath) && request.startsWith("/__prollyglot_translation_cache_only__/")) {
        return translationArtifactUrl(model, artifact);
      }
    }
  }
  return request;
}
