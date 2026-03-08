import {
  buildFallbackUrl,
  evaluateArchiveScale,
  normalizeFallbackBaseUrl,
  resolveExportBackend,
} from "./export_policy.mjs";
import {
  DEFAULT_OUTPUT_OPTIONS,
  buildFallbackFormData,
  buildOutputOptions,
  debugBundleFileName,
} from "./output_options.mjs";
import {
  hasFailedRemoteAssets,
  materializeRemoteImages,
} from "./remote_assets.mjs";
import {
  hasBlockingRuntimeErrors,
  mergeProjectDiagnostics,
  runtimeDiagnosticsForEntry,
  waitForFrameRenderStatus,
} from "./runtime_sync.mjs";

const state = {
  wasm: null,
  zipBytes: null,
  projectIndex: null,
  selectedEntry: null,
  renderedPreview: null,
  previewRuntimeDiagnostics: null,
  previewRenderVersion: 0,
  isBusy: false,
  loadedFileName: null,
  qualityMode: "browser",
  fallbackBaseUrl: "http://127.0.0.1:3476",
  archiveScale: evaluateArchiveScale(0),
  previewRefreshTimer: null,
};

const elements = {
  zipInput: document.getElementById("zip-input"),
  fileName: document.getElementById("file-name"),
  statusChip: document.getElementById("status-chip"),
  statusMessage: document.getElementById("status-message"),
  entryCount: document.getElementById("entry-count"),
  missingCount: document.getElementById("missing-count"),
  warningCount: document.getElementById("warning-count"),
  selectedEntryLabel: document.getElementById("selected-entry-label"),
  entryList: document.getElementById("entry-list"),
  warningList: document.getElementById("warning-list"),
  errorList: document.getElementById("error-list"),
  previewTitle: document.getElementById("preview-title"),
  previewCaption: document.getElementById("preview-caption"),
  previewFrame: document.getElementById("preview-frame"),
  downloadSelected: document.getElementById("download-selected"),
  downloadBatch: document.getElementById("download-batch"),
  downloadDebug: document.getElementById("download-debug"),
  qualityMode: document.getElementById("quality-mode"),
  fallbackUrl: document.getElementById("fallback-url"),
  scaleNote: document.getElementById("scale-note"),
  theme: document.getElementById("theme-input"),
  customCss: document.getElementById("css-input"),
  pageSize: document.getElementById("page-size-input"),
  marginTopMm: document.getElementById("margin-top-input"),
  marginRightMm: document.getElementById("margin-right-input"),
  marginBottomMm: document.getElementById("margin-bottom-input"),
  marginLeftMm: document.getElementById("margin-left-input"),
  landscape: document.getElementById("landscape-input"),
  enableToc: document.getElementById("toc-input"),
  sanitizeHtml: document.getElementById("sanitize-html-input"),
  mermaidMode: document.getElementById("mermaid-mode-input"),
  mathMode: document.getElementById("math-mode-input"),
  title: document.getElementById("title-input"),
  author: document.getElementById("author-input"),
  subject: document.getElementById("subject-input"),
  headerTemplate: document.getElementById("header-template-input"),
  footerTemplate: document.getElementById("footer-template-input"),
  headerPreview: document.getElementById("header-preview-frame"),
  footerPreview: document.getElementById("footer-preview-frame"),
};

const BROWSER_RUNTIME_INFO = {
  renderer: "browser-wasm",
  marknest_version: "0.1.0",
  asset_mode: "bundled_local",
  pdf_engine: "html2pdf.js",
  mermaid_version: "11.11.0",
  mathjax_version: "3.2.2",
  html2pdf_version: "0.10.1",
  mermaid_script_url: "./runtime-assets/mermaid/mermaid.min.js",
  math_script_url: "./runtime-assets/mathjax/es5/tex-svg.js",
  html2pdf_script_url: "./runtime-assets/html2pdf/html2pdf.bundle.min.js",
};

function setStatus(kind, label, message) {
  elements.statusChip.textContent = label;
  elements.statusChip.className = `status-chip status-${kind}`;
  elements.statusMessage.textContent = message;
}

function setBusy(isBusy) {
  state.isBusy = isBusy;
  syncActionButtons();
}

function setTextList(target, items, fallback) {
  target.replaceChildren();
  if (items.length === 0) {
    const item = document.createElement("li");
    item.textContent = fallback;
    target.appendChild(item);
    return;
  }

  for (const value of items) {
    const item = document.createElement("li");
    item.textContent = value;
    target.appendChild(item);
  }
}

function clearPreviewRuntimeDiagnostics() {
  state.previewRuntimeDiagnostics = null;
}

function currentPreviewRuntimeDiagnostics() {
  const runtimeDiagnostics = state.previewRuntimeDiagnostics;
  if (!runtimeDiagnostics || runtimeDiagnostics.entryPath !== state.selectedEntry) {
    return null;
  }

  return runtimeDiagnostics.optionsKey === currentOutputOptionsKey() ? runtimeDiagnostics : null;
}

