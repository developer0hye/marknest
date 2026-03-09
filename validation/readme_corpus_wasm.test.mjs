import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";

import {
  buildGitHubUrlCases,
  buildMinimalWorkspaceFiles,
  resolveLocalAssetWorkspacePath,
} from "./lib/wasm_corpus.mjs";

test("resolveLocalAssetWorkspacePath handles relative and repo-root image references", () => {
  assert.equal(
    resolveLocalAssetWorkspacePath("docs/README.md", "../images/logo.png"),
    "images/logo.png",
  );
  assert.equal(
    resolveLocalAssetWorkspacePath("docs/README.md", "/shared/banner.png"),
    "shared/banner.png",
  );
});

test("resolveLocalAssetWorkspacePath rejects paths that escape the repository root", () => {
  assert.equal(resolveLocalAssetWorkspacePath("README.md", "../../../etc/passwd"), null);
});

test("buildGitHubUrlCases emits repo and blob URLs for a corpus entry", () => {
  assert.deepEqual(
    buildGitHubUrlCases({
      url: "https://github.com/example/project",
      defaultBranch: "main",
      readmePath: "docs/README.md",
    }),
    [
      {
        label: "repo",
        url: "https://github.com/example/project",
      },
      {
        label: "blob",
        url: "https://github.com/example/project/blob/main/docs/README.md",
      },
    ],
  );
});

test("buildMinimalWorkspaceFiles includes the README and referenced local images once", async () => {
  const sourceRoot = await fs.mkdtemp(path.join(os.tmpdir(), "marknest-wasm-corpus-"));
  await fs.mkdir(path.join(sourceRoot, "docs"), { recursive: true });
  await fs.mkdir(path.join(sourceRoot, "images"), { recursive: true });
  await fs.mkdir(path.join(sourceRoot, "shared"), { recursive: true });
  await fs.writeFile(
    path.join(sourceRoot, "docs", "README.md"),
    [
      "# Guide",
      "",
      "![Logo](../images/logo.png)",
      "<img src=\"/shared/banner.png\" alt=\"Banner\" />",
      "![Dup](../images/logo.png)",
    ].join("\n"),
  );
  await fs.writeFile(path.join(sourceRoot, "images", "logo.png"), "logo");
  await fs.writeFile(path.join(sourceRoot, "shared", "banner.png"), "banner");

  const files = await buildMinimalWorkspaceFiles({
    sourceRoot,
    readmePath: "docs/README.md",
    wrapperDirectoryName: "project-main",
  });

  assert.deepEqual(
    files.map((file) => file.path),
    [
      "project-main/docs/README.md",
      "project-main/images/logo.png",
      "project-main/shared/banner.png",
    ],
  );
});
