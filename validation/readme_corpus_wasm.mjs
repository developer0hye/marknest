#!/usr/bin/env node

import { spawnSync } from "node:child_process";
import { createRequire } from "node:module";
import fs from "node:fs";
import fsPromises from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

import {
  buildGitHubArchiveRequest,
  resolveGitHubUrlEntryPath,
} from "../web/github_url_input.mjs";
import { parseCorpusManifest } from "./lib/manifest.mjs";
import {
  buildGitHubUrlCases,
  buildMinimalWorkspaceFiles,
} from "./lib/wasm_corpus.mjs";

const SCRIPT_PATH = fileURLToPath(import.meta.url);
const VALIDATION_ROOT = path.dirname(SCRIPT_PATH);
const REPO_ROOT = path.resolve(VALIDATION_ROOT, "..");
const MANIFEST_PATH = path.join(VALIDATION_ROOT, "readme-corpus-60.tsv");
const CACHE_ROOT = path.join(VALIDATION_ROOT, ".cache", "readme-corpus-60");
const RUNS_ROOT = path.join(VALIDATION_ROOT, ".runs");
const WASM_PACKAGE_ROOT = path.join(VALIDATION_ROOT, ".cache", "wasm-node-pkg");

function fail(message) {
  throw new Error(message);
}

function parseCliArguments(argv) {
  const [subcommand, ...rest] = argv;
  if (subcommand !== "run") {
    fail("Usage: node validation/readme_corpus_wasm.mjs run [--tier smoke|all] [--repo <id>] [--runtime-assets-base-url <url>] [--force-build]");
  }

  const options = {
    tier: "all",
    repoId: null,
    runtimeAssetsBaseUrl: "/runtime-assets",
    forceBuild: false,
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
    if (argument === "--runtime-assets-base-url") {
      const value = rest[index + 1];
      if (!value) {
        fail("Expected a URL after --runtime-assets-base-url.");
      }
      options.runtimeAssetsBaseUrl = value;
      index += 1;
      continue;
    }
    if (argument === "--force-build") {
      options.forceBuild = true;
      continue;
    }
    fail(`Unsupported argument: ${argument}`);
  }

  return options;
}

function commandOutput(command, args, { cwd = REPO_ROOT, allowFailure = false } = {}) {
  const result = spawnSync(command, args, {
    cwd,
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
  return result;
}

async function ensureDirectory(directoryPath) {
  await fsPromises.mkdir(directoryPath, { recursive: true });
}

async function removeDirectory(directoryPath) {
  await fsPromises.rm(directoryPath, { recursive: true, force: true });
}

async function loadManifestEntries() {
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

function cacheDirectoryForEntry(entry) {
  const prefix = `${entry.id}-`;
  const directoryName = fs
    .readdirSync(CACHE_ROOT, { withFileTypes: true })
    .find((directoryEntry) => directoryEntry.isDirectory() && directoryEntry.name.startsWith(prefix))
    ?.name;
  return directoryName ? path.join(CACHE_ROOT, directoryName) : null;
}

async function loadCachedMetadata(entry) {
  const cacheDirectory = cacheDirectoryForEntry(entry);
  if (!cacheDirectory) {
    return null;
  }

  const metadataPath = path.join(cacheDirectory, "metadata.json");
  if (!fs.existsSync(metadataPath)) {
    return null;
  }

  return JSON.parse(await fsPromises.readFile(metadataPath, "utf8"));
}

async function ensureWasmPackage(forceBuild) {
  if (forceBuild) {
    await removeDirectory(WASM_PACKAGE_ROOT);
  }

  if (!fs.existsSync(path.join(WASM_PACKAGE_ROOT, "marknest_wasm.js"))) {
    await ensureDirectory(path.dirname(WASM_PACKAGE_ROOT));
    commandOutput("wasm-pack", [
      "build",
      "crates/marknest-wasm",
      "--target",
      "nodejs",
      "--dev",
      "--out-dir",
      WASM_PACKAGE_ROOT,
    ]);
  }

  return WASM_PACKAGE_ROOT;
}

async function loadWasmBindings(forceBuild) {
  const packageRoot = await ensureWasmPackage(forceBuild);
  const require = createRequire(import.meta.url);
  return require(path.join(packageRoot, "marknest_wasm.js"));
}

function extractRuntimeScriptUrls(html) {
  const mermaidMatch = /"mermaidScript":"([^"]+)"/.exec(String(html));
  const mathMatch = /"mathScript":"([^"]+)"/.exec(String(html));
  return {
    mermaidScriptUrl: mermaidMatch?.[1] ?? null,
    mathScriptUrl: mathMatch?.[1] ?? null,
  };
}

function buildRenderOptions(runtimeAssetsBaseUrl) {
  return {
    theme: "github",
    mermaid_mode: "auto",
    math_mode: "auto",
    runtime_assets_base_url: runtimeAssetsBaseUrl,
    strip_zip_prefix: true,
  };
}