function currentPreviewRemoteDiagnostics() {
  const preview = state.renderedPreview;
  if (!preview || preview.entryPath !== state.selectedEntry) {
    return null;
  }

  if (preview.optionsKey !== currentOutputOptionsKey()) {
    return null;
  }

  return {
    warnings: Array.isArray(preview.remoteWarnings) ? preview.remoteWarnings : [],
    errors: [],
  };
}

function currentPreviewSupplementalDiagnostics() {
  const runtimeDiagnostics = currentPreviewRuntimeDiagnostics();
  const remoteDiagnostics = currentPreviewRemoteDiagnostics();
  if (!runtimeDiagnostics && !remoteDiagnostics) {
    return null;
  }

  return {
    warnings: [
      ...(runtimeDiagnostics?.warnings ?? []),
      ...(remoteDiagnostics?.warnings ?? []),
    ],
    errors: [...(runtimeDiagnostics?.errors ?? [])],
  };
}

function syncDiagnosticLists(projectIndex = state.projectIndex) {
  if (!projectIndex) {
    setTextList(elements.warningList, [], "Warnings will appear after analysis.");
    setTextList(elements.errorList, [], "No errors.");
    return;
  }

  setTextList(elements.warningList, collectWarnings(projectIndex), "No warnings.");
  setTextList(elements.errorList, collectErrors(projectIndex), "No errors.");
}

function collectWarnings(projectIndex) {
  return mergeProjectDiagnostics(projectIndex, currentPreviewSupplementalDiagnostics()).warnings;
}

function collectErrors(projectIndex) {
  return mergeProjectDiagnostics(projectIndex, currentPreviewSupplementalDiagnostics()).errors;
}

function hasDiagnostics() {
  if (!state.projectIndex) {
    return false;
  }

  return (
    collectWarnings(state.projectIndex).length > 0 || collectErrors(state.projectIndex).length > 0
  );
}

