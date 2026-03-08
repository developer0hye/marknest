export const LARGE_ARCHIVE_BYTES = 16 * 1024 * 1024;

export function evaluateArchiveScale(sizeBytes) {
  if (Number(sizeBytes) >= LARGE_ARCHIVE_BYTES) {
    return {
      isLargeArchive: true,
      recommendedQualityMode: "high-quality",
      message:
        "Large ZIP detected. Preview can still work, but High Quality server export is more reliable for bigger archives.",
    };
  }

  return {
    isLargeArchive: false,
    recommendedQualityMode: "browser",
    message: "Archive size is within the browser-friendly range.",
  };
}

export function normalizeFallbackBaseUrl(value) {
  const trimmed = String(value ?? "").trim();
  if (!trimmed) {
    return null;
  }

  return trimmed.replace(/\/+$/, "");
}

export function buildFallbackUrl(baseUrl, pathName, params = {}) {
  const normalizedBaseUrl = normalizeFallbackBaseUrl(baseUrl);
  if (!normalizedBaseUrl) {
    throw new Error("A fallback server URL is required.");
  }

  const url = new URL(pathName, `${normalizedBaseUrl}/`);
  for (const [key, value] of Object.entries(params)) {
    if (value === undefined || value === null || value === "") {
      continue;
    }
    url.searchParams.set(key, String(value));
  }

  return url.toString();
}

export function resolveExportBackend(options) {
  const qualityMode = options?.qualityMode ?? "browser";
  const fallbackBaseUrl = normalizeFallbackBaseUrl(options?.fallbackBaseUrl);
  const browserFailed = Boolean(options?.browserFailed);

  if (qualityMode === "high-quality") {
    return fallbackBaseUrl ? "server" : "server_unavailable";
  }

  if (browserFailed && fallbackBaseUrl) {
    return "server";
  }

  return "browser";
}
