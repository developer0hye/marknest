import test from "node:test";
import assert from "node:assert/strict";

import {
  detectInputKind,
  hasLocalImageReferences,
  stripCommonPrefix,
  filesToArchiveEntries,
  buildZipFromFiles,
} from "./zip_from_files.mjs";

// --- helpers ---

function fakeFile(name, content = "", options = {}) {
  const blob = new Blob([content], { type: options.type ?? "" });
  const file = new File([blob], name, { type: options.type ?? "" });
  if (options.webkitRelativePath !== undefined) {
    Object.defineProperty(file, "webkitRelativePath", {
      value: options.webkitRelativePath,
      writable: false,
      enumerable: true,
    });
  }
  if (options.relativePath !== undefined) {
    Object.defineProperty(file, "relativePath", {
      value: options.relativePath,
      writable: false,
      enumerable: true,
    });
  }
  return file;
}

function fakeWasm() {
  return {
    buildPdfArchive(entries) {
      return new Uint8Array(
        new TextEncoder().encode(JSON.stringify(entries.map((e) => e.path))),
      );
    },
  };
}

// --- detectInputKind ---

test("detectInputKind returns unsupported for null or empty list", () => {
  assert.equal(detectInputKind(null), "unsupported");
  assert.equal(detectInputKind([]), "unsupported");
});

test("detectInputKind returns zip for a .zip file", () => {
  assert.equal(detectInputKind([fakeFile("docs.zip")]), "zip");
});

test("detectInputKind returns zip for application/zip mime type", () => {
  assert.equal(
    detectInputKind([fakeFile("archive", "", { type: "application/zip" })]),
    "zip",
  );
});

test("detectInputKind returns markdown for a .md file", () => {
  assert.equal(detectInputKind([fakeFile("README.md")]), "markdown");
});

test("detectInputKind returns markdown for a .markdown file", () => {
  assert.equal(detectInputKind([fakeFile("guide.markdown")]), "markdown");
});

test("detectInputKind returns unsupported for a single non-zip non-md file", () => {
  assert.equal(detectInputKind([fakeFile("photo.png")]), "unsupported");
});

test("detectInputKind returns folder when multiple files have webkitRelativePath", () => {
  const files = [
    fakeFile("README.md", "", { webkitRelativePath: "project/README.md" }),
    fakeFile("logo.png", "", { webkitRelativePath: "project/logo.png" }),
  ];
  assert.equal(detectInputKind(files), "folder");
});

test("detectInputKind returns folder when multiple files have relativePath from drag-and-drop", () => {
  const files = [
    fakeFile("README.md", "", { relativePath: "project/README.md" }),
    fakeFile("logo.png", "", { relativePath: "project/images/logo.png" }),
  ];
  assert.equal(detectInputKind(files), "folder");
});

test("detectInputKind returns folder for multi-select containing markdown files", () => {
  const files = [fakeFile("README.md"), fakeFile("notes.txt")];
  assert.equal(detectInputKind(files), "folder");
});

test("detectInputKind returns unsupported for multi-select without markdown files", () => {
  const files = [fakeFile("photo.png"), fakeFile("data.csv")];
  assert.equal(detectInputKind(files), "unsupported");
});

// --- hasLocalImageReferences ---

test("hasLocalImageReferences detects relative markdown image paths", () => {
  assert.equal(hasLocalImageReferences("![arch](./images/arch.svg)"), true);
  assert.equal(hasLocalImageReferences("![logo](images/logo.png)"), true);
  assert.equal(hasLocalImageReferences("![pic](../assets/pic.jpg)"), true);
});

test("hasLocalImageReferences detects local html img tags", () => {
  assert.equal(
    hasLocalImageReferences('<img src="diagram.svg" alt="Diagram">'),
    true,
  );
  assert.equal(
    hasLocalImageReferences("<img src='./local.png' />"),
    true,
  );
});

test("hasLocalImageReferences ignores http and https urls", () => {
  assert.equal(
    hasLocalImageReferences("![logo](https://example.com/logo.png)"),
    false,
  );
  assert.equal(
    hasLocalImageReferences("![logo](http://cdn.example.com/img.jpg)"),
    false,
  );
  assert.equal(
    hasLocalImageReferences('<img src="https://example.com/a.svg">'),
    false,
  );
});

test("hasLocalImageReferences ignores data URIs", () => {
  assert.equal(
    hasLocalImageReferences("![inline](data:image/png;base64,abc123)"),
    false,
  );
});

test("hasLocalImageReferences returns false for text-only markdown", () => {
  assert.equal(hasLocalImageReferences("# Title\n\nSome paragraph."), false);
});

test("hasLocalImageReferences detects local among mixed references", () => {
  const md =
    "![remote](https://example.com/a.png)\n![local](./b.png)\n";
  assert.equal(hasLocalImageReferences(md), true);
});

// --- stripCommonPrefix ---

