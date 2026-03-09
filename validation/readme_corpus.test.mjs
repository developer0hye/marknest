import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { PNG } from "pngjs";

import {
  sanitizeBaselineMetrics,
  sanitizeBaselineReport,
} from "./lib/baseline_artifacts.mjs";
import { classifyValidationResult } from "./lib/diff_policy.mjs";
import { parseCorpusManifest, sanitizeCorpusId } from "./lib/manifest.mjs";
import {
  buildSourceSnapshot,
  calculateHeadingCoverage,
  calculateTokenCoverage,
  hasMeaningfulPageText,
  splitExtractedPdfPages,
} from "./lib/text_metrics.mjs";
import {
  buildPageMetrics,
  comparePngBuffers,
  isNearBlankPage,
} from "./lib/png_metrics.mjs";

const TEST_FILE_PATH = fileURLToPath(import.meta.url);
const VALIDATION_ROOT = path.dirname(TEST_FILE_PATH);
const MANIFEST_PATH = path.join(VALIDATION_ROOT, "readme-corpus-60.tsv");

test("parseCorpusManifest accepts the pinned corpus schema", () => {
  const manifestText = [
    "id\ttier\tcategory\trepo\turl\tdefault_branch\tpinned_sha\treadme_path\tstars_at_curation\texpected_patterns\tselection_reason",
    "mermaid-js--mermaid\tsmoke\tdiagram_math\tmermaid-js/mermaid\thttps://github.com/mermaid-js/mermaid\tdevelop\t0123456789abcdef0123456789abcdef01234567\tREADME.md\t81234\tmermaid,badges\tDiagram-heavy README.",
  ].join("\n");

  const manifest = parseCorpusManifest(manifestText);

  assert.equal(manifest.length, 1);
  assert.equal(manifest[0].id, "mermaid-js--mermaid");
  assert.equal(manifest[0].tier, "smoke");
  assert.equal(manifest[0].defaultBranch, "develop");
  assert.equal(manifest[0].readmePath, "README.md");
  assert.equal(manifest[0].starsAtCuration, 81234);
});

test("parseCorpusManifest rejects duplicate ids", () => {
  const manifestText = [
    "id\ttier\tcategory\trepo\turl\tdefault_branch\tpinned_sha\treadme_path\tstars_at_curation\texpected_patterns\tselection_reason",
    "dup\tsmoke\ttest\towner/one\thttps://github.com/owner/one\tmain\t0123456789abcdef0123456789abcdef01234567\tREADME.md\t1\tbadges\tOne.",
    "dup\textended\ttest\towner/two\thttps://github.com/owner/two\tmain\tfedcba9876543210fedcba9876543210fedcba98\tREADME.md\t2\tcode\tTwo.",
  ].join("\n");

  assert.throws(() => parseCorpusManifest(manifestText), /duplicate manifest id/i);
});

test("sanitizeCorpusId lowercases and stabilizes repo names", () => {
  assert.equal(sanitizeCorpusId("PrefectHQ/prefect"), "prefecthq--prefect");
});

test("committed corpus manifest contains 60 entries including 10 math cases", async () => {
  const manifestText = await fs.readFile(MANIFEST_PATH, "utf8");
  const manifest = parseCorpusManifest(manifestText);

  assert.equal(manifest.length, 60);
  assert.equal(manifest.filter((entry) => entry.category === "math").length, 10);
});

test("buildSourceSnapshot extracts headings and image references outside fenced code", () => {
  const markdown = `
# Title

![Local](./images/cover.png)
![Remote](https://example.com/logo.png)

## Details

\`\`\`md
# not-a-heading
![Nope](./ignored.png)
\`\`\`
`;

  const snapshot = buildSourceSnapshot(markdown);

  assert.deepEqual(snapshot.headings, ["title", "details"]);
  assert.deepEqual(snapshot.localImageReferences, ["./images/cover.png"]);
  assert.deepEqual(snapshot.remoteImageReferences, ["https://example.com/logo.png"]);
});

test("buildSourceSnapshot strips raw html and urls from heading labels", () => {
  const snapshot = buildSourceSnapshot(`
### Flowchart <a href="https://example.com/docs">Docs</a> - https://example.com/live
`);

  assert.deepEqual(snapshot.headings, ["flowchart docs"]);
});

test("buildSourceSnapshot decodes common html entities before heading and token analysis", () => {
  const snapshot = buildSourceSnapshot(`
# React &middot;

Use &amp; enjoy the docs.
`);

  assert.deepEqual(snapshot.headings, ["react"]);
  assert.deepEqual(snapshot.sourceTokens, ["react", "use", "enjoy", "the", "docs"]);
});

