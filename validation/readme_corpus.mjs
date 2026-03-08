#!/usr/bin/env node

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import fsPromises from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { classifyValidationResult } from "./lib/diff_policy.mjs";
import { parseCorpusManifest, sanitizeCorpusId } from "./lib/manifest.mjs";
import { buildPageMetrics, comparePngBuffers } from "./lib/png_metrics.mjs";
import {
  buildSourceSnapshot,
  calculateHeadingCoverage,
  calculateTokenCoverage,
  hasMeaningfulPageText,
  splitExtractedPdfPages,
} from "./lib/text_metrics.mjs";

const SCRIPT_PATH = fileURLToPath(import.meta.url);
const VALIDATION_ROOT = path.dirname(SCRIPT_PATH);
const REPO_ROOT = path.resolve(VALIDATION_ROOT, "..");
const MANIFEST_PATH = path.join(VALIDATION_ROOT, "readme-corpus-50.tsv");
const CACHE_ROOT = path.join(VALIDATION_ROOT, ".cache", "readme-corpus-50");
const RUNS_ROOT = path.join(VALIDATION_ROOT, ".runs");
const BASELINE_ROOT = path.join(VALIDATION_ROOT, "baselines", "readme-corpus-50");
const MARKNEST_BINARY = path.join(REPO_ROOT, "target", "debug", "marknest");
const TOKEN_COVERAGE_THRESHOLD = 0.97;
const FALLBACK_BIN_DIRECTORIES = [
  "/opt/homebrew/bin",
  "/usr/local/bin",
  "/usr/bin",
  "/bin",
];
let hasBuiltMarknestBinary = false;

function fail(message) {
  throw new Error(message);
}

function parseCliArguments(argv) {
  const [subcommand, ...rest] = argv;
  if (!subcommand) {
    fail("Usage: node validation/readme_corpus.mjs <verify-manifest|fetch|run|bless> [--tier smoke|all] [--repo <id>] [--offline] [--force]");
  }

  const options = {
    tier: "all",
    repoId: null,
    offline: false,
    force: false,
  };

  for (let index = 0; index < rest.length; index += 1) {
    const argument = rest[index];
    if (argument === "--tier") {
      const value = rest[index + 1];
      if (!value || !["smoke", "all"].includes(value)) {
        fail("Expected --tier smoke or --tier all.");
      }
      options.tier = value;
      index += 1;
      continue;
    }
    if (argument === "--repo") {
      const value = rest[index + 1];
      if (!value) {
        fail("Expected a repo id after --repo.");
      }
      options.repoId = value;
      index += 1;
      continue;
    }
    if (argument === "--offline") {
      options.offline = true;
      continue;
    }
    if (argument === "--force") {
      options.force = true;
      continue;
    }
    fail(`Unsupported argument: ${argument}`);
  }

  return { subcommand, options };
}

function commandOutput(command, args, { cwd = REPO_ROOT, allowFailure = false, env = process.env } = {}) {
  const resolvedCommand = resolveCommandPath(command);
  const result = spawnSync(resolvedCommand, args, {
    cwd,
    env,
    encoding: "utf8",
  });
  if (result.error) {
    throw result.error;
  }
  if (!allowFailure && result.status !== 0) {
    throw new Error(
      `Command failed (${command} ${args.join(" ")}): ${result.stderr || result.stdout || result.status}`,
    );
  }
  return {
    status: result.status ?? 1,
    stdout: result.stdout ?? "",
    stderr: result.stderr ?? "",
  };
}

function resolveCommandPath(command) {
  if (command.includes(path.sep)) {
    return command;
  }

  const pathEntries = String(process.env.PATH ?? "")
    .split(path.delimiter)
    .filter((entry) => entry.length > 0);
  for (const directory of [...pathEntries, ...FALLBACK_BIN_DIRECTORIES]) {
    const candidate = path.join(directory, command);
    if (fs.existsSync(candidate)) {
      return candidate;
    }
  }

  return command;
}

function ensureCommandAvailable(command, probeArgs = ["--version"]) {
  const result = spawnSync(resolveCommandPath(command), probeArgs, {
    cwd: REPO_ROOT,
    encoding: "utf8",
  });
  if (result.error || result.status !== 0) {
    fail(`Required command is unavailable: ${command}`);
  }
}

async function ensureDirectory(directoryPath) {
  await fsPromises.mkdir(directoryPath, { recursive: true });
}