test("stripCommonPrefix removes a shared top-level directory", () => {
  const entries = [
    { path: "project/README.md", bytes: new Uint8Array([1]) },
    { path: "project/images/logo.png", bytes: new Uint8Array([2]) },
  ];

  const stripped = stripCommonPrefix(entries);

  assert.equal(stripped[0].path, "README.md");
  assert.equal(stripped[1].path, "images/logo.png");
});

test("stripCommonPrefix preserves paths when there is no shared prefix", () => {
  const entries = [
    { path: "docs/README.md", bytes: new Uint8Array([1]) },
    { path: "src/main.rs", bytes: new Uint8Array([2]) },
  ];

  const stripped = stripCommonPrefix(entries);

  assert.equal(stripped[0].path, "docs/README.md");
  assert.equal(stripped[1].path, "src/main.rs");
});

test("stripCommonPrefix preserves paths when files are at root level", () => {
  const entries = [
    { path: "README.md", bytes: new Uint8Array([1]) },
    { path: "LICENSE", bytes: new Uint8Array([2]) },
  ];

  const stripped = stripCommonPrefix(entries);

  assert.equal(stripped[0].path, "README.md");
  assert.equal(stripped[1].path, "LICENSE");
});

test("stripCommonPrefix returns empty array unchanged", () => {
  assert.deepEqual(stripCommonPrefix([]), []);
});

// --- filesToArchiveEntries ---

test("filesToArchiveEntries reads files using webkitRelativePath and strips common prefix", async () => {
  const files = [
    fakeFile("README.md", "# Hello", {
      webkitRelativePath: "my-project/README.md",
    }),
    fakeFile("logo.png", "\x89PNG", {
      webkitRelativePath: "my-project/images/logo.png",
    }),
  ];

  const entries = await filesToArchiveEntries(files);

  assert.equal(entries.length, 2);
  assert.equal(entries[0].path, "README.md");
  assert.equal(entries[1].path, "images/logo.png");
  assert.deepEqual(entries[0].bytes, new Uint8Array(new TextEncoder().encode("# Hello")));
});

test("filesToArchiveEntries uses relativePath from drag-and-drop", async () => {
  const files = [
    fakeFile("guide.md", "# Guide", { relativePath: "docs/guide.md" }),
  ];

  const entries = await filesToArchiveEntries(files);

  assert.equal(entries.length, 1);
  assert.equal(entries[0].path, "guide.md");
});

test("filesToArchiveEntries falls back to file name when no relative path exists", async () => {
  const files = [fakeFile("README.md", "# Readme")];

  const entries = await filesToArchiveEntries(files);

  assert.equal(entries.length, 1);
  assert.equal(entries[0].path, "README.md");
});

// --- buildZipFromFiles ---

test("buildZipFromFiles wraps a single markdown file into a zip with no warnings when image-free", async () => {
  const wasm = fakeWasm();
  const files = [fakeFile("guide.md", "# Guide\n\nNo images here.")];

  const result = await buildZipFromFiles(wasm, files);

  assert.equal(result.fileName, "guide.zip");
  assert.deepEqual(result.warnings, []);
  assert.ok(result.zipBytes instanceof Uint8Array);
  assert.ok(result.zipBytes.length > 0);
});

test("buildZipFromFiles warns when a single markdown file has local image references", async () => {
  const wasm = fakeWasm();
  const files = [fakeFile("doc.md", "# Doc\n\n![arch](./images/arch.svg)")];

  const result = await buildZipFromFiles(wasm, files);

  assert.equal(result.fileName, "doc.zip");
  assert.equal(result.warnings.length, 1);
  assert.match(result.warnings[0], /local images/i);
  assert.match(result.warnings[0], /upload the parent folder/i);
});

test("buildZipFromFiles does not warn for markdown with only remote images", async () => {
  const wasm = fakeWasm();
  const files = [
    fakeFile("doc.md", "![logo](https://example.com/logo.png)"),
  ];

  const result = await buildZipFromFiles(wasm, files);

  assert.deepEqual(result.warnings, []);
});

test("buildZipFromFiles processes folder files and derives folder name", async () => {
  const wasm = fakeWasm();
  const files = [
    fakeFile("README.md", "# Hi", {
      webkitRelativePath: "my-docs/README.md",
    }),
    fakeFile("logo.png", "\x89PNG", {
      webkitRelativePath: "my-docs/images/logo.png",
    }),
  ];

  const result = await buildZipFromFiles(wasm, files);

  assert.equal(result.fileName, "my-docs.zip");
  assert.deepEqual(result.warnings, []);
  assert.ok(result.zipBytes instanceof Uint8Array);
});

test("buildZipFromFiles uses fallback folder name when relativePath has no directory", async () => {
  const wasm = fakeWasm();
  const files = [fakeFile("README.md", "# Hi"), fakeFile("notes.md", "# N")];

  const result = await buildZipFromFiles(wasm, files);

  assert.equal(result.fileName, "workspace.zip");
});

test("buildZipFromFiles converts .markdown extension to .zip in filename", async () => {
  const wasm = fakeWasm();
  const files = [fakeFile("notes.markdown", "# Notes")];

  const result = await buildZipFromFiles(wasm, files);

  assert.equal(result.fileName, "notes.zip");
});
