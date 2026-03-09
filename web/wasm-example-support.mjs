export const SAMPLE_ARCHIVE_ROOT = "marknest-demo-main";
export const SAMPLE_ARCHIVE_FILE_NAME = "marknest-demo-main.zip";
export const SAMPLE_ENTRY_PATH = `${SAMPLE_ARCHIVE_ROOT}/README.md`;

const SAMPLE_MARKDOWN = `# MarkNest WASM Example

This sample archive is intentionally wrapped in a shared top-level directory so you can verify \`strip_zip_prefix\`.

## Mermaid

\`\`\`mermaid
graph TD
  Upload[Wrapped ZIP] --> Analyze[analyzeZipWithOptions]
  Analyze --> Render[renderHtml]
  Render --> Preview[Preview iframe]
\`\`\`

## Math

$$
e^{i\\pi} + 1 = 0
$$

- Toggle **Strip shared ZIP prefix** to switch the selected entry from \`${SAMPLE_ENTRY_PATH}\` to \`README.md\`.
- Change the runtime asset base URL to confirm the preview HTML rewrites Mermaid and MathJax asset paths.
`;

const SAMPLE_GUIDE_MARKDOWN = `# Nested Guide

This second entry proves the sample archive behaves like a multi-entry repository snapshot.
`;

const SAMPLE_CSS = `body { font-family: "Iowan Old Style", Georgia, serif; }
pre code { white-space: pre-wrap; }`;

export function buildWasmExampleArchiveEntries() {
  return [
    { path: SAMPLE_ENTRY_PATH, text: SAMPLE_MARKDOWN },
    { path: `${SAMPLE_ARCHIVE_ROOT}/docs/guide.md`, text: SAMPLE_GUIDE_MARKDOWN },
    { path: `${SAMPLE_ARCHIVE_ROOT}/styles/print.css`, text: SAMPLE_CSS },
  ];
}

export function encodeArchiveEntries(entries) {
  const encoder = new TextEncoder();
  return entries.map((entry) => ({
    path: entry.path,
    bytes: encoder.encode(entry.text),
  }));
}

export function buildWasmExampleAnalyzeOptions({ stripZipPrefix = false } = {}) {
  return {
    strip_zip_prefix: Boolean(stripZipPrefix),
  };
}

export function buildWasmExampleRenderOptions({
  stripZipPrefix = false,
  runtimeAssetsBaseUrl = "",
  mermaidMode = "auto",
  mathMode = "auto",
  theme = "github",
} = {}) {
  const normalizedBaseUrl =
    typeof runtimeAssetsBaseUrl === "string" && runtimeAssetsBaseUrl.trim().length > 0
      ? runtimeAssetsBaseUrl.trim()
      : null;

  return {
    theme,
    mermaid_mode: mermaidMode,
    math_mode: mathMode,
    runtime_assets_base_url: normalizedBaseUrl,
    strip_zip_prefix: Boolean(stripZipPrefix),
  };
}

export function pickExampleEntryPath(projectIndex) {
  if (!projectIndex) {
    return null;
  }

  return projectIndex.selected_entry ?? projectIndex.entry_candidates?.[0]?.path ?? null;
}

export function extractRuntimeScriptUrls(html) {
  const mermaidMatch = /"mermaidScript":"([^"]+)"/.exec(String(html));
  const mathMatch = /"mathScript":"([^"]+)"/.exec(String(html));

  return {
    mermaidScriptUrl: mermaidMatch?.[1] ?? null,
    mathScriptUrl: mathMatch?.[1] ?? null,
  };
}