async function removeDirectory(directoryPath) {
  await fsPromises.rm(directoryPath, { recursive: true, force: true });
}

async function loadManifest() {
  const manifestText = await fsPromises.readFile(MANIFEST_PATH, "utf8");
  return parseCorpusManifest(manifestText);
}

function selectManifestEntries(entries, options) {
  let filteredEntries = options.tier === "smoke"
    ? entries.filter((entry) => entry.tier === "smoke")
    : entries;

  if (options.repoId) {
    filteredEntries = filteredEntries.filter((entry) => entry.id === options.repoId);
  }

  if (filteredEntries.length === 0) {
    fail("No corpus entries matched the requested selection.");
  }

  return filteredEntries;
}

function nowStamp() {
  return new Date().toISOString().replace(/[-:.TZ]/g, "").slice(0, 14);
}

async function writeJsonFile(filePath, value) {
  await ensureDirectory(path.dirname(filePath));
  await fsPromises.writeFile(filePath, `${JSON.stringify(value, null, 2)}\n`);
}

async function writeTextFile(filePath, value) {
  await ensureDirectory(path.dirname(filePath));
  await fsPromises.writeFile(filePath, value);
}

function archiveUrl(entry) {
  return `https://github.com/${entry.repo}/archive/${entry.pinnedSha}.tar.gz`;
}

function githubApiHeaders() {
  const headers = [
    "-H",
    "Accept: application/vnd.github+json",
    "-H",
    "User-Agent: marknest-validation",
  ];
  const token = process.env.GITHUB_TOKEN ?? process.env.GH_TOKEN;
  if (token) {
    headers.push("-H", `Authorization: Bearer ${token}`);
  }
  return headers;
}

function githubApiJson(apiPath) {
  const response = commandOutput("curl", [
    "-L",
    ...githubApiHeaders(),
    `https://api.github.com/${apiPath}`,
  ]);
  const parsed = JSON.parse(response.stdout);
  if (parsed?.message === "Not Found") {
    fail(`GitHub API returned Not Found for ${apiPath}.`);
  }
  return parsed;
}

function cacheDirectoryForEntry(entry) {
  return path.join(CACHE_ROOT, `${entry.id}-${entry.pinnedSha.slice(0, 12)}`);
}

async function fetchEntry(entry, { force = false } = {}) {
  const cacheDirectory = cacheDirectoryForEntry(entry);
  const metadataPath = path.join(cacheDirectory, "metadata.json");
  if (!force && fs.existsSync(metadataPath)) {
    return JSON.parse(await fsPromises.readFile(metadataPath, "utf8"));
  }

  await removeDirectory(cacheDirectory);
  await ensureDirectory(cacheDirectory);
  const archivePath = path.join(cacheDirectory, "archive.tar.gz");
  const extractRoot = path.join(cacheDirectory, "source");
  await ensureDirectory(extractRoot);

  commandOutput("curl", ["-L", "-o", archivePath, archiveUrl(entry)]);
  commandOutput("tar", ["-xzf", archivePath, "-C", extractRoot]);

  const extractedEntries = await fsPromises.readdir(extractRoot, { withFileTypes: true });
  const extractedDirectory = extractedEntries.find((directoryEntry) => directoryEntry.isDirectory());
  if (!extractedDirectory) {
    fail(`No extracted source directory was found for ${entry.id}.`);
  }

  const sourceRoot = path.join(extractRoot, extractedDirectory.name);
  const readmeAbsolutePath = path.join(sourceRoot, entry.readmePath);
  await fsPromises.access(readmeAbsolutePath);

  const metadata = {
    id: entry.id,
    repo: entry.repo,
    pinnedSha: entry.pinnedSha,
    sourceRoot,
    readmeAbsolutePath,
    archivePath,
  };
  await writeJsonFile(metadataPath, metadata);
  return metadata;
}

function manifestRowToVerificationFailure(entry, fieldName, actualValue) {
  return `${entry.id}: ${fieldName} mismatch. manifest=${JSON.stringify(entry[fieldName])} actual=${JSON.stringify(actualValue)}`;
}

