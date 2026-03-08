export const DEFAULT_OUTPUT_OPTIONS = Object.freeze({
  theme: "github",
  custom_css: null,
  header_template: null,
  footer_template: null,
  title: null,
  author: null,
  subject: null,
  page_size: "a4",
  margin_top_mm: 16,
  margin_right_mm: 16,
  margin_bottom_mm: 16,
  margin_left_mm: 16,
  landscape: false,
  mermaid_mode: "off",
  math_mode: "off",
});

function normalizeOptionalText(value) {
  const trimmed = String(value ?? "").trim();
  return trimmed ? trimmed : null;
}

function normalizeOptionalBlock(value) {
  const text = String(value ?? "");
  return text.trim() ? text : null;
}

function normalizeMarginMm(value) {
  const parsed = Number(value);
  if (!Number.isFinite(parsed)) {
    return DEFAULT_OUTPUT_OPTIONS.margin_top_mm;
  }

  return parsed < 0 ? 0 : parsed;
}

export function buildOutputOptions(source = {}) {
  const uniformMarginMm = normalizeMarginMm(source.marginMm);
  return {
    theme: normalizeOptionalText(source.theme) ?? DEFAULT_OUTPUT_OPTIONS.theme,
    custom_css: normalizeOptionalBlock(source.customCss),
    header_template: normalizeOptionalBlock(source.headerTemplate),
    footer_template: normalizeOptionalBlock(source.footerTemplate),
    title: normalizeOptionalText(source.title),
    author: normalizeOptionalText(source.author),
    subject: normalizeOptionalText(source.subject),
    page_size: normalizeOptionalText(source.pageSize) ?? DEFAULT_OUTPUT_OPTIONS.page_size,
    margin_top_mm: normalizeMarginMm(source.marginTopMm ?? uniformMarginMm),
    margin_right_mm: normalizeMarginMm(source.marginRightMm ?? uniformMarginMm),
    margin_bottom_mm: normalizeMarginMm(source.marginBottomMm ?? uniformMarginMm),
    margin_left_mm: normalizeMarginMm(source.marginLeftMm ?? uniformMarginMm),
    landscape: Boolean(source.landscape),
    mermaid_mode: normalizeOptionalText(source.mermaidMode) ?? DEFAULT_OUTPUT_OPTIONS.mermaid_mode,
    math_mode: normalizeOptionalText(source.mathMode) ?? DEFAULT_OUTPUT_OPTIONS.math_mode,
  };
}

export function buildFallbackFormData({ zipBytes, fileName, entryPath = null, options }) {
  const payload = new FormData();
  if (entryPath) {
    payload.append("entry", entryPath);
  }

  payload.append("options", JSON.stringify(options ?? DEFAULT_OUTPUT_OPTIONS));
  payload.append(
    "archive",
    new Blob([zipBytes], { type: "application/zip" }),
    fileName || "workspace.zip",
  );
  return payload;
}

export function debugBundleFileName(uploadedFileName, entryPath) {
  const archiveBase = (uploadedFileName || "marknest")
    .replace(/\.zip$/i, "")
    .replace(/[^a-z0-9_-]+/gi, "-")
    .replace(/-+/g, "-")
    .replace(/^-|-$/g, "");
  const entryBase = String(entryPath || "document.md")
    .replace(/\.(md|markdown)$/i, "")
    .replace(/[^a-z0-9_-]+/gi, "-")
    .replace(/-+/g, "-")
    .replace(/^-|-$/g, "");

  return `${archiveBase || "marknest"}-${entryBase || "document"}-debug.zip`;
}