function escapeHtml(value) {
  return String(value)
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

function deriveSelectedPreviewTitle() {
  if (state.renderedPreview?.title) {
    return state.renderedPreview.title;
  }

  const entryName = state.selectedEntry?.split("/").pop() ?? "README.md";
  return entryName.replace(/\.(md|markdown)$/i, "");
}

function currentOutputOptions() {
  return buildOutputOptions({
    theme: elements.theme?.value ?? DEFAULT_OUTPUT_OPTIONS.theme,
    customCss: elements.customCss?.value ?? "",
    headerTemplate: elements.headerTemplate?.value ?? "",
    footerTemplate: elements.footerTemplate?.value ?? "",
    title: elements.title?.value ?? "",
    author: elements.author?.value ?? "",
    subject: elements.subject?.value ?? "",
    pageSize: elements.pageSize?.value ?? DEFAULT_OUTPUT_OPTIONS.page_size,
    marginTopMm: elements.marginTopMm?.value ?? String(DEFAULT_OUTPUT_OPTIONS.margin_top_mm),
    marginRightMm: elements.marginRightMm?.value ?? String(DEFAULT_OUTPUT_OPTIONS.margin_right_mm),
    marginBottomMm:
      elements.marginBottomMm?.value ?? String(DEFAULT_OUTPUT_OPTIONS.margin_bottom_mm),
    marginLeftMm: elements.marginLeftMm?.value ?? String(DEFAULT_OUTPUT_OPTIONS.margin_left_mm),
    landscape: elements.landscape?.checked ?? false,
    enableToc: elements.enableToc?.checked ?? DEFAULT_OUTPUT_OPTIONS.enable_toc,
    sanitizeHtml: elements.sanitizeHtml?.checked ?? DEFAULT_OUTPUT_OPTIONS.sanitize_html,
    mermaidMode: elements.mermaidMode?.value ?? DEFAULT_OUTPUT_OPTIONS.mermaid_mode,
    mathMode: elements.mathMode?.value ?? DEFAULT_OUTPUT_OPTIONS.math_mode,
  });
}

function currentOutputOptionsKey(outputOptions = currentOutputOptions()) {
  return JSON.stringify(outputOptions);
}

function entryAssetRefs(entryPath) {
  return Array.isArray(state.projectIndex?.assets)
    ? state.projectIndex.assets.filter((asset) => asset.entry_path === entryPath)
    : [];
}

async function materializePreview(preview, outputOptions = currentOutputOptions()) {
  const entryPath = preview.entryPath ?? preview.entry_path;
  const remoteMaterialization = await materializeRemoteImages({
    html: preview.html,
    assets: entryAssetRefs(entryPath),
  });

  return {
    entryPath,
    optionsKey: currentOutputOptionsKey(outputOptions),
    title: preview.title,
    html: remoteMaterialization.html,
    remoteAssets: remoteMaterialization.remoteAssets,
    remoteWarnings: remoteMaterialization.warnings,
  };
}

function buildBrowserAssetManifest(entryPath, remoteAssets = []) {
  const entrySelector = `${entryPath} -> `;
  return {
    entry_path: entryPath,
    assets: entryAssetRefs(entryPath),
    remote_assets: remoteAssets,
    missing_assets: (state.projectIndex?.diagnostic?.missing_assets ?? []).filter((message) =>
      String(message).includes(entrySelector),
    ),
    path_errors: (state.projectIndex?.diagnostic?.path_errors ?? []).filter((message) =>
      String(message).includes(entrySelector),
    ),
    warnings: (state.projectIndex?.diagnostic?.warnings ?? []).filter((message) =>
      String(message).includes(entrySelector),
    ),
  };
}

function buildBrowserRenderReport(entryPath, outputOptions, remoteAssets = []) {
  const runtimeDiagnostics = currentPreviewSupplementalDiagnostics();
  const manifest = buildBrowserAssetManifest(entryPath, remoteAssets);
  return {
    status: "success",
    source_kind: state.projectIndex?.source_kind ?? "zip",
    selected_entry: entryPath,
    entry_candidates: (state.projectIndex?.entry_candidates ?? []).map((candidate) => candidate.path),
    warnings: [...manifest.warnings, ...(runtimeDiagnostics?.warnings ?? [])],
    errors: [...manifest.path_errors, ...(runtimeDiagnostics?.errors ?? [])],
    options: outputOptions,
    runtime_info: BROWSER_RUNTIME_INFO,
    remote_assets: remoteAssets,
  };
}

async function runtimeAssetFilesForHtml(html) {
  const files = [];
  const candidates = [
    {
      scriptUrl: "./runtime-assets/mermaid/mermaid.min.js",
      path: "runtime-assets/mermaid/mermaid.min.js",
    },
    {
      scriptUrl: "./runtime-assets/mathjax/es5/tex-svg.js",
      path: "runtime-assets/mathjax/es5/tex-svg.js",
    },
  ];

  for (const candidate of candidates) {
    if (!html.includes(candidate.scriptUrl)) {
      continue;
    }

    const response = await fetch(candidate.scriptUrl);
    if (!response.ok) {
      throw new Error(`Failed to load ${candidate.path} for the debug bundle.`);
    }

    files.push({
      path: candidate.path,
      bytes: new Uint8Array(await response.arrayBuffer()),
    });
  }

  return files;
}

function updateSummary(projectIndex) {
  elements.entryCount.textContent = String(projectIndex.entry_candidates.length);
  elements.missingCount.textContent = String(projectIndex.diagnostic.missing_assets.length);
  elements.warningCount.textContent = String(
    collectWarnings(projectIndex).length + collectErrors(projectIndex).length,
  );
  elements.selectedEntryLabel.textContent = state.selectedEntry ?? "None";
}

function syncActionButtons() {
  const hasSelectedEntry = Boolean(state.selectedEntry && state.wasm && state.zipBytes);
  const hasBatchEntries = Boolean(state.projectIndex?.entry_candidates?.length);
  elements.downloadSelected.disabled = !hasSelectedEntry || state.isBusy;
  elements.downloadBatch.disabled = !hasBatchEntries || state.isBusy;
  elements.downloadDebug.disabled = !hasSelectedEntry || state.isBusy;
}

function renderEntries(projectIndex) {
  elements.entryList.replaceChildren();
  if (projectIndex.entry_candidates.length === 0) {
    elements.entryList.classList.add("empty-state");
    elements.entryList.textContent = "This archive does not contain any Markdown entry candidates.";
    return;
  }

  elements.entryList.classList.remove("empty-state");

  for (const candidate of projectIndex.entry_candidates) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "entry-button";
    if (candidate.path === state.selectedEntry) {
      button.classList.add("is-selected");
    }
    button.addEventListener("click", () => renderPreview(candidate.path));

    const path = document.createElement("span");
    path.className = "entry-path";
    path.textContent = candidate.path;

    const reason = document.createElement("span");
    reason.className = "entry-reason";
    reason.textContent =
      candidate.path === projectIndex.selected_entry
        ? `Auto-selected by analysis (${projectIndex.entry_selection_reason})`
        : "Click to render this entry.";

    button.append(path, reason);
    elements.entryList.appendChild(button);
  }
}

function updateScaleNote() {
  const summary = state.archiveScale;
  const modeLabel = summary.isLargeArchive ? "Recommended: High Quality" : "Recommended: Browser";
  elements.scaleNote.textContent = `${summary.message} ${modeLabel}.`;
  elements.scaleNote.classList.toggle("is-warning", summary.isLargeArchive);
}

function templateSampleContext() {
  return {
    title: deriveSelectedPreviewTitle(),
    entryPath: state.selectedEntry ?? "docs/README.md",
    pageNumber: "3",
    totalPages: "12",
    date: "2026-03-08",
  };
}

function templateHasUnsafeMarkup(templateSource) {
  return /<script|javascript:/i.test(templateSource);
}

