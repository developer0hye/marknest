import test from "node:test";
import assert from "node:assert/strict";

import {
  hasBlockingRuntimeErrors,
  mergeProjectDiagnostics,
  normalizeRenderStatus,
  runtimeDiagnosticsForEntry,
  waitForFrameRenderStatus,
} from "./runtime_sync.mjs";

test("waitForFrameRenderStatus treats a missing runtime status as ready", async () => {
  const status = await waitForFrameRenderStatus(
    {
      contentWindow: {},
    },
    { timeoutMs: 20, pollMs: 1 },
  );

  assert.deepEqual(status, { ready: true, warnings: [], errors: [] });
});

test("waitForFrameRenderStatus waits until the iframe runtime status becomes ready", async () => {
  const iframe = {
    contentWindow: {
      __MARKNEST_RENDER_STATUS__: {
        ready: false,
        warnings: ["Math fallback"],
        errors: [],
      },
    },
  };

  setTimeout(() => {
    iframe.contentWindow.__MARKNEST_RENDER_STATUS__ = {
      ready: true,
      warnings: ["Math fallback"],
      errors: [],
    };
  }, 10);

  const status = await waitForFrameRenderStatus(iframe, { timeoutMs: 60, pollMs: 2 });

  assert.equal(status.ready, true);
  assert.deepEqual(status.warnings, ["Math fallback"]);
  assert.deepEqual(status.errors, []);
});

test("waitForFrameRenderStatus times out when runtime rendering never becomes ready", async () => {
  await assert.rejects(
    () =>
      waitForFrameRenderStatus(
        {
          contentWindow: {
            __MARKNEST_RENDER_STATUS__: { ready: false, warnings: [], errors: [] },
          },
        },
        { timeoutMs: 15, pollMs: 1 },
      ),
    /Timed out while waiting for Mermaid and Math rendering to finish\./,
  );
});

test("normalizeRenderStatus rejects malformed runtime payloads", () => {
  assert.throws(() => normalizeRenderStatus("bad-payload"), /invalid runtime status/i);
});

test("runtimeDiagnosticsForEntry prefixes runtime warnings and errors with the entry path", () => {
  const diagnostics = runtimeDiagnosticsForEntry("docs/README.md", {
    ready: true,
    warnings: ["Math rendering failed: expression 1."],
    errors: ["Mermaid rendering failed: diagram 2."],
  });

  assert.deepEqual(diagnostics.warnings, [
    "Runtime warning (docs/README.md): Math rendering failed: expression 1.",
  ]);
  assert.deepEqual(diagnostics.errors, [
    "Runtime error (docs/README.md): Mermaid rendering failed: diagram 2.",
  ]);
  assert.equal(hasBlockingRuntimeErrors({ ready: true, warnings: [], errors: diagnostics.errors }), true);
});

test("mergeProjectDiagnostics keeps analysis and runtime diagnostics together", () => {
  const merged = mergeProjectDiagnostics(
    {
      diagnostic: {
        missing_assets: ["docs/diagram.png"],
        warnings: ["Mermaid fallback preserved the code block."],
        path_errors: ["Path traversal attempt was blocked."],
      },
    },
    {
      warnings: ["Runtime warning (docs/README.md): Math fallback rendered inline."],
      errors: ["Runtime error (docs/README.md): Mermaid rendering failed."],
    },
  );

  assert.deepEqual(merged.warnings, [
    "Missing asset: docs/diagram.png",
    "Mermaid fallback preserved the code block.",
    "Runtime warning (docs/README.md): Math fallback rendered inline.",
  ]);
  assert.deepEqual(merged.errors, [
    "Path traversal attempt was blocked.",
    "Runtime error (docs/README.md): Mermaid rendering failed.",
  ]);
});