test("calculateTokenCoverage ignores markdown syntax noise and rewards preserved text", () => {
  const snapshot = buildSourceSnapshot(`
# MarkNest

Render [docs](https://example.com) safely.
![Badge](https://img.shields.io/badge/build-passing)
`);

  const coverage = calculateTokenCoverage(snapshot, "MarkNest Render docs safely");

  assert.equal(coverage.missingTokens.length, 0);
  assert.equal(coverage.coverage, 1);
});

test("buildSourceSnapshot excludes fenced code and markdown tables from blocking prose tokens", () => {
  const snapshot = buildSourceSnapshot(`
# Title

Visible prose stays.

| model | map |
| --- | --- |
| yolo26n-seg | 42.1 |

\`\`\`python
fib_n = fib(n - 1) + fib(n - 2)
\`\`\`
`);

  assert.deepEqual(snapshot.sourceTokens, ["title", "visible", "prose", "stays"]);
});

test("buildSourceSnapshot ignores reference-style image badges and link-definition labels", () => {
  const snapshot = buildSourceSnapshot(`
# Flutter

[![Discord badge][]][Discord instructions]

Read the [set of widgets][widget catalog] for more details.

[Discord instructions]: ./docs/contributing/Chat.md
[Discord badge]: https://img.shields.io/discord/608014603317936148?logo=discord
[widget catalog]: https://docs.flutter.dev/ui/widgets
`);

  assert.deepEqual(snapshot.sourceTokens, [
    "flutter",
    "read",
    "the",
    "set",
    "widgets",
    "for",
    "more",
    "details",
  ]);
});

test("buildSourceSnapshot collapses badge-heavy headings and emphasis-fragmented words", () => {
  const snapshot = buildSourceSnapshot(`
# Serde &emsp; [![Build Status]][actions] [![Latest Version]][crates.io]

**Serde is a framework for *ser*ializing and *de*serializing Rust data structures efficiently and generically.**

[Build Status]: https://img.shields.io/github/actions/workflow/status/serde-rs/serde/ci.yml?branch=master
[actions]: https://github.com/serde-rs/serde/actions?query=branch%3Amaster
[Latest Version]: https://img.shields.io/crates/v/serde.svg
[crates.io]: https://crates.io/crates/serde
`);

  assert.deepEqual(snapshot.headings, ["serde"]);
  assert.deepEqual(snapshot.sourceTokens, [
    "serde",
    "framework",
    "for",
    "serializing",
    "and",
    "deserializing",
    "rust",
    "data",
    "structures",
    "efficiently",
    "generically",
  ]);
});

test("calculateHeadingCoverage reports missing level 1-3 headings", () => {
  const snapshot = buildSourceSnapshot(`
# Intro
## Install
### Verify
`);

  const coverage = calculateHeadingCoverage(snapshot, "Intro\nVerify");

  assert.equal(coverage.coverage, 2 / 3);
  assert.deepEqual(coverage.missingHeadings, ["install"]);
});

test("splitExtractedPdfPages trims a trailing form-feed-only page chunk", () => {
  const pages = splitExtractedPdfPages("Page one\fPage two\f");

  assert.deepEqual(pages, ["Page one", "Page two"]);
});

test("hasMeaningfulPageText keeps sparse textual license pages out of near-blank failures", () => {
  assert.equal(
    hasMeaningfulPageText("Copyright (c) Microsoft Corporation. All rights reserved."),
    true,
  );
  assert.equal(hasMeaningfulPageText("\n \n\f"), false);
});

test("isNearBlankPage detects almost-white pages", () => {
  const png = new PNG({ width: 10, height: 10 });
  png.data.fill(255);

  assert.equal(isNearBlankPage(png), true);

  png.data[0] = 0;
  png.data[1] = 0;
  png.data[2] = 0;
  png.data[3] = 255;
  assert.equal(isNearBlankPage(png), false);
});

test("buildPageMetrics reports right-edge contact for possible clipping", () => {
  const png = new PNG({ width: 20, height: 20 });
  png.data.fill(255);

  for (let y = 0; y < png.height; y += 1) {
    const pixelIndex = (y * png.width + (png.width - 1)) * 4;
    png.data[pixelIndex] = 0;
    png.data[pixelIndex + 1] = 0;
    png.data[pixelIndex + 2] = 0;
    png.data[pixelIndex + 3] = 255;
  }

  const metrics = buildPageMetrics(png);

  assert.equal(metrics.nearBlank, false);
  assert.ok(metrics.rightEdgeInkRatio > 0);
  assert.ok(metrics.rightEdgeInkRatio > metrics.bottomEdgeInkRatio);
});