function fillTemplateTokens(templateSource) {
  const sample = templateSampleContext();
  return String(templateSource)
    .replaceAll("{{title}}", escapeHtml(sample.title))
    .replaceAll("{{entryPath}}", escapeHtml(sample.entryPath))
    .replaceAll("{{pageNumber}}", sample.pageNumber)
    .replaceAll("{{totalPages}}", sample.totalPages)
    .replaceAll("{{date}}", sample.date);
}

function templatePreviewDocument(templateSource, label) {
  if (!templateSource) {
    return `<!doctype html><html><body style="margin:0;font:13px/1.5 'Avenir Next',sans-serif;color:#6b7280;background:#f8f3ea;display:grid;place-items:center;min-height:100%;"><p style="margin:0;padding:1rem;text-align:center;">${escapeHtml(label)} preview is empty.</p></body></html>`;
  }

  if (templateHasUnsafeMarkup(templateSource)) {
    return `<!doctype html><html><body style="margin:0;font:13px/1.5 'Avenir Next',sans-serif;color:#8b2f39;background:#fff3f4;display:grid;place-items:center;min-height:100%;"><p style="margin:0;padding:1rem;text-align:center;">Unsafe template markup is blocked in preview.</p></body></html>`;
  }

  return `<!doctype html><html><body style="margin:0;padding:0.65rem;font:12px/1.4 'Avenir Next',sans-serif;color:#1f2937;background:#ffffff;">${fillTemplateTokens(templateSource)}</body></html>`;
}

function updateTemplatePreviews() {
  elements.headerPreview.srcdoc = templatePreviewDocument(
    elements.headerTemplate.value,
    "Header template",
  );
  elements.footerPreview.srcdoc = templatePreviewDocument(
    elements.footerTemplate.value,
    "Footer template",
  );
}

function schedulePreviewRefresh() {
  if (!state.selectedEntry || !state.wasm || !state.zipBytes || state.isBusy) {
    return;
  }

  state.previewRenderVersion += 1;
  state.renderedPreview = null;
  clearPreviewRuntimeDiagnostics();
  if (state.projectIndex) {
    updateSummary(state.projectIndex);
    syncDiagnosticLists(state.projectIndex);
  }

  window.clearTimeout(state.previewRefreshTimer);
  state.previewRefreshTimer = window.setTimeout(async () => {
    try {
      await renderPreview(state.selectedEntry);
    } catch {
      // The preview flow already updates visible status and diagnostics.
    }
  }, 180);
}

async function analyzeZip(file) {
  if (!state.wasm) {
    setStatus("waiting", "Booting", "The WASM runtime is still loading. Try again in a moment.");
    return;
  }

  state.archiveScale = evaluateArchiveScale(file.size);
  updateScaleNote();
  setStatus("ready", "Analyzing", `Reading ${file.name} and scanning Markdown entries in the browser.`);
  elements.fileName.textContent = file.name;
  state.loadedFileName = file.name;
  state.previewRenderVersion += 1;
  window.clearTimeout(state.previewRefreshTimer);
  clearPreviewRuntimeDiagnostics();
  setBusy(true);

  try {
    const arrayBuffer = await file.arrayBuffer();
    const zipBytes = new Uint8Array(arrayBuffer);
    const projectIndex = state.wasm.analyzeZip(zipBytes);
    state.zipBytes = zipBytes;
    state.projectIndex = projectIndex;
    state.selectedEntry =
      projectIndex.selected_entry ?? projectIndex.entry_candidates[0]?.path ?? null;
    state.renderedPreview = null;

    updateSummary(projectIndex);
    renderEntries(projectIndex);
    syncDiagnosticLists(projectIndex);
    updateTemplatePreviews();

    if (projectIndex.entry_candidates.length === 0) {
      elements.previewTitle.textContent = "No preview available";
      elements.previewCaption.textContent = "The uploaded archive did not expose a Markdown entry candidate.";
      setStatus("warning", "No entries", "Analysis completed, but there is no Markdown entry to preview.");
      syncActionButtons();
      return;
    }

    await renderPreview(state.selectedEntry);
  } catch (error) {
    state.previewRenderVersion += 1;
    state.zipBytes = null;
    state.projectIndex = null;
    state.selectedEntry = null;
    state.renderedPreview = null;
    clearPreviewRuntimeDiagnostics();
    updateSummary({
      entry_candidates: [],
      diagnostic: { missing_assets: [], warnings: [], path_errors: [] },
    });
    renderEntries({ entry_candidates: [] });
    setTextList(elements.warningList, [], "Warnings will appear after analysis.");
    setTextList(elements.errorList, [String(error)], "No errors.");
    elements.previewTitle.textContent = "Preview unavailable";
    elements.previewCaption.textContent = "The ZIP could not be analyzed.";
    updateTemplatePreviews();
    setStatus("error", "Failed", `ZIP analysis failed: ${String(error)}`);
    syncActionButtons();
  } finally {
    setBusy(false);
  }
}

