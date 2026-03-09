import test from "node:test";
import assert from "node:assert/strict";

import {
  loadGitHubWorkspaceArchive,
  pickGitHubDirectoryEntryPath,
  resolveLocalAssetWorkspacePath,
} from "./github_workspace_loader.mjs";

test("resolveLocalAssetWorkspacePath normalizes local image references without escaping the repo root", () => {
  assert.equal(resolveLocalAssetWorkspacePath("docs/README.md", "../images/logo.png"), "images/logo.png");
  assert.equal(resolveLocalAssetWorkspacePath("README.md", "/shared/banner.png"), "shared/banner.png");
  assert.equal(resolveLocalAssetWorkspacePath("README.md", "https://example.com/logo.png"), null);
  assert.equal(resolveLocalAssetWorkspacePath("README.md", "../../../etc/passwd"), null);
});

test("pickGitHubDirectoryEntryPath prefers README-style entries inside a directory", () => {
  assert.equal(
    pickGitHubDirectoryEntryPath("docs", [
      { path: "docs/guide.md", type: "file" },
      { path: "docs/README.md", type: "file" },
      { path: "docs/README.ko.md", type: "file" },
    ]),
    "docs/README.md",
  );

  assert.equal(
    pickGitHubDirectoryEntryPath("", [
      { path: "guide.md", type: "file" },
      { path: "README.md", type: "file" },
    ]),
    "README.md",
  );
});

test("loadGitHubWorkspaceArchive builds a wrapped ZIP from a bare repository URL", async () => {
  const fetchRequests = [];
  const wasmCalls = [];
  const assetBytes = new Uint8Array([137, 80, 78, 71]);

  const result = await loadGitHubWorkspaceArchive({
    input: "https://github.com/example/project",
    wasm: {
      buildPdfArchive(entries) {
        wasmCalls.push(entries);
        return new Uint8Array([1, 2, 3]);
      },
    },
    fetchImpl: async (url) => {
      fetchRequests.push(url);

      if (url === "https://api.github.com/repos/example/project") {
        return Response.json({ default_branch: "main" });
      }

      if (url === "https://api.github.com/repos/example/project/readme?ref=main") {
        return Response.json({
          path: "README.md",
          content: Buffer.from(
            [
              "# Example",
              "",
              "![Logo](./images/logo.png)",
              "![Remote](https://example.com/banner.png)",
            ].join("\n"),
            "utf8",
          ).toString("base64"),
          encoding: "base64",
        });
      }

      if (url === "https://api.github.com/repos/example/project/contents/images/logo.png?ref=main") {
        return Response.json({
          path: "images/logo.png",
          content: Buffer.from(assetBytes).toString("base64"),
          encoding: "base64",
        });
      }

      throw new Error(`Unexpected fetch: ${url}`);
    },
  });

  assert.deepEqual(fetchRequests, [
    "https://api.github.com/repos/example/project",
    "https://api.github.com/repos/example/project/readme?ref=main",
    "https://api.github.com/repos/example/project/contents/images/logo.png?ref=main",
  ]);
  assert.equal(result.selectedEntryPath, "README.md");
  assert.equal(result.archiveFileName, "project-main.zip");
  assert.equal(result.sourceLabel, "github.com/example/project@main");
  assert.deepEqual(
    wasmCalls[0].map((entry) => entry.path),
    [
      "project-main/README.md",
      "project-main/images/logo.png",
    ],
  );
  assert.match(new TextDecoder().decode(wasmCalls[0][0].bytes), /# Example/);
  assert.deepEqual(Array.from(wasmCalls[0][1].bytes), Array.from(assetBytes));
  assert.deepEqual(Array.from(result.zipBytes), [1, 2, 3]);
});

test("loadGitHubWorkspaceArchive preserves an explicit blob entry path", async () => {
  const fetchRequests = [];
  const wasmCalls = [];

  const result = await loadGitHubWorkspaceArchive({
    input: "https://github.com/example/project/blob/develop/docs/guide.md",
    wasm: {
      buildPdfArchive(entries) {
        wasmCalls.push(entries);
        return new Uint8Array([9, 9, 9]);
      },
    },
    fetchImpl: async (url) => {
      fetchRequests.push(url);

      if (url === "https://api.github.com/repos/example/project/contents/docs/guide.md?ref=develop") {
        return Response.json({
          path: "docs/guide.md",
          content: Buffer.from("# Guide\n", "utf8").toString("base64"),
          encoding: "base64",
        });
      }

      throw new Error(`Unexpected fetch: ${url}`);
    },
  });

  assert.deepEqual(fetchRequests, [
    "https://api.github.com/repos/example/project/contents/docs/guide.md?ref=develop",
  ]);
  assert.equal(result.selectedEntryPath, "docs/guide.md");
  assert.equal(result.archiveFileName, "project-develop.zip");
  assert.deepEqual(
    wasmCalls[0].map((entry) => entry.path),
    ["project-develop/docs/guide.md"],
  );
});
