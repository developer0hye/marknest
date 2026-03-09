import init, * as wasmBindings from "./marknest_wasm.js";

import {
  SAMPLE_ARCHIVE_FILE_NAME,
  SAMPLE_ARCHIVE_ROOT,
  buildWasmExampleAnalyzeOptions,
  buildWasmExampleArchiveEntries,
  buildWasmExampleRenderOptions,
  encodeArchiveEntries,
  extractRuntimeScriptUrls,
  pickExampleEntryPath,
} from "./wasm-example-support.mjs";
import {
  parseGitHubInputUrl,
  resolveGitHubUrlEntryPath,
} from "./github_url_input.mjs";
import { loadGitHubWorkspaceArchive } from "./github_workspace_loader.mjs";
import { hasFailedRemoteAssets, materializeRemoteImages } from "./remote_assets.mjs";

const state = {
  wasm: null,
  zipBytes: null,
  loadedFileName: SAMPLE_ARCHIVE_FILE_NAME,
  sourceDetail: null,
  projectIndex: null,
  selectedEntry: null,
  sampleGitHubUrl: "https://github.com/example/marknest-demo",
};

const elements = {
  status: document.getElementById("status"),
  zipInput: document.getElementById("zip-input"),
  loadSample: document.getElementById("load-sample"),
  analyze: document.getElementById("analyze-button"),
  render: document.getElementById("render-button"),
  stripZipPrefix: document.getElementById("strip-zip-prefix"),
  githubUrlInput: document.getElementById("github-url-input"),
  renderFromGitHubUrl: document.getElementById("render-from-github-url"),
  runtimeAssetsBaseUrl: document.getElementById("runtime-assets-base-url"),
  mermaidMode: document.getElementById("mermaid-mode"),
  mathMode: document.getElementById("math-mode"),
  theme: document.getElementById("theme"),
  sourceName: document.getElementById("source-name"),
  sourceDetail: document.getElementById("source-detail"),
  entrySelect: document.getElementById("entry-select"),
  entryCount: document.getElementById("entry-count"),
  selectedEntry: document.getElementById("selected-entry"),
  diagnostics: document.getElementById("diagnostics"),
  projectIndexJson: document.getElementById("project-index-json"),
  apiCall: document.getElementById("api-call"),
  previewMeta: document.getElementById("preview-meta"),
  previewFrame: document.getElementById("preview-frame"),
  mermaidScriptUrl: document.getElementById("mermaid-script-url"),
  mathScriptUrl: document.getElementById("math-script-url"),
};

function setStatus(message, tone = "ready") {
  elements.status.dataset.tone = tone;
  elements.status.textContent = message;
}

function syncSourceSummary() {
  elements.sourceName.textContent = state.loadedFileName ?? "No archive loaded";
  if (typeof state.sourceDetail === "string" && state.sourceDetail.length > 0) {
    elements.sourceDetail.textContent = state.sourceDetail;
    return;
  }

  elements.sourceDetail.textContent = "Loaded from a ZIP file and analyzed locally in the browser.";
}

function renderProjectIndex() {
  const projectIndex = state.projectIndex;
  if (!projectIndex) {
    elements.entrySelect.replaceChildren();
    elements.entryCount.textContent = "0";
    elements.selectedEntry.textContent = "None";
    elements.diagnostics.textContent = "Analyze a ZIP archive to inspect entry candidates and diagnostics.";
    elements.projectIndexJson.textContent = "{\n  \"status\": \"waiting\"\n}";
    return;
  }

  elements.entryCount.textContent = String(projectIndex.entry_candidates.length);
  elements.selectedEntry.textContent = state.selectedEntry ?? "None";

  const summary = [
    `warnings: ${projectIndex.diagnostic?.warnings?.length ?? 0}`,
    `missing assets: ${projectIndex.diagnostic?.missing_assets?.length ?? 0}`,
    `path errors: ${projectIndex.diagnostic?.path_errors?.length ?? 0}`,
  ];
  elements.diagnostics.textContent = summary.join(" | ");
  elements.projectIndexJson.textContent = JSON.stringify(projectIndex, null, 2);

  elements.entrySelect.replaceChildren();
  for (const candidate of projectIndex.entry_candidates) {
    const option = document.createElement("option");
    option.value = candidate.path;
    option.textContent = candidate.path;
    option.selected = candidate.path === state.selectedEntry;
    elements.entrySelect.appendChild(option);
  }
}

function currentAnalyzeOptions() {
  return buildWasmExampleAnalyzeOptions({
    stripZipPrefix: elements.stripZipPrefix.checked,
  });
}