test("comparePngBuffers returns a diff ratio and image buffer", () => {
  const left = new PNG({ width: 4, height: 4 });
  const right = new PNG({ width: 4, height: 4 });
  left.data.fill(255);
  right.data.fill(255);

  right.data[0] = 0;
  right.data[1] = 0;
  right.data[2] = 0;
  right.data[3] = 255;

  const diff = comparePngBuffers(PNG.sync.write(left), PNG.sync.write(right));

  assert.ok(diff.diffPixels > 0);
  assert.ok(diff.diffRatio > 0);
  assert.ok(Buffer.isBuffer(diff.diffPng));
});

test("classifyValidationResult blocks on content loss and keeps reflow-only changes advisory", () => {
  const blocking = classifyValidationResult({
    convertExitCode: 0,
    reportStatus: "success",
    reportErrors: 0,
    localMissingAssets: ["images/cover.png"],
    headingCoverage: { coverage: 0.5, missingHeadings: ["install"] },
    tokenCoverage: { coverage: 0.94, missingTokens: ["install"] },
    nearBlankPages: [],
    baselinePageCountDelta: 2,
    remoteAssetFailures: ["https://example.com/logo.png"],
  });

  assert.equal(blocking.hardFailures.length > 0, true);
  assert.equal(blocking.advisories.includes("baseline_page_count_changed"), true);

  const advisoryOnly = classifyValidationResult({
    convertExitCode: 0,
    reportStatus: "success",
    reportErrors: 0,
    localMissingAssets: [],
    headingCoverage: { coverage: 1, missingHeadings: [] },
    tokenCoverage: { coverage: 0.99, missingTokens: [] },
    nearBlankPages: [],
    baselinePageCountDelta: 3,
    remoteAssetFailures: ["https://example.com/logo.png"],
  });

  assert.deepEqual(advisoryOnly.hardFailures, []);
  assert.deepEqual(advisoryOnly.advisories.sort(), [
    "baseline_page_count_changed",
    "remote_asset_failures",
  ]);
});

test("classifyValidationResult does not block on warning exit codes alone", () => {
  const warningOnly = classifyValidationResult({
    convertExitCode: 1,
    reportStatus: "warning",
    reportErrors: 0,
    localMissingAssets: [],
    headingCoverage: { coverage: 1, missingHeadings: [] },
    tokenCoverage: { coverage: 1, missingTokens: [] },
    nearBlankPages: [],
    baselinePageCountDelta: 0,
    remoteAssetFailures: [],
  });

  assert.deepEqual(warningOnly.hardFailures, []);
  assert.deepEqual(warningOnly.advisories, ["convert_warning_exit"]);
});

test("sanitizeBaselineReport replaces ephemeral absolute paths with portable paths", () => {
  const sanitized = sanitizeBaselineReport(
    {
      input_path: "/Users/yhkwon/tmp/source/README.md",
      outputs: [
        {
          entry_path: "README.md",
          output_path: "/Users/yhkwon/tmp/run/output.pdf",
          warnings: [],
        },
      ],
      remote_assets: [{ original_reference: "https://example.com/logo.png", status: "inlined" }],
    },
    { readmePath: "README.md" },
  );

  assert.equal(sanitized.input_path, "README.md");
  assert.equal(sanitized.outputs[0].output_path, "output.pdf");
  assert.deepEqual(sanitized.remote_assets, [
    { original_reference: "https://example.com/logo.png", status: "inlined" },
  ]);
});

test("sanitizeBaselineMetrics removes baseline-diff noise from committed metrics", () => {
  const sanitized = sanitizeBaselineMetrics({
    baseline_exists: false,
    baseline_page_count_delta: 3,
    diff_summary: [
      {
        page: "page-0001.png",
        diffPixels: 25,
        diffRatio: 0.02,
        diffPath: "/Users/yhkwon/tmp/run/diffs/page-0001.png",
      },
    ],
    advisories: ["baseline_page_count_changed", "remote_asset_failures"],
  });

  assert.equal(sanitized.baseline_exists, true);
  assert.equal(sanitized.baseline_page_count_delta, 0);
  assert.deepEqual(sanitized.diff_summary, []);
  assert.deepEqual(sanitized.advisories, ["remote_asset_failures"]);
});