async function verifyManifestEntryOnline(entry) {
  const repositoryResponse = githubApiJson(`repos/${entry.repo}`);
  const readmeResponse = githubApiJson(`repos/${entry.repo}/readme`);
  const commitResponse = githubApiJson(`repos/${entry.repo}/commits/${entry.pinnedSha}`);

  const failures = [];
  if (sanitizeCorpusId(entry.repo) !== entry.id) {
    failures.push(`${entry.id}: id does not match the sanitized repo name.`);
  }
  if (repositoryResponse.full_name !== entry.repo) {
    failures.push(manifestRowToVerificationFailure(entry, "repo", repositoryResponse.full_name));
  }
  if (repositoryResponse.html_url !== entry.url) {
    failures.push(manifestRowToVerificationFailure(entry, "url", repositoryResponse.html_url));
  }
  if (repositoryResponse.default_branch !== entry.defaultBranch) {
    failures.push(
      manifestRowToVerificationFailure(entry, "defaultBranch", repositoryResponse.default_branch),
    );
  }
  if (readmeResponse.path !== entry.readmePath) {
    failures.push(manifestRowToVerificationFailure(entry, "readmePath", readmeResponse.path));
  }
  if (commitResponse.sha !== entry.pinnedSha) {
    failures.push(manifestRowToVerificationFailure(entry, "pinnedSha", commitResponse.sha));
  }
  return failures;
}

async function ensureMarknestBinary() {
  if (!hasBuiltMarknestBinary) {
    commandOutput("cargo", ["build", "-p", "marknest"]);
    hasBuiltMarknestBinary = true;
  }
  await fsPromises.access(MARKNEST_BINARY);
  return MARKNEST_BINARY;
}

function listPngPages(directoryPath) {
  if (!fs.existsSync(directoryPath)) {
    return [];
  }
  return fs
    .readdirSync(directoryPath)
    .filter((name) => name.endsWith(".png"))
    .sort((left, right) => left.localeCompare(right));
}

async function rasterizePdf(pdfPath, pagesDirectory) {
  await ensureDirectory(pagesDirectory);
  commandOutput("pdftoppm", ["-png", "-r", "96", pdfPath, path.join(pagesDirectory, "page")]);
  const rasterizedFiles = listPngPages(pagesDirectory).filter((name) => /^page-\d+\.png$/.test(name));
  let pageIndex = 1;
  for (const rasterizedFile of rasterizedFiles) {
    const currentPath = path.join(pagesDirectory, rasterizedFile);
    const renamedPath = path.join(
      pagesDirectory,
      `page-${String(pageIndex).padStart(4, "0")}.png`,
    );
    if (currentPath !== renamedPath) {
      await fsPromises.rename(currentPath, renamedPath);
    }
    pageIndex += 1;
  }
  return listPngPages(pagesDirectory);
}

function extractPdfText(pdfPath) {
  return commandOutput("pdftotext", ["-layout", pdfPath, "-"]).stdout;
}

async function computePageMetrics(pagesDirectory, pageTexts) {
  const pageFiles = listPngPages(pagesDirectory);
  const pageMetrics = [];
  for (const pageFile of pageFiles) {
    const pageBuffer = await fsPromises.readFile(path.join(pagesDirectory, pageFile));
    const pagePngModule = await import("pngjs");
    const pagePng = pagePngModule.PNG.sync.read(pageBuffer);
    const pageIndex = pageMetrics.length;
    pageMetrics.push({
      page: pageFile,
      hasMeaningfulText: hasMeaningfulPageText(pageTexts[pageIndex] ?? ""),
      ...buildPageMetrics(pagePng),
    });
  }
  return pageMetrics;
}

async function compareAgainstBaseline(entry, runDirectory) {
  const baselineDirectory = path.join(BASELINE_ROOT, entry.id);
  if (!fs.existsSync(baselineDirectory)) {
    return {
      baselineExists: false,
      baselinePageCountDelta: 0,
      diffSummary: [],
    };
  }

  const baselinePagesDirectory = path.join(baselineDirectory, "pages");
  const currentPagesDirectory = path.join(runDirectory, "pages");
  const baselinePages = listPngPages(baselinePagesDirectory);
  const currentPages = listPngPages(currentPagesDirectory);
  const sharedPageCount = Math.min(baselinePages.length, currentPages.length);
  const diffsDirectory = path.join(runDirectory, "diffs");
  await ensureDirectory(diffsDirectory);

  const diffSummary = [];
  for (let index = 0; index < sharedPageCount; index += 1) {
    const baselinePageBuffer = await fsPromises.readFile(path.join(baselinePagesDirectory, baselinePages[index]));
    const currentPageBuffer = await fsPromises.readFile(path.join(currentPagesDirectory, currentPages[index]));
    const diff = comparePngBuffers(baselinePageBuffer, currentPageBuffer);
    const diffPath = path.join(diffsDirectory, baselinePages[index]);
    await fsPromises.writeFile(diffPath, diff.diffPng);
    diffSummary.push({
      page: baselinePages[index],
      diffPixels: diff.diffPixels,
      diffRatio: diff.diffRatio,
      diffPath,
    });
  }

  return {
    baselineExists: true,
    baselinePageCountDelta: currentPages.length - baselinePages.length,
    diffSummary,
  };
}

