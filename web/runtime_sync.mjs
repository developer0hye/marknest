function sleep(durationMs) {
  return new Promise((resolve) => setTimeout(resolve, durationMs));
}

function normalizeMessageList(value) {
  if (!Array.isArray(value)) {
    return [];
  }

  return value
    .map((item) => String(item ?? "").trim())
    .filter((item) => item.length > 0);
}

export function normalizeRenderStatus(rawStatus) {
  if (!rawStatus || typeof rawStatus !== "object") {
    throw new Error("Render frame exposed an invalid runtime status.");
  }

  return {
    ready: Boolean(rawStatus.ready),
    warnings: normalizeMessageList(rawStatus.warnings),
    errors: normalizeMessageList(rawStatus.errors),
  };
}

function readRenderStatus(iframe) {
  try {
    const frameWindow = iframe?.contentWindow;
    if (!frameWindow) {
      throw new Error("Render frame is not available.");
    }

    const rawStatus = frameWindow.__MARKNEST_RENDER_STATUS__;
    if (rawStatus === undefined || rawStatus === null) {
      return { ready: true, warnings: [], errors: [] };
    }

    return normalizeRenderStatus(rawStatus);
  } catch (error) {
    throw new Error(`Render frame is not accessible: ${String(error.message || error)}`);
  }
}

export async function waitForFrameRenderStatus(
  iframe,
  { timeoutMs = 15000, pollMs = 50 } = {},
) {
  const startedAt = Date.now();

  while (Date.now() - startedAt <= timeoutMs) {
    const status = readRenderStatus(iframe);
    if (status.ready) {
      return status;
    }

    await sleep(pollMs);
  }

  throw new Error("Timed out while waiting for Mermaid and Math rendering to finish.");
}

export function runtimeDiagnosticsForEntry(entryPath, status) {
  const entryLabel = entryPath || "selected entry";
  return {
    warnings: status.warnings.map((message) => `Runtime warning (${entryLabel}): ${message}`),
    errors: status.errors.map((message) => `Runtime error (${entryLabel}): ${message}`),
  };
}

export function mergeProjectDiagnostics(projectIndex, runtimeDiagnostics = null) {
  const diagnostic = projectIndex?.diagnostic ?? {
    missing_assets: [],
    warnings: [],
    path_errors: [],
  };

  return {
    warnings: [
      ...diagnostic.missing_assets.map((value) => `Missing asset: ${value}`),
      ...diagnostic.warnings,
      ...(runtimeDiagnostics?.warnings ?? []),
    ],
    errors: [...diagnostic.path_errors, ...(runtimeDiagnostics?.errors ?? [])],
  };
}

export function hasBlockingRuntimeErrors(status) {
  return status.errors.length > 0;
}
