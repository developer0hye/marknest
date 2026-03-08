const DEFAULT_TIMEOUT_MS = 15000;
const DEFAULT_MAX_BYTES_PER_ASSET = 16 * 1024 * 1024;
const DEFAULT_MAX_TOTAL_BYTES = 64 * 1024 * 1024;

function normalizeRemoteAssetList(assets) {
  if (!Array.isArray(assets)) {
    return [];
  }

  return assets
    .filter((asset) => asset && typeof asset === "object")
    .filter((asset) => typeof asset.fetch_url === "string" && asset.fetch_url.trim().length > 0)
    .map((asset) => ({
      entryPath: String(asset.entry_path ?? asset.entryPath ?? "").trim(),
      originalReference: String(asset.original_reference ?? asset.originalReference ?? "").trim(),
      fetchUrl: String(asset.fetch_url).trim(),
    }))
    .filter((asset) => asset.originalReference.length > 0);
}

function normalizeContentType(value) {
  return String(value ?? "")
    .split(";")[0]
    .trim()
    .toLowerCase();
}

function inferMimeTypeFromUrl(url) {
  const normalized = String(url ?? "")
    .split("#")[0]
    .split("?")[0]
    .toLowerCase();

  if (normalized.endsWith(".png")) {
    return "image/png";
  }
  if (normalized.endsWith(".jpg") || normalized.endsWith(".jpeg")) {
    return "image/jpeg";
  }
  if (normalized.endsWith(".gif")) {
    return "image/gif";
  }
  if (normalized.endsWith(".svg")) {
    return "image/svg+xml";
  }
  if (normalized.endsWith(".webp")) {
    return "image/webp";
  }
  if (normalized.endsWith(".bmp")) {
    return "image/bmp";
  }
  if (normalized.endsWith(".avif")) {
    return "image/avif";
  }

  return null;
}

function looksLikeSvg(bytes) {
  const decoded = new TextDecoder().decode(bytes);
  return decoded.includes("<svg");
}

function encodeBase64(bytes) {
  if (typeof Buffer !== "undefined") {
    return Buffer.from(bytes).toString("base64");
  }

  let binary = "";
  const chunkSize = 0x8000;
  for (let index = 0; index < bytes.length; index += chunkSize) {
    const chunk = bytes.subarray(index, index + chunkSize);
    binary += String.fromCharCode(...chunk);
  }
  return btoa(binary);
}

function findSrcAttributeSpan(tag) {
  const lowerTag = tag.toLowerCase();
  let offset = 0;

  while (offset < lowerTag.length) {
    const relativeIndex = lowerTag.indexOf("src", offset);
    if (relativeIndex === -1) {
      return null;
    }

    const previousCharacter = tag[relativeIndex - 1];
    const previousIsBoundary =
      relativeIndex > 0 && (/\s/.test(previousCharacter) || previousCharacter === "<");
    if (!previousIsBoundary) {
      offset = relativeIndex + 3;
      continue;
    }

    let cursor = relativeIndex + 3;
    while (cursor < tag.length && /\s/.test(tag[cursor])) {
      cursor += 1;
    }
    if (tag[cursor] !== "=") {
      offset = relativeIndex + 3;
      continue;
    }

    cursor += 1;
    while (cursor < tag.length && /\s/.test(tag[cursor])) {
      cursor += 1;
    }

    const firstCharacter = tag[cursor];
    if (firstCharacter === '"' || firstCharacter === "'") {
      const quote = firstCharacter;
      const valueStart = cursor + 1;
      const valueEnd = tag.indexOf(quote, valueStart);
      if (valueEnd === -1) {
        return null;
      }
      return { valueStart, valueEnd, isQuoted: true };
    }

    let valueEnd = cursor;
    while (valueEnd < tag.length && !/\s|>/.test(tag[valueEnd])) {
      valueEnd += 1;
    }
    return { valueStart: cursor, valueEnd, isQuoted: false };
  }

  return null;
}

function rewriteImgTag(tag, replacements) {
  const srcSpan = findSrcAttributeSpan(tag);
  if (!srcSpan) {
    return tag;
  }

  const originalReference = tag.slice(srcSpan.valueStart, srcSpan.valueEnd);
  const replacement = replacements.get(originalReference);
  if (!replacement) {
    return tag;
  }

  if (srcSpan.isQuoted) {
    return `${tag.slice(0, srcSpan.valueStart)}${replacement}${tag.slice(srcSpan.valueEnd)}`;
  }

  return `${tag.slice(0, srcSpan.valueStart)}"${replacement}"${tag.slice(srcSpan.valueEnd)}`;
}

function rewriteHtmlImgSources(html, replacements) {
  if (replacements.size === 0) {
    return html;
  }

  const lowerHtml = html.toLowerCase();
  let rewrittenHtml = "";
  let cursor = 0;

  while (cursor < html.length) {
    const relativeTagIndex = lowerHtml.indexOf("<img", cursor);
    if (relativeTagIndex === -1) {
      break;
    }

    const tagEnd = lowerHtml.indexOf(">", relativeTagIndex);
    if (tagEnd === -1) {
      break;
    }

    rewrittenHtml += html.slice(cursor, relativeTagIndex);
    rewrittenHtml += rewriteImgTag(html.slice(relativeTagIndex, tagEnd + 1), replacements);
    cursor = tagEnd + 1;
  }

  rewrittenHtml += html.slice(cursor);
  return rewrittenHtml;
}

