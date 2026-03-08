const EXPECTED_HEADERS = [
  "id",
  "tier",
  "category",
  "repo",
  "url",
  "default_branch",
  "pinned_sha",
  "readme_path",
  "stars_at_curation",
  "expected_patterns",
  "selection_reason",
];

export function sanitizeCorpusId(repoFullName) {
  return String(repoFullName ?? "")
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9/._-]+/g, "-")
    .replace(/\//g, "--");
}

export function parseCorpusManifest(manifestText) {
  const normalizedText = String(manifestText ?? "").replace(/\r\n/g, "\n").trim();
  if (!normalizedText) {
    throw new Error("The corpus manifest is empty.");
  }

  const lines = normalizedText
    .split("\n")
    .map((line) => line.trimEnd())
    .filter((line) => line.length > 0);
  const header = lines[0].split("\t");
  if (header.length !== EXPECTED_HEADERS.length) {
    throw new Error("The corpus manifest header is invalid.");
  }
  for (let index = 0; index < EXPECTED_HEADERS.length; index += 1) {
    if (header[index] !== EXPECTED_HEADERS[index]) {
      throw new Error("The corpus manifest header is invalid.");
    }
  }

  const ids = new Set();
  const entries = [];
  for (const line of lines.slice(1)) {
    const parts = line.split("\t");
    if (parts.length !== EXPECTED_HEADERS.length) {
      throw new Error(`Invalid manifest row: ${line}`);
    }

    const [
      id,
      tier,
      category,
      repo,
      url,
      defaultBranch,
      pinnedSha,
      readmePath,
      starsAtCuration,
      expectedPatterns,
      selectionReason,
    ] = parts.map((value) => value.trim());

    if (!id || !repo || !url || !defaultBranch || !pinnedSha || !readmePath) {
      throw new Error(`Manifest row is missing required values: ${line}`);
    }
    if (!["smoke", "extended"].includes(tier)) {
      throw new Error(`Unsupported corpus tier: ${tier}`);
    }
    if (!/^[0-9a-f]{40}$/i.test(pinnedSha)) {
      throw new Error(`Invalid pinned SHA for ${id}.`);
    }
    if (!/^[^/]+\/[^/]+$/.test(repo)) {
      throw new Error(`Invalid repo name for ${id}.`);
    }
    if (!/^https:\/\/github\.com\/[^/]+\/[^/]+$/i.test(url)) {
      throw new Error(`Invalid GitHub repo URL for ${id}.`);
    }
    if (readmePath.startsWith("/") || readmePath.includes("..")) {
      throw new Error(`Invalid readme_path for ${id}.`);
    }

    const parsedStars = Number.parseInt(starsAtCuration, 10);
    if (!Number.isFinite(parsedStars) || parsedStars < 0) {
      throw new Error(`Invalid stars_at_curation for ${id}.`);
    }
    if (ids.has(id)) {
      throw new Error(`Duplicate manifest id: ${id}`);
    }
    ids.add(id);

    entries.push({
      id,
      tier,
      category,
      repo,
      url,
      defaultBranch,
      pinnedSha: pinnedSha.toLowerCase(),
      readmePath,
      starsAtCuration: parsedStars,
      expectedPatterns: expectedPatterns
        .split(",")
        .map((value) => value.trim())
        .filter((value) => value.length > 0),
      selectionReason,
    });
  }

  return entries;
}