function buildSourceMetadata(entry, sourceSnapshot) {
  return {
    id: entry.id,
    repo: entry.repo,
    url: entry.url,
    pinned_sha: entry.pinnedSha,
    default_branch: entry.defaultBranch,
    readme_path: entry.readmePath,
    stars_at_curation: entry.starsAtCuration,
    headings: sourceSnapshot.headings,
    local_image_references: sourceSnapshot.localImageReferences,
    remote_image_references: sourceSnapshot.remoteImageReferences,
    source_tokens: sourceSnapshot.sourceTokens,
  };
}

function filterSelectedEntryDiagnostics(values, entryPath) {
  if (!Array.isArray(values)) {
    return [];
  }

  return values.filter((value) => String(value).startsWith(`${entryPath} -> `));
}

async function runEntryValidation(entry, runRoot, { requireBaseline = true, forceFetch = false } = {}) {
  const fetchMetadata = await fetchEntry(entry, { force: forceFetch });
  const runDirectory = path.join(runRoot, entry.id);
  await removeDirectory(runDirectory);
  await ensureDirectory(runDirectory);
  const pagesDirectory = path.join(runDirectory, "pages");
  const sourceMarkdown = await fsPromises.readFile(fetchMetadata.readmeAbsolutePath, "utf8");
  const sourceSnapshot = buildSourceSnapshot(sourceMarkdown);
  await writeJsonFile(path.join(runDirectory, "source.json"), buildSourceMetadata(entry, sourceSnapshot));

  const binaryPath = await ensureMarknestBinary();
  const pdfPath = path.join(runDirectory, "output.pdf");
  const reportPath = path.join(runDirectory, "report.json");
  const assetManifestPath = path.join(runDirectory, "asset-manifest.json");
  const startedAt = Date.now();
  const convertResult = commandOutput(
    binaryPath,
    [
      "convert",
      fetchMetadata.readmeAbsolutePath,
      "--mermaid",
      "auto",
      "--math",
      "auto",
      "--render-report",
      reportPath,
      "--asset-manifest",
      assetManifestPath,
      "-o",
      pdfPath,
    ],
    { allowFailure: true, cwd: fetchMetadata.sourceRoot },
  );
  const elapsedSeconds = Math.round((Date.now() - startedAt) / 1000);
  await writeTextFile(
    path.join(runDirectory, "convert.log"),
    `${convertResult.stdout}${convertResult.stderr}`,
  );

  const report = fs.existsSync(reportPath)
    ? JSON.parse(await fsPromises.readFile(reportPath, "utf8"))
    : {};
  const assetManifest = fs.existsSync(assetManifestPath)
    ? JSON.parse(await fsPromises.readFile(assetManifestPath, "utf8"))
    : {};
  const pdfExists = fs.existsSync(pdfPath);
  const pdfText = pdfExists ? extractPdfText(pdfPath) : "";
  const pdfTextPages = splitExtractedPdfPages(pdfText);
  const pageFiles = pdfExists ? await rasterizePdf(pdfPath, pagesDirectory) : [];
  const pageMetrics = pdfExists ? await computePageMetrics(pagesDirectory, pdfTextPages) : [];
  const headingCoverage = calculateHeadingCoverage(sourceSnapshot, pdfText);
  const tokenCoverage = calculateTokenCoverage(sourceSnapshot, pdfText);
  const comparison = await compareAgainstBaseline(entry, runDirectory);
  const localMissingAssets = [
    ...filterSelectedEntryDiagnostics(assetManifest.missing_assets, entry.readmePath),
    ...filterSelectedEntryDiagnostics(assetManifest.path_errors, entry.readmePath),
  ];
  const remoteAssetFailures = (Array.isArray(report.remote_assets) ? report.remote_assets : [])
    .filter((remoteAsset) => remoteAsset.status === "failed")
    .map((remoteAsset) => remoteAsset.fetch_url || remoteAsset.original_reference);
  const nearBlankPages = pageMetrics
    .filter((metric) => metric.nearBlank && !metric.hasMeaningfulText)
    .map((metric) => metric.page);
  const edgeContactPages = pageMetrics
    .filter((metric) => metric.rightEdgeInkRatio > 0 || metric.bottomEdgeInkRatio > 0)
    .map((metric) => ({
      page: metric.page,
      right_edge_ink_ratio: metric.rightEdgeInkRatio,
      bottom_edge_ink_ratio: metric.bottomEdgeInkRatio,
    }));

  const classification = classifyValidationResult({
    convertExitCode: convertResult.status,
    reportStatus: report.status ?? "missing",
    reportErrors: Array.isArray(report.errors) ? report.errors.length : 0,
    localMissingAssets,
    headingCoverage,
    tokenCoverage,
    nearBlankPages,
    baselinePageCountDelta: comparison.baselinePageCountDelta,
    remoteAssetFailures,
  });
  if (requireBaseline && !comparison.baselineExists) {
    classification.hardFailures.push("baseline_missing");
  }
  if (edgeContactPages.length > 0) {
    classification.advisories.push("edge_contact_detected");
  }

  const metrics = {
    id: entry.id,
    repo: entry.repo,
    url: entry.url,
    pinned_sha: entry.pinnedSha,
    elapsed_sec: elapsedSeconds,
    convert_exit_code: convertResult.status,
    report_status: report.status ?? "missing",
    pdf_bytes: pdfExists ? (await fsPromises.stat(pdfPath)).size : 0,
    page_count: pageFiles.length,
    token_coverage: tokenCoverage.coverage,
    missing_tokens: tokenCoverage.missingTokens,
    heading_coverage: headingCoverage.coverage,
    missing_headings: headingCoverage.missingHeadings,
    local_missing_assets: localMissingAssets,
    remote_asset_failures: remoteAssetFailures,
    near_blank_pages: nearBlankPages,
    edge_contact_pages: edgeContactPages,
    baseline_exists: comparison.baselineExists,
    baseline_page_count_delta: comparison.baselinePageCountDelta,
    diff_summary: comparison.diffSummary,
    hard_failures: [...new Set(classification.hardFailures)].sort(),
    advisories: [...new Set(classification.advisories)].sort(),
    thresholds: {
      token_coverage_minimum: TOKEN_COVERAGE_THRESHOLD,
    },
  };

  await writeJsonFile(path.join(runDirectory, "metrics.json"), metrics);

  return {
    entry,
    runDirectory,
    pdfPath,
    reportPath,
    assetManifestPath,
    sourcePath: path.join(runDirectory, "source.json"),
    metricsPath: path.join(runDirectory, "metrics.json"),
    report,
    assetManifest,
    metrics,
  };
}

