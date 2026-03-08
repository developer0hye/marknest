import test from "node:test";
import assert from "node:assert/strict";

import {
  LARGE_ARCHIVE_BYTES,
  buildFallbackUrl,
  evaluateArchiveScale,
  normalizeFallbackBaseUrl,
  resolveExportBackend,
} from "./export_policy.mjs";

test("large archives recommend high quality export mode", () => {
  const summary = evaluateArchiveScale(LARGE_ARCHIVE_BYTES);

  assert.equal(summary.isLargeArchive, true);
  assert.equal(summary.recommendedQualityMode, "high-quality");
  assert.match(summary.message, /High Quality/i);
});

test("fallback base urls are trimmed and normalized", () => {
  assert.equal(
    normalizeFallbackBaseUrl(" http://127.0.0.1:3476/ "),
    "http://127.0.0.1:3476",
  );
  assert.equal(normalizeFallbackBaseUrl("   "), null);
});

test("fallback request urls preserve encoded entry paths", () => {
  assert.equal(
    buildFallbackUrl("http://127.0.0.1:3476/", "/api/render/pdf", {
      entry: "docs/README.md",
    }),
    "http://127.0.0.1:3476/api/render/pdf?entry=docs%2FREADME.md",
  );
});

test("browser failures switch to the server when a fallback url exists", () => {
  assert.equal(
    resolveExportBackend({
      qualityMode: "browser",
      fallbackBaseUrl: "http://127.0.0.1:3476",
      browserFailed: true,
    }),
    "server",
  );
});

test("high quality mode requires the server fallback path", () => {
  assert.equal(
    resolveExportBackend({
      qualityMode: "high-quality",
      fallbackBaseUrl: null,
      browserFailed: false,
    }),
    "server_unavailable",
  );
});