async function runEntryCase(wasmBindings, entry, metadata, urlCase, runtimeAssetsBaseUrl) {
  const wrapperDirectoryName = `${entry.repo.split("/")[1]}-${entry.pinnedSha.slice(0, 12)}`;
  const files = await buildMinimalWorkspaceFiles({
    sourceRoot: metadata.sourceRoot,
    readmePath: entry.readmePath,
    wrapperDirectoryName,
  });
  const zipBytes = wasmBindings.buildPdfArchive(files);
  const archiveRequest = buildGitHubArchiveRequest(urlCase.url);
  const projectIndex = wasmBindings.analyzeZipWithOptions(zipBytes, {
    strip_zip_prefix: archiveRequest?.stripZipPrefix ?? true,
  });
  const entryPath = resolveGitHubUrlEntryPath(urlCase.url, projectIndex);
  if (!entryPath) {
    throw new Error(`Could not resolve an entry path for ${urlCase.url}`);
  }
  if (!projectIndex.entry_candidates.some((candidate) => candidate.path === entryPath)) {
    throw new Error(`Resolved entry ${entryPath} is not available in the analyzed project index.`);
  }

  const preview = wasmBindings.renderHtml(zipBytes, entryPath, buildRenderOptions(runtimeAssetsBaseUrl));
  const runtimeScriptUrls = extractRuntimeScriptUrls(preview.html);

  return {
    url_case: urlCase.label,
    input_url: urlCase.url,
    selected_entry: entryPath,
    analyzed_selected_entry: projectIndex.selected_entry ?? null,
    entry_candidates: projectIndex.entry_candidates.map((candidate) => candidate.path),
    html_length: preview.html.length,
    title: preview.title,
    mermaid_script_url: runtimeScriptUrls.mermaidScriptUrl,
    math_script_url: runtimeScriptUrls.mathScriptUrl,
  };
}

function classifyCaseResult(caseResult, runtimeAssetsBaseUrl) {
  const failures = [];
  if (caseResult.html_length <= 0) {
    failures.push("empty_html");
  }
  if (
    typeof runtimeAssetsBaseUrl === "string"
    && runtimeAssetsBaseUrl.trim().length > 0
    && !String(caseResult.mermaid_script_url ?? "").startsWith(runtimeAssetsBaseUrl.trim())
  ) {
    failures.push("runtime_asset_base_url_not_applied");
  }
  if (!caseResult.entry_candidates.includes(caseResult.selected_entry)) {
    failures.push("selected_entry_missing_from_candidates");
  }
  return failures;
}

function buildSummaryRows(results) {
  return results.map((result) => ({
    id: result.id,
    repo: result.repo,
    status: result.status,
    case_count: result.caseResults.length,
    failure_count: result.failures.length,
    failures: result.failures.join(","),
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

function nowStamp() {
  return new Date().toISOString().replace(/[-:.TZ]/g, "").slice(0, 14);
}

async function writeRunSummary(runRoot, results) {
  const summaryRows = buildSummaryRows(results);
  await fsPromises.writeFile(path.join(runRoot, "summary.tsv"), rowsToTsv(summaryRows));
  await fsPromises.writeFile(
    path.join(runRoot, "summary.json"),
    `${JSON.stringify(summaryRows, null, 2)}\n`,
  );
  await fsPromises.writeFile(
    path.join(runRoot, "results.json"),
    `${JSON.stringify(results, null, 2)}\n`,
  );
}

async function runCorpus(options) {
  const manifestEntries = selectManifestEntries(await loadManifestEntries(), options);
  const wasmBindings = await loadWasmBindings(options.forceBuild);
  const runRoot = path.join(RUNS_ROOT, `wasm-corpus-${nowStamp()}`);
  await ensureDirectory(runRoot);
  const results = [];

  for (const entry of manifestEntries) {
    const metadata = await loadCachedMetadata(entry);
    if (!metadata) {
      results.push({
        id: entry.id,
        repo: entry.repo,
        status: "failed",
        failures: ["cache_missing"],
        caseResults: [],
      });
      continue;
    }

    const caseResults = [];
    const failures = [];
    for (const urlCase of buildGitHubUrlCases(entry)) {
      try {
        const caseResult = await runEntryCase(
          wasmBindings,
          entry,
          metadata,
          urlCase,
          options.runtimeAssetsBaseUrl,
        );
        caseResults.push(caseResult);
        failures.push(...classifyCaseResult(caseResult, options.runtimeAssetsBaseUrl));
      } catch (error) {
        failures.push(`${urlCase.label}:${String(error.message ?? error)}`);
      }
    }

    results.push({
      id: entry.id,
      repo: entry.repo,
      status: failures.length === 0 ? "passed" : "failed",
      failures: [...new Set(failures)].sort(),
      caseResults,
    });
    console.log(
      `${entry.id}: ${failures.length === 0 ? "passed" : "failed"} (${caseResults.length} case${caseResults.length === 1 ? "" : "s"})`,
    );
  }

  await writeRunSummary(runRoot, results);
  const failedCount = results.filter((result) => result.status !== "passed").length;
  console.log(`WASM corpus run complete: ${results.length - failedCount}/${results.length} passed`);
  console.log(`Artifacts: ${runRoot}`);
  process.exitCode = failedCount === 0 ? 0 : 1;
}

const options = parseCliArguments(process.argv.slice(2));
runCorpus(options).catch((error) => {
  console.error(error instanceof Error ? error.message : String(error));
  process.exitCode = 1;
});