async function copyDirectory(sourcePath, destinationPath) {
  await removeDirectory(destinationPath);
  await ensureDirectory(path.dirname(destinationPath));
  await fsPromises.cp(sourcePath, destinationPath, { recursive: true });
}

function buildSummaryRows(results) {
  return results.map((result) => ({
    id: result.entry.id,
    repo: result.entry.repo,
    hard_failures: result.metrics.hard_failures.join(","),
    advisories: result.metrics.advisories.join(","),
    elapsed_sec: result.metrics.elapsed_sec,
    convert_exit_code: result.metrics.convert_exit_code,
    report_status: result.metrics.report_status,
    page_count: result.metrics.page_count,
    pdf_bytes: result.metrics.pdf_bytes,
    token_coverage: result.metrics.token_coverage.toFixed(3),
    heading_coverage: result.metrics.heading_coverage.toFixed(3),
  }));
}

function rowsToTsv(rows) {
  if (rows.length === 0) {
    return "";
  }
  const headers = Object.keys(rows[0]);
  const lines = [headers.join("\t")];
  for (const row of rows) {
    lines.push(headers.map((header) => String(row[header] ?? "")).join("\t"));
  }
  return `${lines.join("\n")}\n`;
}

async function writeRunSummary(runRoot, results) {
  const summaryRows = buildSummaryRows(results);
  await writeTextFile(path.join(runRoot, "summary.tsv"), rowsToTsv(summaryRows));
  await writeJsonFile(path.join(runRoot, "summary.json"), summaryRows);
}