function setReadyStatusForPreview(runtimeStatus, preview) {
  if (hasBlockingRuntimeErrors(runtimeStatus)) {
    setStatus(
      "error",
      "Runtime blocked",
      "Preview rendered, but Mermaid or Math runtime errors are blocking Browser Fast export. Use High Quality Fallback or change the render modes.",
    );
    return;
  }

  if (runtimeStatus.warnings.length > 0) {
    setStatus(
      "warning",
      "Ready",
      "Preview rendered with runtime warnings. Diagnostics show the affected Mermaid or Math output.",
    );
    return;
  }

  if ((preview?.remoteWarnings?.length ?? 0) > 0) {
    setStatus(
      "warning",
      "Ready",
      "Preview rendered with remote image warnings. Diagnostics show which remote assets could not be materialized in the browser.",
    );
    return;
  }

  if (hasDiagnostics()) {
    setStatus("warning", "Ready", "Preview rendered. Diagnostics are shown alongside the selected archive.");
  } else if (state.archiveScale.isLargeArchive) {
    setStatus(
      "warning",
      "Ready",
      "Preview rendered. This archive is large enough that High Quality server export is recommended.",
    );
  } else {
    setStatus("ready", "Ready", "Preview rendered successfully in the browser.");
  }
}

async function loadFrameSrcdoc(iframe, html) {
  await new Promise((resolve) => {
    iframe.addEventListener("load", resolve, { once: true });
    iframe.srcdoc = html;
  });
}

async function renderPreview(entryPath, outputOptions = currentOutputOptions()) {
  if (!state.wasm || !state.zipBytes || !state.projectIndex) {
    return;
  }

  const previewRenderVersion = state.previewRenderVersion + 1;
  state.previewRenderVersion = previewRenderVersion;
  state.selectedEntry = entryPath;
  state.renderedPreview = null;
  clearPreviewRuntimeDiagnostics();
  updateSummary(state.projectIndex);
  renderEntries(state.projectIndex);
  syncDiagnosticLists(state.projectIndex);
  updateTemplatePreviews();
  setStatus("ready", "Rendering", `Rendering ${entryPath} into preview HTML with the current output controls.`);

  const optionsKey = currentOutputOptionsKey(outputOptions);

  try {
    const preview = state.wasm.renderHtml(state.zipBytes, entryPath, outputOptions);
    const materializedPreview = await materializePreview(preview, outputOptions);
    if (previewRenderVersion !== state.previewRenderVersion) {
      return;
    }

    state.renderedPreview = materializedPreview;
    elements.previewTitle.textContent = materializedPreview.title;
    elements.previewCaption.textContent = entryPath;
    await loadFrameSrcdoc(elements.previewFrame, materializedPreview.html);
    if (previewRenderVersion !== state.previewRenderVersion) {
      return;
    }

    const runtimeStatus = await waitForFrameRenderStatus(elements.previewFrame);
    if (previewRenderVersion !== state.previewRenderVersion) {
      return;
    }

    state.previewRuntimeDiagnostics = {
      entryPath,
      optionsKey,
      ...runtimeDiagnosticsForEntry(entryPath, runtimeStatus),
    };
    updateSummary(state.projectIndex);
    syncDiagnosticLists(state.projectIndex);
    updateTemplatePreviews();
    setReadyStatusForPreview(runtimeStatus, materializedPreview);
    syncActionButtons();
  } catch (error) {
    if (previewRenderVersion !== state.previewRenderVersion) {
      return;
    }

    clearPreviewRuntimeDiagnostics();
    updateSummary(state.projectIndex);
    syncDiagnosticLists(state.projectIndex);
    elements.previewCaption.textContent = entryPath;
    if (!state.renderedPreview) {
      elements.previewTitle.textContent = "Preview unavailable";
    }

    const hasVisiblePreview =
      state.renderedPreview?.entryPath === entryPath && state.renderedPreview?.optionsKey === optionsKey;
    setStatus(
      "error",
      hasVisiblePreview ? "Runtime failed" : "Render failed",
      hasVisiblePreview
        ? `Preview rendered, but browser runtime completion failed: ${String(error)}`
        : `Preview rendering failed: ${String(error)}`,
    );
    syncActionButtons();
  }
}

function derivePdfPath(entryPath) {
  return entryPath.replace(/\.(md|markdown)$/i, ".pdf");
}

function derivePdfFileName(entryPath) {
  return derivePdfPath(entryPath).split("/").pop() ?? "document.pdf";
}

function deriveArchiveFileName() {
  const baseName = state.loadedFileName?.replace(/\.zip$/i, "") || "marknest";
  return `${baseName}-pdfs.zip`;
}

async function waitForFrameLoad(iframe) {
  await loadFrameSrcdoc(iframe, "<!doctype html><html><body></body></html>");
}

