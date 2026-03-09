/**
 * Converts folder contents or a single Markdown file into ZIP bytes
 * that the existing WASM analyzeZip / renderHtml pipeline can consume.
 */

/**
 * Classify the user-provided file list.
 *
 * @param {File[]} files
 * @returns {"zip" | "folder" | "markdown" | "unsupported"}
 */
export function detectInputKind(files) {
  if (!files || files.length === 0) {
    return "unsupported";
  }

  if (files.length === 1) {
    const file = files[0];
    const name = file.name.toLowerCase();

    if (name.endsWith(".zip") || file.type === "application/zip") {
      return "zip";
    }

    if (name.endsWith(".md") || name.endsWith(".markdown")) {
      return "markdown";
    }

    return "unsupported";
  }

  // Multiple files: either a folder upload (webkitRelativePath set) or
  // a drag-and-drop directory read (relativePath set).
  const hasRelativePaths = files.some(
    (file) => file.webkitRelativePath || file.relativePath,
  );
  if (hasRelativePaths) {
    return "folder";
  }

  // Multiple files without relative paths — could be a multi-select.
  // Check if any are markdown files; treat as folder-like input.
  const hasMarkdown = files.some((file) => {
    const name = file.name.toLowerCase();
    return name.endsWith(".md") || name.endsWith(".markdown");
  });

  return hasMarkdown ? "folder" : "unsupported";
}

/**
 * Check whether a Markdown source references local images that cannot be
 * resolved when the browser only has the single file.
 *
 * @param {string} markdownText
 * @returns {boolean}
 */
export function hasLocalImageReferences(markdownText) {
  // Markdown image syntax: ![alt](path)
  const markdownImagePattern = /!\[[^\]]*\]\(([^)]+)\)/g;
  // HTML image syntax: <img ... src="path" ...>
  const htmlImagePattern = /<img[^>]+src\s*=\s*["']([^"']+)["'][^>]*>/gi;

  for (const pattern of [markdownImagePattern, htmlImagePattern]) {
    let match;
    while ((match = pattern.exec(markdownText)) !== null) {
      const reference = match[1].trim();
      if (!reference) {
        continue;
      }

      // External URLs are fine — they will be fetched by the remote asset pipeline.
      if (/^https?:\/\//i.test(reference)) {
        continue;
      }

      // Data URIs are already inline.
      if (/^data:/i.test(reference)) {
        continue;
      }

      // Anything else is a local reference that cannot be resolved.
      return true;
    }
  }

  return false;
}

/**
 * Remove a single shared directory prefix from all entry paths.
 *
 * When uploading a folder named `my-project/`, every file path starts with
 * `my-project/`. The ZIP analysis pipeline works better without that
 * top-level wrapper.
 *
 * @param {{path: string, bytes: Uint8Array}[]} entries
 * @returns {{path: string, bytes: Uint8Array}[]}
 */
export function stripCommonPrefix(entries) {
  if (entries.length === 0) {
    return entries;
  }

  const firstSlash = entries[0].path.indexOf("/");
  if (firstSlash < 0) {
    return entries;
  }

  const prefix = entries[0].path.slice(0, firstSlash + 1);
  const allMatch = entries.every((entry) => entry.path.startsWith(prefix));
  if (!allMatch) {
    return entries;
  }

  return entries.map((entry) => ({
    path: entry.path.slice(prefix.length),
    bytes: entry.bytes,
  }));
}

/**
 * Read a list of `File` objects into `{path, bytes}[]` entries suitable
 * for `buildPdfArchive`.
 *
 * @param {File[]} files
 * @returns {Promise<{path: string, bytes: Uint8Array}[]>}
 */
export async function filesToArchiveEntries(files) {
  const entries = [];

  for (const file of files) {
    const path = file.relativePath || file.webkitRelativePath || file.name;
    if (!path) {
      continue;
    }

    const arrayBuffer = await file.arrayBuffer();
    entries.push({
      path,
      bytes: new Uint8Array(arrayBuffer),
    });
  }

  return stripCommonPrefix(entries);
}

/**
 * Convert a list of `File` objects (from folder selection, drag-and-drop, or
 * a single `.md` upload) into ZIP bytes that the WASM pipeline can analyze.
 *
 * @param {{ buildPdfArchive: (files: {path: string, bytes: Uint8Array}[]) => Uint8Array }} wasm
 * @param {File[]} files
 * @returns {Promise<{zipBytes: Uint8Array, fileName: string, warnings: string[]}>}
 */
export async function buildZipFromFiles(wasm, files) {
  const inputKind = detectInputKind(files);
  const warnings = [];

  if (inputKind === "markdown") {
    const file = files[0];
    const text = await file.text();

    if (hasLocalImageReferences(text)) {
      warnings.push(
        "This Markdown file references local images that cannot be resolved in the browser. " +
          "Only external (HTTP) images will be displayed. To include local images, upload the parent folder instead.",
      );
    }

    const bytes = new Uint8Array(await file.arrayBuffer());
    const archiveEntries = [{ path: file.name, bytes }];
    const zipBytes = wasm.buildPdfArchive(archiveEntries);
    const fileName = file.name.replace(/\.(md|markdown)$/i, ".zip");
    return { zipBytes, fileName, warnings };
  }

  // Folder input: read all files and build a ZIP.
  const archiveEntries = await filesToArchiveEntries(files);
  if (archiveEntries.length === 0) {
    throw new Error("The selected folder does not contain any files.");
  }

  const zipBytes = wasm.buildPdfArchive(archiveEntries);

  // Derive a folder name from the first file's original relative path.
  const firstFile = files[0];
  const relativePath = firstFile.relativePath || firstFile.webkitRelativePath || "";
  const folderName = relativePath.split("/")[0] || "workspace";
  const fileName = `${folderName}.zip`;

  return { zipBytes, fileName, warnings };
}

/**
 * Recursively read all files from a dropped directory entry.
 *
 * @param {FileSystemDirectoryEntry} directoryEntry
 * @returns {Promise<File[]>}
 */
export async function readDirectoryEntries(directoryEntry) {
  const files = [];
  const reader = directoryEntry.createReader();

  async function readBatch() {
    return new Promise((resolve, reject) => {
      reader.readEntries(resolve, reject);
    });
  }

  let batch;
  do {
    batch = await readBatch();
    for (const entry of batch) {
      if (entry.isFile) {
        const file = await new Promise((resolve, reject) =>
          entry.file(resolve, reject),
        );
        // Attach the full path — entry.fullPath starts with "/".
        Object.defineProperty(file, "relativePath", {
          value: entry.fullPath.replace(/^\//, ""),
          writable: false,
          enumerable: true,
        });
        files.push(file);
      } else if (entry.isDirectory) {
        files.push(...(await readDirectoryEntries(entry)));
      }
    }
  } while (batch.length > 0);

  return files;
}