function currentRenderOptions() {
  return buildWasmExampleRenderOptions({
    stripZipPrefix: elements.stripZipPrefix.checked,
    runtimeAssetsBaseUrl: elements.runtimeAssetsBaseUrl.value,
    mermaidMode: elements.mermaidMode.value,
    mathMode: elements.mathMode.value,
    theme: elements.theme.value,
  });
}

function entryAssetRefs(entryPath) {
  return Array.isArray(state.projectIndex?.assets)
    ? state.projectIndex.assets.filter((asset) => asset.entry_path === entryPath)
    : [];
}

function renderCallSnapshot(renderOptions = null) {
  const parsedGitHubUrl = parseGitHubInputUrl(elements.githubUrlInput.value);
  const snapshot = {
    githubUrlInput: parsedGitHubUrl ?? null,
    analyzeZipWithOptions: currentAnalyzeOptions(),
    renderHtml:
      state.selectedEntry && renderOptions
        ? {
            entry_path: state.selectedEntry,
            options: renderOptions,
          }
        : null,
  };

  elements.apiCall.textContent = JSON.stringify(snapshot, null, 2);
}

function clearPreview() {
  elements.previewMeta.textContent = "Render an entry to inspect the HTML produced by the WASM bindings.";
  elements.previewFrame.srcdoc =
    "<!doctype html><html><body style='margin:0;display:grid;place-items:center;min-height:100vh;font:16px/1.5 Georgia,serif;background:#f6efe5;color:#4f3728;'><div style='padding:2rem;text-align:center;max-width:30rem;'><h1 style='margin:0 0 1rem;'>WASM preview waiting</h1><p style='margin:0;'>Analyze a ZIP archive and render an entry to inspect the iframe output here.</p></div></body></html>";
  elements.mermaidScriptUrl.textContent = "-";
  elements.mathScriptUrl.textContent = "-";
  renderCallSnapshot();
}

function syncProjectIndex(projectIndex) {
  state.projectIndex = projectIndex;
  state.selectedEntry = pickExampleEntryPath(projectIndex);
  renderProjectIndex();
  clearPreview();
  renderCallSnapshot();
}

function loadSampleArchive() {
  const sampleFiles = encodeArchiveEntries(buildWasmExampleArchiveEntries());
  state.zipBytes = state.wasm.buildPdfArchive(sampleFiles);
  state.loadedFileName = SAMPLE_ARCHIVE_FILE_NAME;
  state.sourceDetail = `Built from in-memory files under ${SAMPLE_ARCHIVE_ROOT}/ to demonstrate wrapper ZIP handling.`;
  state.sampleGitHubUrl = "https://github.com/example/marknest-demo";
  elements.githubUrlInput.value = state.sampleGitHubUrl;
  syncSourceSummary();
  setStatus("Wrapped sample ZIP loaded. Analyzing with the current strip-prefix setting.");
  analyzeArchive();
}

async function loadUploadedArchive(file) {
  state.zipBytes = new Uint8Array(await file.arrayBuffer());
  state.loadedFileName = file.name;
  state.sourceDetail = "Loaded from a ZIP file and analyzed locally in the browser.";
  syncSourceSummary();
  setStatus(`Loaded ${file.name}. Analyzing with the current strip-prefix setting.`);
  analyzeArchive();
}

function analyzeArchive() {
  if (!state.wasm || !state.zipBytes) {
    setStatus("Load a sample or upload a ZIP archive first.", "warning");
    return;
  }

  try {
    const projectIndex = state.wasm.analyzeZipWithOptions(state.zipBytes, currentAnalyzeOptions());
    syncProjectIndex(projectIndex);
    if (!state.selectedEntry) {
      setStatus("Analysis complete, but no Markdown entry candidates were found.", "warning");
      return;
    }

    renderSelectedEntry();
  } catch (error) {
    state.projectIndex = null;
    state.selectedEntry = null;
    renderProjectIndex();
    clearPreview();
    setStatus(`Analysis failed: ${String(error)}`, "error");
  }
}