async function buildPdfBlobFromPreview(preview, fileName, options) {
  if (typeof window.html2pdf !== "function") {
    throw new Error("html2pdf.js is not available.");
  }

  const pageSize = options.page_size === "letter" ? "letter" : "a4";
  const orientation = options.landscape ? "landscape" : "portrait";
  const marginInches = [
    options.margin_top_mm / 25.4,
    options.margin_left_mm / 25.4,
    options.margin_bottom_mm / 25.4,
    options.margin_right_mm / 25.4,
  ];
  const iframe = document.createElement("iframe");
  iframe.setAttribute("aria-hidden", "true");
  iframe.tabIndex = -1;
  iframe.style.position = "fixed";
  iframe.style.top = "0";
  iframe.style.left = "-10000px";
  iframe.style.width = options.landscape ? "297mm" : "210mm";
  iframe.style.height = options.landscape ? "210mm" : "297mm";
  iframe.style.opacity = "0";
  iframe.style.pointerEvents = "none";
  document.body.appendChild(iframe);

  try {
    await waitForFrameLoad(iframe);
    await loadFrameSrcdoc(iframe, preview.html);

    const sourceDocument = iframe.contentDocument?.documentElement;
    if (!sourceDocument) {
      throw new Error("Preview document could not be mounted for PDF rendering.");
    }

    const entryPath = preview.entryPath ?? preview.entry_path ?? fileName;
    const runtimeStatus = await waitForFrameRenderStatus(iframe);
    const runtimeDiagnostics = runtimeDiagnosticsForEntry(entryPath, runtimeStatus);
    if (hasBlockingRuntimeErrors(runtimeStatus)) {
      throw new Error(runtimeDiagnostics.errors.join(" "));
    }

    const pdfBlob = await window
      .html2pdf()
      .set({
        filename: fileName,
        margin: marginInches,
        html2canvas: { scale: 2, useCORS: true, backgroundColor: "#ffffff" },
        jsPDF: { unit: "in", format: pageSize, orientation },
        pagebreak: { mode: ["css", "legacy"] },
      })
      .from(sourceDocument)
      .outputPdf("blob");

    return { pdfBlob, runtimeDiagnostics };
  } finally {
    iframe.remove();
  }
}

function triggerDownload(blob, fileName) {
  const objectUrl = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = objectUrl;
  anchor.download = fileName;
  document.body.appendChild(anchor);
  anchor.click();
  anchor.remove();
  setTimeout(() => URL.revokeObjectURL(objectUrl), 0);
}

async function ensureSelectedPreview(outputOptions = currentOutputOptions()) {
  if (!state.selectedEntry || !state.wasm || !state.zipBytes) {
    throw new Error("No entry is selected.");
  }

  const optionsKey = currentOutputOptionsKey(outputOptions);
  if (
    state.renderedPreview?.entryPath === state.selectedEntry &&
    state.renderedPreview?.optionsKey === optionsKey
  ) {
    return state.renderedPreview;
  }

  await renderPreview(state.selectedEntry, outputOptions);
  if (
    state.renderedPreview?.entryPath === state.selectedEntry &&
    state.renderedPreview?.optionsKey === optionsKey
  ) {
    return state.renderedPreview;
  }

  throw new Error(`Preview is not ready for ${state.selectedEntry}.`);
}

function currentFallbackBaseUrl() {
  return normalizeFallbackBaseUrl(state.fallbackBaseUrl);
}

function resolvePreferredBackend(browserFailed) {
  return resolveExportBackend({
    qualityMode: state.qualityMode,
    fallbackBaseUrl: currentFallbackBaseUrl(),
    browserFailed,
  });
}

function currentFallbackPayload(entryPath = null) {
  if (!state.zipBytes) {
    throw new Error("No ZIP archive is loaded.");
  }

  return buildFallbackFormData({
    zipBytes: state.zipBytes,
    fileName: state.loadedFileName,
    entryPath,
    options: currentOutputOptions(),
  });
}

async function fetchFallbackBinary(pathName, { entryPath = null } = {}) {
  const requestUrl = buildFallbackUrl(currentFallbackBaseUrl(), pathName);
  const response = await fetch(requestUrl, {
    method: "POST",
    body: currentFallbackPayload(entryPath),
  });

  if (!response.ok) {
    const errorText = await response.text();
    throw new Error(errorText || `Fallback service request failed with ${response.status}.`);
  }

  return response.blob();
}

function setDownloadCompleteStatus(mode, { warningEntryCount = 0 } = {}) {
  const notices = [];
  if (mode === "browser" && currentOutputOptions().header_template) {
    notices.push("Header and footer templates only apply in High Quality Fallback mode.");
  }

  if (warningEntryCount > 0) {
    notices.push(
      warningEntryCount === 1
        ? "The exported entry emitted browser render warnings."
        : `${warningEntryCount} exported entries emitted browser render warnings.`,
    );
  } else if (hasDiagnostics()) {
    notices.push("Diagnostics remain visible for the current archive.");
  }

  if (notices.length > 0) {
    const lead = mode === "browser" ? "Browser Fast export completed." : "Export completed.";
    setStatus("warning", "Downloaded", `${lead} ${notices.join(" ")}`);
    return;
  }

  setStatus("ready", "Downloaded", "Export completed successfully.");
}

