import {
  SPOKEN_LANGUAGES,
  type TranslationLanguage
} from "./language-catalog";

export interface TranslationArtifact {
  path: string;
  size: number;
  sha256: string;
}

export type TranslationModelKind = "direct" | "toEnglish" | "manyToMany";

export interface TranslationModelManifest {
  kind: TranslationModelKind;
  sourceLanguages: readonly TranslationLanguage[];
  targetLanguages: readonly TranslationLanguage[];
  modelId: string;
  displayName: string;
  revision: string;
  license: "Apache-2.0" | "MIT";
  artifacts: readonly TranslationArtifact[];
  totalBytes: number;
}

export const TRANSLATION_CACHE_KEY = "prollyglot-translation-models-v1";

const ALL_TRANSLATION_LANGUAGES = SPOKEN_LANGUAGES.map(
  ({ code }) => code
) as TranslationLanguage[];
const NON_ENGLISH_LANGUAGES = ALL_TRANSLATION_LANGUAGES.filter(
  (language) => language !== "en"
);

export const TRANSLATION_MODELS: readonly TranslationModelManifest[] = [
  {
    kind: "direct",
    sourceLanguages: ["ja"],
    targetLanguages: ["en"],
    modelId: "Xenova/opus-mt-ja-en",
    displayName: "Japanese to English · Compact",
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
    kind: "direct",
    sourceLanguages: ["es"],
    targetLanguages: ["en"],
    modelId: "Xenova/opus-mt-es-en",
    displayName: "Spanish to English · Compact",
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
  },
  {
    kind: "toEnglish",
    sourceLanguages: NON_ENGLISH_LANGUAGES,
    targetLanguages: ["en"],
    modelId: "Xenova/opus-mt-mul-en",
    displayName: "Multilingual to English · Compact",
    revision: "72a05e47cee89c718a9db4dc70d02fef3bc39de8",
    license: "Apache-2.0",
    totalBytes: 118_351_723,
    artifacts: [
      {
        path: "config.json",
        size: 1_390,
        sha256: "115093532b9893a6e3ec64951db15bafbad200f34929304f9570c3cd7f1dff94"
      },
      {
        path: "onnx/encoder_model_quantized.onnx",
        size: 52_475_294,
        sha256: "5ce609a524375dbdd9c66b62b82b41abc667799022667ce424f20c245bd56925"
      },
      {
        path: "onnx/decoder_model_merged_quantized.onnx",
        size: 59_785_040,
        sha256: "6add167a0cd3f78aa298b8f927d2e8645e33cb17df11e92967f0c5b5703c8c4d"
      },
      {
        path: "generation_config.json",
        size: 293,
        sha256: "66300b2138c7a98fe085d16b590ecbc01d64d46e9db240ee3dab4eadd3a3b1b9"
      },
      {
        path: "tokenizer.json",
        size: 6_089_424,
        sha256: "7ae61d18c438de0cf069a5cd25edc0d9d899353d710bf0e774819f97201049b9"
      },
      {
        path: "tokenizer_config.json",
        size: 282,
        sha256: "0e5fceb4caf753096870f0a74ec2a0a9825327cefd1cc06e0b8dc71e75257cf7"
      }
    ]
  },
  {
    kind: "manyToMany",
    sourceLanguages: ALL_TRANSLATION_LANGUAGES,
    targetLanguages: ALL_TRANSLATION_LANGUAGES,
    modelId: "Xenova/m2m100_418M",
    displayName: "Universal 29-language translator",
    revision: "9c374f0b7aca709787cea97b047bfbbd1559d177",
    license: "MIT",
    totalBytes: 639_976_029,
    artifacts: [
      {
        path: "config.json",
        size: 908,
        sha256: "1dbdf77ddc7809acd4c54ccf0eab46f840b40174afb1b6f6de8787244e832938"
      },
      {
        path: "onnx/encoder_model_quantized.onnx",
        size: 287_856_370,
        sha256: "13a94e354a9140764eb81102d77d3ec6952d796e6f113c651eeb3c3443da0386"
      },
      {
        path: "onnx/decoder_model_merged_quantized.onnx",
        size: 344_128_178,
        sha256: "007654bcabb6cea6fd3bde34ce933137b431330b3755781145d7b6906270b45a"
      },
      {
        path: "generation_config.json",
        size: 233,
        sha256: "722210dd0bee7bef4e8e7f9a8574d8c56a2dfff723d73f390ce67892740b9009"
      },
      {
        path: "tokenizer.json",
        size: 7_988_527,
        sha256: "03d9e111731c2d71f39a2c2a88499743e4c251385d07f0384b4349a23ba54363"
      },
      {
        path: "tokenizer_config.json",
        size: 1_813,
        sha256: "bacfd4b9da25a61e01f17abe660465f616c9a1a3f5e23ab9ad3326c3788f2d9f"
      }
    ]
  }
] as const;

export function translationModelById(modelId: string): TranslationModelManifest | undefined {
  return TRANSLATION_MODELS.find((model) => model.modelId === modelId);
}

export function translationModelsForRoute(
  sourceLanguage: TranslationLanguage,
  targetLanguage: TranslationLanguage
): TranslationModelManifest[] {
  if (sourceLanguage === targetLanguage) return [];
  return TRANSLATION_MODELS.filter((model) =>
    model.sourceLanguages.includes(sourceLanguage)
    && model.targetLanguages.includes(targetLanguage)
  );
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
 * Transformers.js probes the default branch before it applies the pinned
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
