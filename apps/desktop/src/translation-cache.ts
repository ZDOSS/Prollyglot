import {
  TRANSLATION_CACHE_KEY,
  canonicalTranslationCacheKey
} from "./translation-catalog";

interface TranslationCache {
  match(request: string): Promise<Response | undefined>;
  put(request: string, response: Response): Promise<void>;
  delete(request: string): Promise<boolean>;
}

interface StoredResponse {
  key: string;
  body: Blob;
  headers: Array<[string, string]>;
  status: number;
  statusText: string;
}

const DATABASE_NAME = "prollyglot-translation-cache";
const STORE_NAME = "responses";

export async function openTranslationCache(): Promise<TranslationCache> {
  if ("caches" in globalThis) {
    try {
      const cache = await globalThis.caches.open(TRANSLATION_CACHE_KEY);
      return {
        match: async (request) => (await cache.match(canonicalTranslationCacheKey(request))) ?? undefined,
        put: (request, response) => cache.put(canonicalTranslationCacheKey(request), response),
        delete: (request) => cache.delete(canonicalTranslationCacheKey(request))
      };
    } catch (error) {
      console.warn("Cache Storage is unavailable; using IndexedDB for translation models.", error);
    }
  }

  if (!("indexedDB" in globalThis)) {
    throw new Error("This WebView does not provide local storage for translation models.");
  }
  return new IndexedDbTranslationCache(await openDatabase());
}

class IndexedDbTranslationCache implements TranslationCache {
  constructor(private readonly database: IDBDatabase) {}

  async match(request: string): Promise<Response | undefined> {
    request = canonicalTranslationCacheKey(request);
    const transaction = this.database.transaction(STORE_NAME, "readonly");
    const record = await requestResult<StoredResponse | undefined>(
      transaction.objectStore(STORE_NAME).get(request)
    );
    await transactionComplete(transaction);
    if (!record) return undefined;
    return new Response(record.body, {
      headers: record.headers,
      status: record.status,
      statusText: record.statusText
    });
  }

  async put(request: string, response: Response): Promise<void> {
    request = canonicalTranslationCacheKey(request);
    const record: StoredResponse = {
      key: request,
      body: await response.blob(),
      headers: [...response.headers.entries()],
      status: response.status,
      statusText: response.statusText
    };
    const transaction = this.database.transaction(STORE_NAME, "readwrite");
    transaction.objectStore(STORE_NAME).put(record);
    await transactionComplete(transaction);
  }

  async delete(request: string): Promise<boolean> {
    request = canonicalTranslationCacheKey(request);
    const existing = await this.match(request);
    if (!existing) return false;
    const transaction = this.database.transaction(STORE_NAME, "readwrite");
    transaction.objectStore(STORE_NAME).delete(request);
    await transactionComplete(transaction);
    return true;
  }
}

function openDatabase(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const request = globalThis.indexedDB.open(DATABASE_NAME, 1);
    request.addEventListener("upgradeneeded", () => {
      const database = request.result;
      if (!database.objectStoreNames.contains(STORE_NAME)) {
        database.createObjectStore(STORE_NAME, { keyPath: "key" });
      }
    });
    request.addEventListener("success", () => {
      request.result.addEventListener("versionchange", () => request.result.close());
      resolve(request.result);
    });
    request.addEventListener("blocked", () => {
      reject(new Error("Translation model storage is blocked by another Prollyglot window."));
    });
    request.addEventListener("error", () => {
      reject(request.error ?? new Error("Could not open translation model storage."));
    });
  });
}

function requestResult<Result>(request: IDBRequest<Result>): Promise<Result> {
  return new Promise((resolve, reject) => {
    request.addEventListener("success", () => resolve(request.result));
    request.addEventListener("error", () => {
      reject(request.error ?? new Error("Translation model storage request failed."));
    });
  });
}

function transactionComplete(transaction: IDBTransaction): Promise<void> {
  return new Promise((resolve, reject) => {
    transaction.addEventListener("complete", () => resolve());
    transaction.addEventListener("abort", () => {
      reject(transaction.error ?? new Error("Translation model storage transaction was aborted."));
    });
    transaction.addEventListener("error", () => {
      reject(transaction.error ?? new Error("Translation model storage transaction failed."));
    });
  });
}

export type { TranslationCache };