async function blessResults(results) {
  ensureCommandAvailable("git", ["lfs", "version"]);
  for (const result of results) {
    if (result.metrics.hard_failures.length > 0) {
      continue;
    }
    const baselineDirectory = path.join(BASELINE_ROOT, result.entry.id);
    await removeDirectory(baselineDirectory);
    await ensureDirectory(baselineDirectory);
    await fsPromises.copyFile(result.pdfPath, path.join(baselineDirectory, "output.pdf"));
    await fsPromises.copyFile(result.reportPath, path.join(baselineDirectory, "report.json"));
    await fsPromises.copyFile(
      result.assetManifestPath,
      path.join(baselineDirectory, "asset-manifest.json"),
    );
    await fsPromises.copyFile(result.sourcePath, path.join(baselineDirectory, "source.json"));
    await fsPromises.copyFile(result.metricsPath, path.join(baselineDirectory, "metrics.json"));
    await copyDirectory(path.join(result.runDirectory, "pages"), path.join(baselineDirectory, "pages"));
  }
}

async function commandVerifyManifest(entries, options) {
  if (options.offline) {
    for (const entry of entries) {
      if (sanitizeCorpusId(entry.repo) !== entry.id) {
        fail(`Manifest id mismatch for ${entry.repo}.`);
      }
    }
    console.log(`Verified ${entries.length} manifest entries offline.`);
    return 0;
  }

  ensureCommandAvailable("curl");
  const failures = [];
  for (const entry of entries) {
    const entryFailures = await verifyManifestEntryOnline(entry);
    failures.push(...entryFailures);
  }
  if (failures.length > 0) {
    fail(`Manifest verification failed:\n- ${failures.join("\n- ")}`);
  }
  console.log(`Verified ${entries.length} manifest entries against GitHub.`);
  return 0;
}

async function commandFetch(entries, options) {
  ensureCommandAvailable("curl");
  ensureCommandAvailable("tar", ["--version"]);
  for (const entry of entries) {
    const metadata = await fetchEntry(entry, { force: options.force });
    console.log(`${entry.id}\t${metadata.sourceRoot}`);
  }
  return 0;
}

async function commandRun(entries, options) {
  ensureCommandAvailable("curl");
  ensureCommandAvailable("tar", ["--version"]);
  ensureCommandAvailable("pdftoppm", ["-v"]);
  ensureCommandAvailable("pdftotext", ["-v"]);
  const runRoot = path.join(RUNS_ROOT, `run-${nowStamp()}`);
  await ensureDirectory(runRoot);

  const results = [];
  for (const entry of entries) {
    results.push(await runEntryValidation(entry, runRoot, { requireBaseline: true, forceFetch: options.force }));
  }
  await writeRunSummary(runRoot, results);

  const hardFailureCount = results.reduce(
    (count, result) => count + result.metrics.hard_failures.length,
    0,
  );
  console.log(`Run artifacts: ${runRoot}`);
  return hardFailureCount > 0 ? 1 : 0;
}

async function commandBless(entries, options) {
  ensureCommandAvailable("curl");
  ensureCommandAvailable("tar", ["--version"]);
  ensureCommandAvailable("pdftoppm", ["-v"]);
  ensureCommandAvailable("pdftotext", ["-v"]);
  const runRoot = path.join(RUNS_ROOT, `bless-${nowStamp()}`);
  await ensureDirectory(runRoot);

  const results = [];
  for (const entry of entries) {
    results.push(await runEntryValidation(entry, runRoot, { requireBaseline: false, forceFetch: options.force }));
  }
  await writeRunSummary(runRoot, results);
  await blessResults(results);

  const hardFailureCount = results.reduce(
    (count, result) => count + result.metrics.hard_failures.length,
    0,
  );
  console.log(`Bless artifacts: ${runRoot}`);
  return hardFailureCount > 0 ? 1 : 0;
}

async function main() {
  const { subcommand, options } = parseCliArguments(process.argv.slice(2));
  const manifestEntries = await loadManifest();
  const selectedEntries = selectManifestEntries(manifestEntries, options);

  if (subcommand === "verify-manifest") {
    process.exitCode = await commandVerifyManifest(selectedEntries, options);
    return;
  }
  if (subcommand === "fetch") {
    process.exitCode = await commandFetch(selectedEntries, options);
    return;
  }
  if (subcommand === "run") {
    process.exitCode = await commandRun(selectedEntries, options);
    return;
  }
  if (subcommand === "bless") {
    process.exitCode = await commandBless(selectedEntries, options);
    return;
  }

  fail(`Unsupported subcommand: ${subcommand}`);
}

main().catch((error) => {
  console.error(String(error instanceof Error ? error.message : error));
  process.exitCode = 1;
});
