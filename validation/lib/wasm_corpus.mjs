import fs from "node:fs/promises";
import path from "node:path";

import { buildSourceSnapshot } from "./text_metrics.mjs";

function stripReferenceDecorations(reference) {
  return String(reference ?? "")
    .trim()
    .split("#", 1)[0]
    .split("?", 1)[0]
    .trim();
}

export function resolveLocalAssetWorkspacePath(readmePath, reference) {
  const cleanReference = stripReferenceDecorations(reference);
  if (
    cleanReference.length === 0
    || cleanReference.startsWith("http://")
    || cleanReference.startsWith("https://")
    || cleanReference.startsWith("data:")
    || cleanReference.startsWith("mailto:")
  ) {
    return null;
  }

  const normalizedPath = cleanReference.startsWith("/")
    ? path.posix.normalize(cleanReference.slice(1))
    : path.posix.normalize(
      path.posix.join(path.posix.dirname(readmePath), cleanReference),
    );

  if (
    normalizedPath.length === 0
    || normalizedPath === "."
    || normalizedPath === ".."
    || normalizedPath.startsWith("../")
    || path.posix.isAbsolute(normalizedPath)
  ) {
    return null;
  }

  return normalizedPath;
}

export function buildGitHubUrlCases(entry) {
  const cases = [
    {
      label: "repo",
      url: entry.url,
    },
  ];

  if (entry.defaultBranch && entry.readmePath) {
    cases.push({
      label: "blob",
      url: `${entry.url}/blob/${entry.defaultBranch}/${entry.readmePath}`,
    });
  }

  return cases;
}

export async function buildMinimalWorkspaceFiles({
  sourceRoot,
  readmePath,
  wrapperDirectoryName,
}) {
  const readmeAbsolutePath = path.join(sourceRoot, ...readmePath.split("/"));
  const markdownText = await fs.readFile(readmeAbsolutePath, "utf8");
  const snapshot = buildSourceSnapshot(markdownText);
  const assetPaths = [];
  const seenAssetPaths = new Set();

  for (const reference of snapshot.localImageReferences) {
    const resolvedPath = resolveLocalAssetWorkspacePath(readmePath, reference);
    if (!resolvedPath || seenAssetPaths.has(resolvedPath)) {
      continue;
    }
    seenAssetPaths.add(resolvedPath);
    assetPaths.push(resolvedPath);
  }

  const files = [];
  for (const relativePath of [readmePath, ...assetPaths]) {
    const absolutePath = path.join(sourceRoot, ...relativePath.split("/"));
    const stat = await fs.stat(absolutePath).catch(() => null);
    if (!stat?.isFile()) {
      continue;
    }

    files.push({
      path: `${wrapperDirectoryName}/${relativePath}`,
      bytes: new Uint8Array(await fs.readFile(absolutePath)),
    });
  }

  return files;
}
