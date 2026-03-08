import test from "node:test";
import assert from "node:assert/strict";

import {
  buildFallbackFormData,
  buildOutputOptions,
  debugBundleFileName,
} from "./output_options.mjs";

test("buildOutputOptions normalizes empty fields and preserves explicit output controls", () => {
  const options = buildOutputOptions({
    theme: "docs",
    customCss: "body { color: rgb(5, 4, 3); }",
    headerTemplate: "  ",
    footerTemplate: "<div>{{pageNumber}}</div>",
    title: "Guide Pack",
    author: "Docs Team",
    subject: "",
    pageSize: "letter",
    marginTopMm: "18",
    marginRightMm: "12",
    marginBottomMm: "24",
    marginLeftMm: "10",
    landscape: true,
    mermaidMode: "auto",
    mathMode: "on",
  });

  assert.deepEqual(options, {
    theme: "docs",
    custom_css: "body { color: rgb(5, 4, 3); }",
    header_template: null,
    footer_template: "<div>{{pageNumber}}</div>",
    title: "Guide Pack",
    author: "Docs Team",
    subject: null,
    page_size: "letter",
    margin_top_mm: 18,
    margin_right_mm: 12,
    margin_bottom_mm: 24,
    margin_left_mm: 10,
    landscape: true,
    mermaid_mode: "auto",
    math_mode: "on",
  });
});

test("buildOutputOptions fans out a uniform margin when side-specific values are omitted", () => {
  const options = buildOutputOptions({
    marginMm: "16",
  });

  assert.equal(options.margin_top_mm, 16);
  assert.equal(options.margin_right_mm, 16);
  assert.equal(options.margin_bottom_mm, 16);
  assert.equal(options.margin_left_mm, 16);
});

test("buildFallbackFormData stores the archive as a blob field and JSON options as text", async () => {
  const payload = buildFallbackFormData({
    zipBytes: new Uint8Array([1, 2, 3]),
    fileName: "docs.zip",
    entryPath: "docs/README.md",
    options: buildOutputOptions({
      theme: "github",
      pageSize: "a4",
      marginTopMm: "18",
      marginRightMm: "12",
      marginBottomMm: "20",
      marginLeftMm: "10",
      landscape: false,
      mermaidMode: "off",
      mathMode: "off",
    }),
  });

  assert.equal(payload.get("entry"), "docs/README.md");

  const archive = payload.get("archive");
  assert.equal(archive.name, "docs.zip");
  assert.equal(await archive.text(), "\u0001\u0002\u0003");

  const options = JSON.parse(payload.get("options"));
  assert.equal(options.theme, "github");
  assert.equal(options.page_size, "a4");
  assert.equal(options.margin_top_mm, 18);
  assert.equal(options.margin_right_mm, 12);
  assert.equal(options.margin_bottom_mm, 20);
  assert.equal(options.margin_left_mm, 10);
});

test("debugBundleFileName derives a stable archive name from the uploaded zip and entry path", () => {
  assert.equal(
    debugBundleFileName("workspace.zip", "docs/README.md"),
    "workspace-docs-README-debug.zip",
  );
  assert.equal(debugBundleFileName(null, "guide.md"), "marknest-guide-debug.zip");
});