async function fetchRemoteAssetDataUri(
  asset,
  {
    fetchImpl,
    timeoutMs,
    maxBytesPerAsset,
    maxTotalBytes,
    totalDownloadedBytesRef,
  },
) {
  const controller = new AbortController();
  const timeoutId = setTimeout(() => controller.abort(), timeoutMs);

  try {
    const response = await fetchImpl(asset.fetchUrl, {
      mode: "cors",
      redirect: "follow",
      signal: controller.signal,
    });
    if (!response.ok) {
      throw new Error(`HTTP ${response.status} from ${asset.fetchUrl}`);
    }

    const contentLengthHeader = response.headers.get("content-length");
    if (contentLengthHeader) {
      const contentLength = Number.parseInt(contentLengthHeader, 10);
      if (Number.isFinite(contentLength) && contentLength > maxBytesPerAsset) {
        throw new Error(
          `response size ${contentLength} bytes exceeds the per-asset limit of ${maxBytesPerAsset} bytes`,
        );
      }
      if (
        Number.isFinite(contentLength) &&
        contentLength > maxTotalBytes - totalDownloadedBytesRef.value
      ) {
        throw new Error(
          `response size ${contentLength} bytes exceeds the remaining per-entry remote asset budget`,
        );
      }
    }

    const bytes = new Uint8Array(await response.arrayBuffer());
    if (bytes.length > maxBytesPerAsset) {
      throw new Error(
        `response size exceeds the per-asset limit of ${maxBytesPerAsset} bytes`,
      );
    }
    if (bytes.length > maxTotalBytes - totalDownloadedBytesRef.value) {
      throw new Error(
        `response size ${bytes.length} bytes exceeds the remaining per-entry remote asset budget`,
      );
    }
    totalDownloadedBytesRef.value += bytes.length;

    const contentType = normalizeContentType(response.headers.get("content-type"));
    const finalUrl = response.url || asset.fetchUrl;
    const inferredMimeType =
      inferMimeTypeFromUrl(finalUrl) ||
      inferMimeTypeFromUrl(asset.fetchUrl) ||
      (looksLikeSvg(bytes) ? "image/svg+xml" : null);
    const mimeType =
      contentType.startsWith("image/")
        ? contentType
        : inferredMimeType;

    if (!mimeType) {
      throw new Error(
        contentType
          ? `response content type ${contentType} is not a supported image`
          : "response did not declare a supported image type",
      );
    }

    return `data:${mimeType};base64,${encodeBase64(bytes)}`;
  } catch (error) {
    if (error?.name === "AbortError") {
      throw new Error(`Timed out while fetching ${asset.fetchUrl}.`);
    }
    throw error instanceof Error ? error : new Error(String(error));
  } finally {
    clearTimeout(timeoutId);
  }
}

export async function materializeRemoteImages({
  html,
  assets,
  fetchImpl = fetch,
  timeoutMs = DEFAULT_TIMEOUT_MS,
  maxBytesPerAsset = DEFAULT_MAX_BYTES_PER_ASSET,
  maxTotalBytes = DEFAULT_MAX_TOTAL_BYTES,
} = {}) {
  const normalizedAssets = normalizeRemoteAssetList(assets);
  const replacements = new Map();
  const remoteAssets = [];
  const warnings = [];
  const cachedResults = new Map();
  const totalDownloadedBytesRef = { value: 0 };

  for (const asset of normalizedAssets) {
    let resultPromise = cachedResults.get(asset.fetchUrl);
    if (!resultPromise) {
      resultPromise = fetchRemoteAssetDataUri(asset, {
        fetchImpl,
        timeoutMs,
        maxBytesPerAsset,
        maxTotalBytes,
        totalDownloadedBytesRef,
      });
      cachedResults.set(asset.fetchUrl, resultPromise);
    }

    try {
      const dataUri = await resultPromise;
      replacements.set(asset.originalReference, dataUri);
      remoteAssets.push({
        original_reference: asset.originalReference,
        fetch_url: asset.fetchUrl,
        status: "inlined",
        message: null,
      });
    } catch (error) {
      const message = String(error?.message || error);
      warnings.push(
        `Remote asset could not be materialized: ${asset.entryPath || "selected entry"} -> ${asset.originalReference} (${message})`,
      );
      remoteAssets.push({
        original_reference: asset.originalReference,
        fetch_url: asset.fetchUrl,
        status: "failed",
        message,
      });
    }
  }

  return {
    html: rewriteHtmlImgSources(String(html ?? ""), replacements),
    remoteAssets,
    warnings,
  };
}

export function hasFailedRemoteAssets(remoteAssets) {
  return Array.isArray(remoteAssets)
    ? remoteAssets.some((asset) => asset?.status === "failed")
    : false;
}
