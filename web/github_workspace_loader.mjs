import {
  parseGitHubInputUrl,
  resolveGitHubUrlEntryPath,
} from "./github_url_input.mjs";

const MARKDOWN_EXTENSIONS = new Set([".md", ".markdown", ".mdown", ".mkdn"]);

function normalizeAssetReference(reference) {
  return String(reference ?? "")
    .trim()
    .split("#", 1)[0]
    .split("?", 1)[0]
    .trim();
}

function isMarkdownPath(filePath) {
  const lowerPath = String(filePath ?? "").toLowerCase();
  for (const extension of MARKDOWN_EXTENSIONS) {
    if (lowerPath.endsWith(extension)) {
      return true;
    }
  }
  return false;
}

function decodeBase64Bytes(base64Text) {
  const normalized = String(base64Text ?? "").replace(/\s+/g, "");
  const binary = atob(normalized);
  const bytes = new Uint8Array(binary.length);
  for (let index = 0; index < binary.length; index += 1) {
    bytes[index] = binary.charCodeAt(index);
  }
  return bytes;
}

function decodeGitHubFileContent(fileJson) {
  if (fileJson?.encoding === "base64" && typeof fileJson.content === "string") {
    return decodeBase64Bytes(fileJson.content);
  }
  return null;
}

function encodeGitHubPath(pathname) {
  return String(pathname ?? "")
    .split("/")
    .filter((segment) => segment.length > 0)
    .map((segment) => encodeURIComponent(segment))
    .join("/");
}

function sanitizeArchiveSegment(segment, fallbackValue) {
  const sanitized = String(segment ?? "")
    .trim()
    .replace(/[^a-z0-9._-]+/gi, "-")
    .replace(/^-+|-+$/g, "");
  return sanitized.length > 0 ? sanitized : fallbackValue;
}

function buildGitHubApiUrl(owner, repo, suffix, searchParams = null) {
  const url = new URL(`https://api.github.com/repos/${encodeURIComponent(owner)}/${encodeURIComponent(repo)}${suffix}`);
  if (searchParams) {
    for (const [key, value] of Object.entries(searchParams)) {
      if (value !== null && value !== undefined && String(value).length > 0) {
        url.searchParams.set(key, String(value));
      }
    }
  }
  return url.toString();
}

function buildRawGitHubUrl(owner, repo, gitRef, filePath) {
  return `https://raw.githubusercontent.com/${encodeURIComponent(owner)}/${encodeURIComponent(repo)}/${encodeURIComponent(gitRef)}/${encodeGitHubPath(filePath)}`;
}