async function downloadSelectedPdfInBrowser() {
  const outputOptions = currentOutputOptions();
  setStatus("ready", "Exporting", `Rendering ${state.selectedEntry} to a browser PDF.`);
  const preview = await ensureSelectedPreview(outputOptions);
  if (hasFailedRemoteAssets(preview.remoteAssets) && currentFallbackBaseUrl()) {
    throw new Error(
      `Remote images could not be materialized in the browser for ${preview.entryPath}.`,
    );
  }
  const { pdfBlob, runtimeDiagnostics } = await buildPdfBlobFromPreview(
    preview,
    derivePdfFileName(preview.entryPath),
    outputOptions,
  );
  triggerDownload(pdfBlob, derivePdfFileName(preview.entryPath));
  setDownloadCompleteStatus("browser", {
    warningEntryCount:
      runtimeDiagnostics.warnings.length > 0 || (preview.remoteWarnings?.length ?? 0) > 0 ? 1 : 0,
  });
}

async function downloadSelectedPdfFromServer(reasonLabel) {
  if (!state.selectedEntry) {
    throw new Error("No entry is selected.");
  }

  setStatus("ready", reasonLabel, `Sending ${state.selectedEntry} to the High Quality fallback service.`);
  const pdfBlob = await fetchFallbackBinary("/api/render/pdf", { entryPath: state.selectedEntry });
  triggerDownload(pdfBlob, derivePdfFileName(state.selectedEntry));
  setDownloadCompleteStatus("server");
}

async function downloadBatchZipInBrowser() {
  if (!state.projectIndex || !state.wasm || !state.zipBytes) {
    return;
  }

  const outputOptions = currentOutputOptions();
  const entryPaths = state.projectIndex.entry_candidates.map((candidate) => candidate.path);
  setStatus("ready", "Preparing", `Rendering ${entryPaths.length} entries for browser export.`);
  const previews = state.wasm.renderHtmlBatch(state.zipBytes, entryPaths, outputOptions);
  const files = [];
  let warningEntryCount = 0;

  for (let index = 0; index < previews.length; index += 1) {
    const preview = await materializePreview(previews[index], outputOptions);
    setStatus(
      "ready",
      "Exporting",
      `Rendering PDF ${index + 1} of ${previews.length}: ${preview.entryPath}.`,
    );
    if (hasFailedRemoteAssets(preview.remoteAssets) && currentFallbackBaseUrl()) {
      throw new Error(
        `Remote images could not be materialized in the browser for ${preview.entryPath}.`,
      );
    }
    const { pdfBlob, runtimeDiagnostics } = await buildPdfBlobFromPreview(
      preview,
      derivePdfFileName(preview.entryPath),
      outputOptions,
    );
    if (runtimeDiagnostics.warnings.length > 0 || preview.remoteWarnings.length > 0) {
      warningEntryCount += 1;
    }
    const pdfBytes = new Uint8Array(await pdfBlob.arrayBuffer());
    files.push({
      path: derivePdfPath(preview.entryPath),
      bytes: pdfBytes,
    });
  }

  setStatus("ready", "Packaging", `Bundling ${files.length} PDFs into a ZIP download.`);
  const archiveBytes = state.wasm.buildPdfArchive(files);
  const archiveBlob = new Blob([archiveBytes], { type: "application/zip" });
  triggerDownload(archiveBlob, deriveArchiveFileName());
  setDownloadCompleteStatus("browser", { warningEntryCount });
}

async function downloadBatchZipFromServer(reasonLabel) {
  setStatus("ready", reasonLabel, "Sending the archive to the High Quality fallback service for batch export.");
  const archiveBlob = await fetchFallbackBinary("/api/render/batch");
  triggerDownload(archiveBlob, deriveArchiveFileName());
  setDownloadCompleteStatus("server");
}

async function downloadDebugBundle() {
  if (!state.wasm || !state.zipBytes || !state.selectedEntry) {
    return;
  }

  try {
    setBusy(true);
    const outputOptions = currentOutputOptions();
    const preview = await ensureSelectedPreview(outputOptions);
    const manifest = buildBrowserAssetManifest(preview.entryPath, preview.remoteAssets);
    const report = buildBrowserRenderReport(preview.entryPath, outputOptions, preview.remoteAssets);
    setStatus("ready", "Packaging", `Building a debug bundle for ${state.selectedEntry}.`);
    const files = [
      {
        path: "debug.html",
        bytes: new TextEncoder().encode(preview.html),
      },
      {
        path: "asset-manifest.json",
        bytes: new TextEncoder().encode(JSON.stringify(manifest, null, 2)),
      },
      {
        path: "render-report.json",
        bytes: new TextEncoder().encode(JSON.stringify(report, null, 2)),
      },
      ...(await runtimeAssetFilesForHtml(preview.html)),
    ];
    const bundleBytes = state.wasm.buildPdfArchive(files);
    const bundleBlob = new Blob([bundleBytes], { type: "application/zip" });
    triggerDownload(bundleBlob, debugBundleFileName(state.loadedFileName, state.selectedEntry));
    setStatus("ready", "Downloaded", "Debug bundle downloaded with HTML, asset manifest, and render report.");
  } catch (error) {
    setStatus("error", "Bundle failed", `Debug bundle generation failed: ${String(error)}`);
  } finally {
    setBusy(false);
  }
}

