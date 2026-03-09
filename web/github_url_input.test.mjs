import test from "node:test";
import assert from "node:assert/strict";

import {
  buildGitHubArchiveRequest,
  parseGitHubInputUrl,
  resolveGitHubUrlEntryPath,
} from "./github_url_input.mjs";

test("parseGitHubInputUrl parses a bare repository URL", () => {
  assert.deepEqual(parseGitHubInputUrl("https://github.com/user/repo"), {
    owner: "user",
    repo: "repo",
    gitRef: null,
    subpath: null,
    isFileReference: false,
  });
});

test("parseGitHubInputUrl parses a blob URL with a file path", () => {
  assert.deepEqual(
    parseGitHubInputUrl("https://github.com/user/repo/blob/main/docs/guide.md"),
    {
      owner: "user",
      repo: "repo",
      gitRef: "main",
      subpath: "docs/guide.md",
      isFileReference: true,
    },
  );
});

test("buildGitHubArchiveRequest preserves blob entry selection for browser flows", () => {
  assert.deepEqual(
    buildGitHubArchiveRequest("https://github.com/user/repo/blob/main/docs/guide.md"),
    {
      owner: "user",
      repo: "repo",
      gitRef: "main",
      explicitEntry: "docs/guide.md",
      stripZipPrefix: true,
    },
  );
});

test("resolveGitHubUrlEntryPath prefers an explicit blob path over the analyzed default", () => {
  const projectIndex = {
    selected_entry: "README.md",
    entry_candidates: [{ path: "README.md" }, { path: "docs/guide.md" }],
  };

  assert.equal(
    resolveGitHubUrlEntryPath(
      "https://github.com/user/repo/blob/main/docs/guide.md",
      projectIndex,
    ),
    "docs/guide.md",
  );
});

test("resolveGitHubUrlEntryPath falls back to the analyzed default for repo URLs", () => {
  const projectIndex = {
    selected_entry: "README.md",
    entry_candidates: [{ path: "README.md" }],
  };

  assert.equal(resolveGitHubUrlEntryPath("https://github.com/user/repo", projectIndex), "README.md");
});

test("parseGitHubInputUrl rejects non-GitHub URLs", () => {
  assert.equal(parseGitHubInputUrl("https://gitlab.com/user/repo"), null);
  assert.equal(parseGitHubInputUrl("README.md"), null);
});