async function renderSelectedEntry() {
  if (!state.wasm || !state.zipBytes || !state.selectedEntry) {
    setStatus("Analyze a ZIP archive and choose an entry before rendering.", "warning");
    return;
  }

  const renderOptions = currentRenderOptions();

  try {
    const preview = state.wasm.renderHtml(state.zipBytes, state.selectedEntry, renderOptions);
    const remoteMaterialization = await materializeRemoteImages({
      html: preview.html,
      assets: entryAssetRefs(state.selectedEntry),
    });
    const runtimeScriptUrls = extractRuntimeScriptUrls(preview.html);

    elements.previewMeta.textContent = `${preview.title || "Untitled"} | ${state.selectedEntry}`;
    elements.previewFrame.srcdoc = remoteMaterialization.html;
    elements.mermaidScriptUrl.textContent = runtimeScriptUrls.mermaidScriptUrl ?? "Not injected";
    elements.mathScriptUrl.textContent = runtimeScriptUrls.mathScriptUrl ?? "Not injected";
    renderCallSnapshot(renderOptions);
    if (hasFailedRemoteAssets(remoteMaterialization.remoteAssets)) {
      setStatus(
        "Preview rendered with remote image warnings. Some external assets could not be materialized in the browser.",
        "warning",
      );
      return;
    }

    setStatus("Preview rendered. Inspect the iframe and runtime asset URLs below.");
  } catch (error) {
    clearPreview();
    setStatus(`Render failed: ${String(error)}`, "error");
  }
}

async function renderFromGitHubUrlInput() {
  const parsedUrl = parseGitHubInputUrl(elements.githubUrlInput.value);
  if (!parsedUrl) {
    setStatus("Enter a valid GitHub repository, tree, or blob URL.", "warning");
    renderCallSnapshot();
    return;
  }

  if (!state.wasm) {
    setStatus("WASM is still loading.", "warning");
    return;
  }

  setStatus("Fetching the GitHub workspace and rebuilding a local ZIP in the browser...", "warning");

  try {
    const githubWorkspace = await loadGitHubWorkspaceArchive({
      input: elements.githubUrlInput.value,
      wasm: state.wasm,
    });
    state.zipBytes = githubWorkspace.zipBytes;
    state.loadedFileName = githubWorkspace.archiveFileName;
    state.sourceDetail = `Fetched ${githubWorkspace.sourceLabel}, rebuilt a wrapper ZIP locally, and analyzed it with the same WASM pipeline.`;
    syncSourceSummary();

    const projectIndex = state.wasm.analyzeZipWithOptions(state.zipBytes, currentAnalyzeOptions());
    syncProjectIndex(projectIndex);

    const entryPath = resolveGitHubUrlEntryPath(elements.githubUrlInput.value, projectIndex)
      ?? githubWorkspace.selectedEntryPath;
    if (entryPath && projectIndex.entry_candidates.some((candidate) => candidate.path === entryPath)) {
      state.selectedEntry = entryPath;
      renderProjectIndex();
    }

    if (!state.selectedEntry) {
      setStatus("GitHub workspace loaded, but no Markdown entry was selected.", "warning");
      renderCallSnapshot();
      return;
    }

    renderSelectedEntry();
    setStatus(`GitHub workspace loaded from ${githubWorkspace.sourceLabel}.`);
  } catch (error) {
    setStatus(`GitHub load failed: ${String(error)}`, "error");
    renderCallSnapshot();
  }
}

async function boot() {
  setStatus("Loading marknest_wasm via Trunk output...", "warning");

  try {
    await init();
    state.wasm = wasmBindings;
    syncSourceSummary();
    clearPreview();
    renderProjectIndex();
    setStatus("WASM runtime ready. The wrapped sample ZIP will load automatically.");
    loadSampleArchive();
  } catch (error) {
    setStatus(`WASM boot failed: ${String(error)}`, "error");
  }
}

elements.loadSample.addEventListener("click", () => {
  if (!state.wasm) {
    setStatus("WASM is still loading.", "warning");
    return;
  }

  loadSampleArchive();
});

elements.zipInput.addEventListener("change", async (event) => {
  const file = event.target.files?.[0];
  if (!file) {
    return;
  }

  await loadUploadedArchive(file);
});

elements.analyze.addEventListener("click", analyzeArchive);
elements.render.addEventListener("click", () => {
  void renderSelectedEntry();
});
elements.renderFromGitHubUrl.addEventListener("click", renderFromGitHubUrlInput);

elements.entrySelect.addEventListener("change", (event) => {
  state.selectedEntry = event.target.value || null;
  renderProjectIndex();
  void renderSelectedEntry();
});

elements.stripZipPrefix.addEventListener("change", () => {
  if (!state.zipBytes) {
    return;
  }

  analyzeArchive();
});

for (const control of [elements.runtimeAssetsBaseUrl, elements.mermaidMode, elements.mathMode, elements.theme]) {
  control.addEventListener("change", () => {
    if (!state.selectedEntry) {
      return;
    }

    void renderSelectedEntry();
  });
}

elements.githubUrlInput.addEventListener("change", renderCallSnapshot);

boot();
