const DEFAULT_NATIVE_CHUNK_BYTES = 4 * 1024 * 1024;

export async function nativeArtifactResponse(
  url: string,
  totalBytes: number,
  displayName: string,
  fetchRange: typeof fetch = globalThis.fetch,
  chunkBytes = DEFAULT_NATIVE_CHUNK_BYTES
): Promise<Response | undefined> {
  if (!Number.isSafeInteger(totalBytes) || totalBytes <= 0) {
    throw new Error("A native translation artifact must have a positive bounded size.");
  }
  if (!Number.isSafeInteger(chunkBytes) || chunkBytes <= 0) {
    throw new Error("A native translation read chunk must have a positive bounded size.");
  }
  const firstEnd = Math.min(totalBytes - 1, chunkBytes - 1);
  const first = await readRange(fetchRange, url, 0, firstEnd, totalBytes);
  if (!first) return undefined;
  let offset = first.byteLength;
  const stream = new ReadableStream<Uint8Array>({
    start(controller) {
      controller.enqueue(first);
      if (offset === totalBytes) controller.close();
    },
    async pull(controller) {
      if (offset >= totalBytes) {
        controller.close();
        return;
      }
      const end = Math.min(totalBytes - 1, offset + chunkBytes - 1);
      try {
        const next = await readRange(fetchRange, url, offset, end, totalBytes);
        if (!next) {
          controller.error(new Error(
            `${displayName} disappeared from native storage while it was loading.`
          ));
          return;
        }
        offset += next.byteLength;
        controller.enqueue(next);
        if (offset === totalBytes) controller.close();
      } catch (error) {
        controller.error(error);
      }
    }
  });
  return new Response(stream, {
    status: 200,
    headers: {
      "content-length": String(totalBytes),
      "content-type": url.endsWith(".json")
        ? "application/json"
        : "application/octet-stream",
      "x-prollyglot-storage": "native"
    }
  });
}

async function readRange(
  fetchRange: typeof fetch,
  url: string,
  start: number,
  end: number,
  totalBytes: number
): Promise<Uint8Array | undefined> {
  const response = await fetchRange(url, { headers: { Range: `bytes=${start}-${end}` } });
  if (nativeUnavailable(response.status)) return undefined;
  if (response.status !== 206) {
    throw new Error(`Native translation storage returned HTTP ${response.status} for a byte range.`);
  }
  const expectedLength = end - start + 1;
  const expectedRange = `bytes ${start}-${end}/${totalBytes}`;
  if (response.headers.get("content-range") !== expectedRange) {
    throw new Error("Native translation storage returned an unexpected byte range.");
  }
  const bytes = new Uint8Array(await response.arrayBuffer());
  if (bytes.byteLength !== expectedLength) {
    throw new Error(
      `Native translation storage returned ${bytes.byteLength} bytes; expected ${expectedLength}.`
    );
  }
  return bytes;
}

function nativeUnavailable(status: number): boolean {
  return status === 404 || status === 409;
}
