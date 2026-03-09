import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";

import {
  SAMPLE_ARCHIVE_ROOT,
  SAMPLE_ENTRY_PATH,
  buildWasmExampleAnalyzeOptions,
  buildWasmExampleArchiveEntries,
  buildWasmExampleRenderOptions,
  extractRuntimeScriptUrls,
  pickExampleEntryPath,
} from "./wasm-example-support.mjs";

test("the built-in WASM example archive is wrapped in a shared top-level directory", () => {
  const entries = buildWasmExampleArchiveEntries();

  assert.equal(entries.length, 3);
  assert.equal(entries[0].path, SAMPLE_ENTRY_PATH);
  assert.ok(entries.every((entry) => entry.path.startsWith(`${SAMPLE_ARCHIVE_ROOT}/`)));
  assert.match(entries[0].text, /strip_zip_prefix/);
});

test("WASM example render options trim custom runtime asset base URLs", () => {
  const options = buildWasmExampleRenderOptions({
    stripZipPrefix: true,
    runtimeAssetsBaseUrl: " /runtime-assets/custom ",
    mermaidMode: "on",
    mathMode: "auto",
    theme: "docs",
  });

  assert.deepEqual(options, {
    theme: "docs",
    mermaid_mode: "on",
    math_mode: "auto",
    runtime_assets_base_url: "/runtime-assets/custom",
    strip_zip_prefix: true,
  });
});

test("WASM example analyze options default to no prefix stripping", () => {
  assert.deepEqual(buildWasmExampleAnalyzeOptions(), { strip_zip_prefix: false });
  assert.deepEqual(buildWasmExampleAnalyzeOptions({ stripZipPrefix: true }), {
    strip_zip_prefix: true,
  });
});

test("WASM example picks the selected entry when analysis provides one", () => {
  assert.equal(
    pickExampleEntryPath({
      selected_entry: "README.md",
      entry_candidates: [{ path: "docs/guide.md" }],
    }),
    "README.md",
  );
  assert.equal(
    pickExampleEntryPath({
      selected_entry: null,
      entry_candidates: [{ path: "docs/guide.md" }],
    }),
    "docs/guide.md",
  );
  assert.equal(pickExampleEntryPath(null), null);
});

test("WASM example extracts runtime script URLs from rendered preview HTML", () => {
  const html =
    '<script>const config={"mermaidScript":"/runtime-assets/mermaid/mermaid.min.js","mathScript":"/runtime-assets/mathjax/es5/tex-svg.js"};</script>';

  assert.deepEqual(extractRuntimeScriptUrls(html), {
    mermaidScriptUrl: "/runtime-assets/mermaid/mermaid.min.js",
    mathScriptUrl: "/runtime-assets/mathjax/es5/tex-svg.js",
  });
});

test("index.html copies the standalone WASM example assets for Trunk output", () => {
  const indexPath = path.join(process.cwd(), "index.html");
  const html = fs.readFileSync(indexPath, "utf8");

  assert.match(html, /rel="copy-file"\s+href="web\/wasm-example\.html"/i);
  assert.match(html, /rel="copy-file"\s+href="web\/wasm-example\.js"/i);
  assert.match(html, /rel="copy-file"\s+href="web\/wasm-example-support\.mjs"/i);
  assert.match(html, /rel="copy-file"\s+href="web\/github_url_input\.mjs"/i);
  assert.match(html, /rel="copy-file"\s+href="web\/github_workspace_loader\.mjs"/i);
  assert.match(html, /href="\.\/wasm-example\.html"/i);
});

test("the standalone WASM example page exposes the controls needed for browser verification", () => {
  const examplePath = path.join(process.cwd(), "web", "wasm-example.html");
  const html = fs.readFileSync(examplePath, "utf8");

  assert.match(html, /id="strip-zip-prefix"/i);
  assert.match(html, /id="github-url-input"/i);
  assert.match(html, /id="render-from-github-url"/i);
  assert.match(html, /Load from GitHub URL/i);
  assert.match(html, /Public GitHub URLs fetch the target README/i);
  assert.match(html, /id="runtime-assets-base-url"/i);
  assert.match(html, /id="entry-select"/i);
  assert.match(html, /src="\.\/wasm-example\.js"/i);
});

test("the standalone WASM example script materializes remote images before writing the preview iframe", () => {
  const scriptPath = path.join(process.cwd(), "web", "wasm-example.js");
  const script = fs.readFileSync(scriptPath, "utf8");

  assert.match(script, /from "\.\/remote_assets\.mjs"/i);
  assert.match(script, /materializeRemoteImages\(/);
});
