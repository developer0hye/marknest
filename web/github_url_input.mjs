function normalizeGitHubSubpath(subpath) {
  if (typeof subpath !== "string") {
    return null;
  }

  const normalizedSegments = [];
  for (const rawSegment of subpath.split("/")) {
    const segment = rawSegment.trim();
    if (segment.length === 0 || segment === ".") {
      continue;
    }
    if (segment === "..") {
      return null;
    }
    normalizedSegments.push(segment);
  }

  return normalizedSegments.length > 0 ? normalizedSegments.join("/") : null;
}

export function parseGitHubInputUrl(input) {
  const trimmed = String(input ?? "").trim();
  const afterScheme = trimmed.startsWith("https://")
    ? trimmed.slice("https://".length)
    : trimmed.startsWith("http://")
      ? trimmed.slice("http://".length)
      : null;
  if (!afterScheme) {
    return null;
  }

  const afterHost = afterScheme.startsWith("github.com/")
    ? afterScheme.slice("github.com/".length)
    : afterScheme.startsWith("www.github.com/")
      ? afterScheme.slice("www.github.com/".length)
      : null;
  if (!afterHost) {
    return null;
  }

  const segments = afterHost
    .replace(/\/+$/g, "")
    .split("/")
    .map((segment) => segment.trim())
    .filter((segment) => segment.length > 0);
  if (segments.length < 2) {
    return null;
  }

  const owner = segments[0];
  const repo = segments[1].replace(/\.git$/i, "");
  if (!owner || !repo) {
    return null;
  }

  if (segments.length === 2) {
    return {
      owner,
      repo,
      gitRef: null,
      subpath: null,
      isFileReference: false,
    };
  }

  const pathType = segments[2];
  const isFileReference = pathType === "blob";
  if (!isFileReference && pathType !== "tree") {
    return null;
  }
  if (segments.length < 4) {
    return null;
  }

  const gitRef = segments[3];
  const subpath = segments.length > 4 ? normalizeGitHubSubpath(segments.slice(4).join("/")) : null;

  return {
    owner,
    repo,
    gitRef,
    subpath,
    isFileReference,
  };
}

export function buildGitHubArchiveRequest(input) {
  const parsed = parseGitHubInputUrl(input);
  if (!parsed) {
    return null;
  }

  return {
    owner: parsed.owner,
    repo: parsed.repo,
    gitRef: parsed.gitRef,
    explicitEntry: parsed.isFileReference ? parsed.subpath : null,
    stripZipPrefix: true,
  };
}

export function resolveGitHubUrlEntryPath(input, projectIndex) {
  const request = buildGitHubArchiveRequest(input);
  if (!request) {
    return null;
  }

  if (request.explicitEntry) {
    return request.explicitEntry;
  }

  return projectIndex?.selected_entry ?? projectIndex?.entry_candidates?.[0]?.path ?? null;
}