async function downloadSelectedPdf() {
  try {
    setBusy(true);

    const preferredBackend = resolvePreferredBackend(false);
    if (preferredBackend === "server_unavailable") {
      throw new Error("High Quality mode requires a fallback server URL.");
    }

    if (preferredBackend === "server") {
      await downloadSelectedPdfFromServer("Fallback");
      return;
    }

    try {
      await downloadSelectedPdfInBrowser();
    } catch (error) {
      if (resolvePreferredBackend(true) !== "server") {
        throw error;
      }

      setStatus("warning", "Fallback", `Browser export failed. Retrying via the fallback service: ${String(error)}`);
      await downloadSelectedPdfFromServer("Fallback");
    }
  } catch (error) {
    setStatus("error", "Export failed", `Selected PDF export failed: ${String(error)}`);
  } finally {
    setBusy(false);
  }
}

async function downloadBatchZip() {
  if (!state.projectIndex || !state.wasm || !state.zipBytes) {
    return;
  }

  try {
    setBusy(true);

    const preferredBackend = resolvePreferredBackend(false);
    if (preferredBackend === "server_unavailable") {
      throw new Error("High Quality mode requires a fallback server URL.");
    }

    if (preferredBackend === "server") {
      await downloadBatchZipFromServer("Fallback");
      return;
    }

    try {
      await downloadBatchZipInBrowser();
    } catch (error) {
      if (resolvePreferredBackend(true) !== "server") {
        throw error;
      }

      setStatus("warning", "Fallback", `Browser batch export failed. Retrying via the fallback service: ${String(error)}`);
      await downloadBatchZipFromServer("Fallback");
    }
  } catch (error) {
    setStatus("error", "Export failed", `Batch PDF ZIP export failed: ${String(error)}`);
  } finally {
    setBusy(false);
  }
}

function connectWasmBindings() {
  if (!window.wasmBindings || state.wasm) {
    return;
  }

  state.wasm = window.wasmBindings;
  setStatus("ready", "Ready", "The WASM runtime is ready. Upload a ZIP archive to inspect it.");
  syncActionButtons();
}

function bindRenderOptionControl(control, { refreshPreview = false } = {}) {
  control.addEventListener("change", () => {
    updateTemplatePreviews();
    if (refreshPreview) {
      schedulePreviewRefresh();
    }
  });
}

function bindRenderOptionTextControl(control, { refreshPreview = false } = {}) {
  control.addEventListener("input", () => {
    updateTemplatePreviews();
    if (refreshPreview) {
      schedulePreviewRefresh();
    }
  });
}

window.addEventListener("TrunkApplicationStarted", connectWasmBindings);
window.addEventListener("load", connectWasmBindings, { once: true });
connectWasmBindings();
updateScaleNote();
updateTemplatePreviews();

elements.zipInput.addEventListener("change", async (event) => {
  const file = event.target.files?.[0];
  if (!file) {
    return;
  }

  await analyzeZip(file);
});

elements.qualityMode.addEventListener("change", (event) => {
  state.qualityMode = event.target.value;
  updateScaleNote();
});

elements.fallbackUrl.addEventListener("change", (event) => {
  state.fallbackBaseUrl = event.target.value;
});

bindRenderOptionControl(elements.theme, { refreshPreview: true });
bindRenderOptionControl(elements.pageSize);
bindRenderOptionControl(elements.marginTopMm);
bindRenderOptionControl(elements.marginRightMm);
bindRenderOptionControl(elements.marginBottomMm);
bindRenderOptionControl(elements.marginLeftMm);
bindRenderOptionControl(elements.landscape);
bindRenderOptionControl(elements.enableToc, { refreshPreview: true });
bindRenderOptionControl(elements.sanitizeHtml, { refreshPreview: true });
bindRenderOptionControl(elements.mermaidMode, { refreshPreview: true });
bindRenderOptionControl(elements.mathMode, { refreshPreview: true });
bindRenderOptionTextControl(elements.customCss, { refreshPreview: true });
bindRenderOptionTextControl(elements.title, { refreshPreview: true });
bindRenderOptionTextControl(elements.author, { refreshPreview: true });
bindRenderOptionTextControl(elements.subject, { refreshPreview: true });
bindRenderOptionTextControl(elements.headerTemplate);
bindRenderOptionTextControl(elements.footerTemplate);

elements.downloadSelected.addEventListener("click", async () => {
  await downloadSelectedPdf();
});

elements.downloadBatch.addEventListener("click", async () => {
  await downloadBatchZip();
});

elements.downloadDebug.addEventListener("click", async () => {
  await downloadDebugBundle();
});