function extractLocalImageReferences(markdownText) {
  const references = [];
  const seenReferences = new Set();
  for (const { pattern, groupIndex } of [
    { pattern: /!\[[^\]]*]\(([^)\s]+)(?:\s+"[^"]*")?\)/g, groupIndex: 1 },
    { pattern: /<img[^>]+src=(["']?)([^"'\s>]+)\1[^>]*>/gi, groupIndex: 2 },
  ]) {
    for (const match of String(markdownText ?? "").matchAll(pattern)) {
      const reference = normalizeAssetReference(match[groupIndex]);
      if (reference.length === 0 || seenReferences.has(reference)) {
        continue;
      }
      seenReferences.add(reference);
      references.push(reference);
    }
  }
  return references;
}

async function fetchGitHubJson(url, fetchImpl) {
  const response = await fetchImpl(url, {
    headers: {
      accept: "application/vnd.github+json",
    },
  });
  if (!response.ok) {
    const responseText = await response.text().catch(() => "");
    throw new Error(`GitHub request failed (${response.status}): ${responseText || url}`);
  }
  return response.json();
}

async function fetchGitHubFile(owner, repo, gitRef, filePath, fetchImpl) {
  const apiUrl = buildGitHubApiUrl(owner, repo, `/contents/${encodeGitHubPath(filePath)}`, {
    ref: gitRef,
  });
  const fileJson = await fetchGitHubJson(apiUrl, fetchImpl);
  const inlineBytes = decodeGitHubFileContent(fileJson);
  if (inlineBytes) {
    return {
      path: fileJson.path ?? filePath,
      bytes: inlineBytes,
    };
  }

  const downloadUrl = fileJson?.download_url || buildRawGitHubUrl(owner, repo, gitRef, filePath);
  const response = await fetchImpl(downloadUrl);
  if (!response.ok) {
    throw new Error(`GitHub raw fetch failed (${response.status}): ${downloadUrl}`);
  }
  return {
    path: fileJson.path ?? filePath,
    bytes: new Uint8Array(await response.arrayBuffer()),
  };
}

async function resolveDefaultBranch(owner, repo, fetchImpl) {
  const repositoryJson = await fetchGitHubJson(buildGitHubApiUrl(owner, repo, ""), fetchImpl);
  if (typeof repositoryJson.default_branch !== "string" || repositoryJson.default_branch.length === 0) {
    throw new Error("GitHub repository metadata did not include a default branch.");
  }
  return repositoryJson.default_branch;
}

async function resolveRepoReadme(owner, repo, gitRef, fetchImpl) {
  const readmeJson = await fetchGitHubJson(buildGitHubApiUrl(owner, repo, "/readme", { ref: gitRef }), fetchImpl);
  const inlineBytes = decodeGitHubFileContent(readmeJson);
  if (inlineBytes) {
    return {
      path: readmeJson.path,
      bytes: inlineBytes,
    };
  }

  const downloadUrl = readmeJson?.download_url || buildRawGitHubUrl(owner, repo, gitRef, readmeJson.path);
  const response = await fetchImpl(downloadUrl);
  if (!response.ok) {
    throw new Error(`GitHub README fetch failed (${response.status}): ${downloadUrl}`);
  }
  return {
    path: readmeJson.path,
    bytes: new Uint8Array(await response.arrayBuffer()),
  };
}

async function listGitHubDirectory(owner, repo, gitRef, directoryPath, fetchImpl) {
  const suffix = directoryPath.length > 0
    ? `/contents/${encodeGitHubPath(directoryPath)}`
    : "/contents";
  const directoryJson = await fetchGitHubJson(buildGitHubApiUrl(owner, repo, suffix, { ref: gitRef }), fetchImpl);
  if (!Array.isArray(directoryJson)) {
    throw new Error(`GitHub path is not a directory: ${directoryPath || "/"}`);
  }
  return directoryJson;
}

function readmePreferenceScore(filePath, directoryPath) {
  const baseName = String(filePath ?? "").split("/").pop() ?? "";
  const lowerBaseName = baseName.toLowerCase();
  const normalizedDirectory = String(directoryPath ?? "").replace(/^\/+|\/+$/g, "");

  if (lowerBaseName === "readme.md") {
    return 0;
  }
  if (lowerBaseName === "readme.markdown") {
    return 1;
  }
  if (lowerBaseName.startsWith("readme.")) {
    return 2;
  }
  if (normalizedDirectory.length === 0 && lowerBaseName === "index.md") {
    return 3;
  }
  if (lowerBaseName === "index.md") {
    return 4;
  }
  return 10;
}

export function pickGitHubDirectoryEntryPath(directoryPath, entries) {
  const normalizedDirectory = String(directoryPath ?? "").replace(/^\/+|\/+$/g, "");
  const prefix = normalizedDirectory.length > 0 ? `${normalizedDirectory}/` : "";
  const markdownEntries = (Array.isArray(entries) ? entries : [])
    .filter((entry) => (entry?.type === "file" || entry?.type === "blob") && isMarkdownPath(entry.path))
    .filter((entry) => prefix.length === 0 || String(entry.path).startsWith(prefix))
    .sort((left, right) => {
      const scoreDifference = readmePreferenceScore(left.path, normalizedDirectory)
        - readmePreferenceScore(right.path, normalizedDirectory);
      if (scoreDifference !== 0) {
        return scoreDifference;
      }
      return String(left.path).localeCompare(String(right.path));
    });
  return markdownEntries[0]?.path ?? null;
}

export function resolveLocalAssetWorkspacePath(readmePath, reference) {
  const cleanReference = normalizeAssetReference(reference);
  if (
    cleanReference.length === 0
    || cleanReference.startsWith("http://")
    || cleanReference.startsWith("https://")
    || cleanReference.startsWith("data:")
    || cleanReference.startsWith("mailto:")
  ) {
    return null;
  }

  const baseSegments = cleanReference.startsWith("/")
    ? []
    : String(readmePath ?? "")
      .replace(/^\/+|\/+$/g, "")
      .split("/")
      .slice(0, -1)
      .filter((segment) => segment.length > 0);
  const candidateSegments = cleanReference
    .replace(/^\/+/, "")
    .split("/")
    .filter((segment) => segment.length > 0 && segment !== ".");
  const resolvedSegments = [...baseSegments];

  for (const segment of candidateSegments) {
    const decodedSegment = decodeURIComponent(segment);
    if (decodedSegment === "..") {
      if (resolvedSegments.length === 0) {
        return null;
      }
      resolvedSegments.pop();
      continue;
    }
    resolvedSegments.push(decodedSegment);
  }

  if (resolvedSegments.length === 0) {
    return null;
  }

  return resolvedSegments.join("/");
}

async function resolveGitHubMarkdownFile(parsedInput, fetchImpl) {
  const gitRef = parsedInput.gitRef || await resolveDefaultBranch(parsedInput.owner, parsedInput.repo, fetchImpl);
  if (parsedInput.isFileReference && parsedInput.subpath) {
    const file = await fetchGitHubFile(parsedInput.owner, parsedInput.repo, gitRef, parsedInput.subpath, fetchImpl);
    return { gitRef, file };
  }

  if (!parsedInput.subpath) {
    const file = await resolveRepoReadme(parsedInput.owner, parsedInput.repo, gitRef, fetchImpl);
    return { gitRef, file };
  }

  const directoryEntries = await listGitHubDirectory(
    parsedInput.owner,
    parsedInput.repo,
    gitRef,
    parsedInput.subpath,
    fetchImpl,
  );
  const entryPath = pickGitHubDirectoryEntryPath(parsedInput.subpath, directoryEntries);
  if (!entryPath) {
    throw new Error(`No Markdown entry found under ${parsedInput.subpath}.`);
  }
  const file = await fetchGitHubFile(parsedInput.owner, parsedInput.repo, gitRef, entryPath, fetchImpl);
  return { gitRef, file };
}

export async function loadGitHubWorkspaceArchive({
  input,
  wasm,
  fetchImpl = fetch,
}) {
  if (!wasm || typeof wasm.buildPdfArchive !== "function") {
    throw new Error("WASM runtime is not ready to build a browser archive.");
  }

  const parsedInput = parseGitHubInputUrl(input);
  if (!parsedInput) {
    throw new Error("Enter a valid GitHub repository, tree, or blob URL.");
  }

  const { gitRef, file } = await resolveGitHubMarkdownFile(parsedInput, fetchImpl);
  const markdownBytes = file.bytes;
  const markdownText = new TextDecoder().decode(markdownBytes);
  const assetPaths = [];
  const seenAssetPaths = new Set();

  for (const reference of extractLocalImageReferences(markdownText)) {
    const resolvedPath = resolveLocalAssetWorkspacePath(file.path, reference);
    if (!resolvedPath || seenAssetPaths.has(resolvedPath)) {
      continue;
    }
    seenAssetPaths.add(resolvedPath);
    assetPaths.push(resolvedPath);
  }

  const archiveEntries = [file];
  for (const assetPath of assetPaths) {
    try {
      const assetFile = await fetchGitHubFile(parsedInput.owner, parsedInput.repo, gitRef, assetPath, fetchImpl);
      archiveEntries.push(assetFile);
    } catch (_error) {
      // Missing local assets should remain missing in the generated workspace so the WASM diagnostics stay visible.
    }
  }

  const archiveStem = `${sanitizeArchiveSegment(parsedInput.repo, "repo")}-${sanitizeArchiveSegment(gitRef, "ref")}`;
  const wrappedEntries = archiveEntries.map((entry) => ({
    path: `${archiveStem}/${entry.path}`,
    bytes: entry.bytes,
  }));
  const zipBytes = wasm.buildPdfArchive(wrappedEntries);
  const syntheticIndex = {
    selected_entry: file.path,
    entry_candidates: [{ path: file.path }],
  };

  return {
    archiveFileName: `${archiveStem}.zip`,
    selectedEntryPath: resolveGitHubUrlEntryPath(input, syntheticIndex) ?? file.path,
    sourceLabel: `github.com/${parsedInput.owner}/${parsedInput.repo}@${gitRef}`,
    zipBytes,
  };
}
