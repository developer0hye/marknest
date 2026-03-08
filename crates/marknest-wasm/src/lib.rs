use std::io::{Cursor, Write};
use std::path::{Component, Path};

use marknest_core::{
    AssetRef, DEFAULT_MATH_TIMEOUT_MS, DEFAULT_MERMAID_TIMEOUT_MS, MATHJAX_SCRIPT_URL,
    MATHJAX_VERSION, MERMAID_SCRIPT_URL, MERMAID_VERSION, MathMode, MermaidMode, PdfMetadata,
    ProjectIndex, ProjectSourceKind, RUNTIME_ASSET_MODE, RenderOptions, RenderedHtmlDocument,
    ThemePreset, analyze_zip, render_zip_entry_with_options,
};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
use zip::write::SimpleFileOptions;

const HTML2PDF_SCRIPT_URL: &str = "./runtime-assets/html2pdf/html2pdf.bundle.min.js";
const HTML2PDF_VERSION: &str = "0.10.1";
const MERMAID_RUNTIME_ASSET_BYTES: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../runtime-assets/mermaid/mermaid.min.js"
));
const MATHJAX_RUNTIME_ASSET_BYTES: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../runtime-assets/mathjax/es5/tex-svg.js"
));

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct RenderPreview {
    title: String,
    html: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct RenderPreviewEntry {
    entry_path: String,
    title: String,
    html: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct PdfArchiveFile {
    path: String,
    bytes: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
enum BrowserPageSize {
    #[default]
    A4,
    Letter,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
struct BrowserOutputOptions {
    theme: ThemePreset,
    custom_css: Option<String>,
    enable_toc: bool,
    sanitize_html: bool,
    title: Option<String>,
    author: Option<String>,
    subject: Option<String>,
    page_size: BrowserPageSize,
    margin_top_mm: i32,
    margin_right_mm: i32,
    margin_bottom_mm: i32,
    margin_left_mm: i32,
    landscape: bool,
    header_template: Option<String>,
    footer_template: Option<String>,
    mermaid_mode: MermaidMode,
    math_mode: MathMode,
    mermaid_timeout_ms: u32,
    math_timeout_ms: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct BrowserAssetManifest {
    entry_path: String,
    assets: Vec<AssetRef>,
    missing_assets: Vec<String>,
    path_errors: Vec<String>,
    warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct BrowserRenderReport {
    status: &'static str,
    source_kind: ProjectSourceKind,
    selected_entry: String,
    entry_candidates: Vec<String>,
    warnings: Vec<String>,
    errors: Vec<String>,
    options: BrowserOutputOptions,
    runtime_info: BrowserRuntimeInfo,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct BrowserRuntimeInfo {
    renderer: &'static str,
    marknest_version: &'static str,
    asset_mode: &'static str,
    pdf_engine: &'static str,
    mermaid_version: &'static str,
    mathjax_version: &'static str,
    html2pdf_version: &'static str,
    mermaid_script_url: &'static str,
    math_script_url: &'static str,
    html2pdf_script_url: &'static str,
}

impl Default for BrowserOutputOptions {
    fn default() -> Self {
        Self {
            theme: ThemePreset::Github,
            custom_css: None,
            enable_toc: false,
            sanitize_html: true,
            title: None,
            author: None,
            subject: None,
            page_size: BrowserPageSize::A4,
            margin_top_mm: 16,
            margin_right_mm: 16,
            margin_bottom_mm: 16,
            margin_left_mm: 16,
            landscape: false,
            header_template: None,
            footer_template: None,
            mermaid_mode: MermaidMode::Off,
            math_mode: MathMode::Off,
            mermaid_timeout_ms: DEFAULT_MERMAID_TIMEOUT_MS,
            math_timeout_ms: DEFAULT_MATH_TIMEOUT_MS,
        }
    }
}

impl BrowserOutputOptions {
    fn normalized(&self) -> Self {
        Self {
            theme: self.theme,
            custom_css: normalize_optional_block(&self.custom_css),
            enable_toc: self.enable_toc,
            sanitize_html: self.sanitize_html,
            title: normalize_optional_text(&self.title),
            author: normalize_optional_text(&self.author),
            subject: normalize_optional_text(&self.subject),
            page_size: self.page_size,
            margin_top_mm: self.margin_top_mm.max(0),
            margin_right_mm: self.margin_right_mm.max(0),
            margin_bottom_mm: self.margin_bottom_mm.max(0),
            margin_left_mm: self.margin_left_mm.max(0),
            landscape: self.landscape,
            header_template: normalize_optional_block(&self.header_template),
            footer_template: normalize_optional_block(&self.footer_template),
            mermaid_mode: self.mermaid_mode,
            math_mode: self.math_mode,
            mermaid_timeout_ms: self.mermaid_timeout_ms.max(1),
            math_timeout_ms: self.math_timeout_ms.max(1),
        }
    }

    fn render_options(&self) -> RenderOptions {
        let normalized = self.normalized();
        RenderOptions {
            theme: normalized.theme,
            metadata: PdfMetadata {
                title: normalized.title,
                author: normalized.author,
                subject: normalized.subject,
            },
            custom_css: normalized.custom_css,
            enable_toc: normalized.enable_toc,
            sanitize_html: normalized.sanitize_html,
            mermaid_mode: normalized.mermaid_mode,
            math_mode: normalized.math_mode,
            mermaid_timeout_ms: normalized.mermaid_timeout_ms,
            math_timeout_ms: normalized.math_timeout_ms,
        }
    }
}

fn normalize_optional_text(value: &Option<String>) -> Option<String> {
    value.as_ref().and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn normalize_optional_block(value: &Option<String>) -> Option<String> {
    value.as_ref().and_then(|value| {
        if value.trim().is_empty() {
            None
        } else {
            Some(value.clone())
        }
    })
}

fn browser_runtime_info() -> BrowserRuntimeInfo {
    BrowserRuntimeInfo {
        renderer: "browser-wasm",
        marknest_version: env!("CARGO_PKG_VERSION"),
        asset_mode: RUNTIME_ASSET_MODE,
        pdf_engine: "html2pdf.js",
        mermaid_version: MERMAID_VERSION,
        mathjax_version: MATHJAX_VERSION,
        html2pdf_version: HTML2PDF_VERSION,
        mermaid_script_url: MERMAID_SCRIPT_URL,
        math_script_url: MATHJAX_SCRIPT_URL,
        html2pdf_script_url: HTML2PDF_SCRIPT_URL,
    }
}

#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
}

#[wasm_bindgen(js_name = analyzeZip)]
pub fn analyze_zip_binding(zip_bytes: Vec<u8>) -> Result<JsValue, JsValue> {
    let project_index =
        analyze_zip_model(&zip_bytes).map_err(|message| JsValue::from_str(&message))?;
    serde_wasm_bindgen::to_value(&project_index)
        .map_err(|error| JsValue::from_str(&format!("Failed to encode project index: {error}")))
}

#[wasm_bindgen(js_name = renderHtml)]
pub fn render_html_binding(
    zip_bytes: Vec<u8>,
    entry_path: String,
    options: JsValue,
) -> Result<JsValue, JsValue> {
    let parsed_options =
        parse_browser_output_options(options).map_err(|message| JsValue::from_str(&message))?;
    let preview = render_preview_model(&zip_bytes, &entry_path, &parsed_options)
        .map_err(|message| JsValue::from_str(&message))?;
    serde_wasm_bindgen::to_value(&preview)
        .map_err(|error| JsValue::from_str(&format!("Failed to encode rendered HTML: {error}")))
}

#[wasm_bindgen(js_name = renderHtmlBatch)]
pub fn render_html_batch_binding(
    zip_bytes: Vec<u8>,
    entry_paths: JsValue,
    options: JsValue,
) -> Result<JsValue, JsValue> {
    let entry_paths: Vec<String> = serde_wasm_bindgen::from_value(entry_paths)
        .map_err(|error| JsValue::from_str(&format!("Invalid entry list: {error}")))?;
    let parsed_options =
        parse_browser_output_options(options).map_err(|message| JsValue::from_str(&message))?;
    let previews = render_preview_batch_model(&zip_bytes, &entry_paths, &parsed_options)
        .map_err(|message| JsValue::from_str(&message))?;
    serde_wasm_bindgen::to_value(&previews).map_err(|error| {
        JsValue::from_str(&format!("Failed to encode rendered batch HTML: {error}"))
    })
}

#[wasm_bindgen(js_name = buildPdfArchive)]
pub fn build_pdf_archive_binding(files: JsValue) -> Result<Vec<u8>, JsValue> {
    let files: Vec<PdfArchiveFile> = serde_wasm_bindgen::from_value(files)
        .map_err(|error| JsValue::from_str(&format!("Invalid PDF archive input: {error}")))?;
    build_pdf_archive_model(&files).map_err(|message| JsValue::from_str(&message))
}

#[wasm_bindgen(js_name = buildDebugBundle)]
pub fn build_debug_bundle_binding(
    zip_bytes: Vec<u8>,
    entry_path: String,
    options: JsValue,
) -> Result<Vec<u8>, JsValue> {
    let parsed_options =
        parse_browser_output_options(options).map_err(|message| JsValue::from_str(&message))?;
    build_debug_bundle_model(&zip_bytes, &entry_path, &parsed_options)
        .map_err(|message| JsValue::from_str(&message))
}

fn analyze_zip_model(zip_bytes: &[u8]) -> Result<ProjectIndex, String> {
    analyze_zip(zip_bytes).map_err(|error| error.to_string())
}

fn parse_browser_output_options(options: JsValue) -> Result<BrowserOutputOptions, String> {
    if options.is_undefined() || options.is_null() {
        return Ok(BrowserOutputOptions::default());
    }

    serde_wasm_bindgen::from_value(options)
        .map(|options: BrowserOutputOptions| options.normalized())
        .map_err(|error| format!("Invalid browser output options: {error}"))
}

fn render_preview_model(
    zip_bytes: &[u8],
    entry_path: &str,
    options: &BrowserOutputOptions,
) -> Result<RenderPreview, String> {
    let rendered_document: RenderedHtmlDocument =
        render_zip_entry_with_options(zip_bytes, entry_path, &options.render_options())
            .map_err(|error| error.to_string())?;

    Ok(RenderPreview {
        title: rendered_document.title,
        html: rendered_document.html,
    })
}

fn render_preview_batch_model(
    zip_bytes: &[u8],
    entry_paths: &[String],
    options: &BrowserOutputOptions,
) -> Result<Vec<RenderPreviewEntry>, String> {
    let mut previews: Vec<RenderPreviewEntry> = Vec::with_capacity(entry_paths.len());

    for entry_path in entry_paths {
        let rendered_document: RenderedHtmlDocument =
            render_zip_entry_with_options(zip_bytes, entry_path, &options.render_options())
                .map_err(|error| error.to_string())?;

        previews.push(RenderPreviewEntry {
            entry_path: entry_path.clone(),
            title: rendered_document.title,
            html: rendered_document.html,
        });
    }

    Ok(previews)
}

fn build_debug_bundle_model(
    zip_bytes: &[u8],
    entry_path: &str,
    options: &BrowserOutputOptions,
) -> Result<Vec<u8>, String> {
    let normalized_options = options.normalized();
    let project_index = analyze_zip_model(zip_bytes)?;
    let preview = render_preview_model(zip_bytes, entry_path, &normalized_options)?;
    let manifest = build_asset_manifest(&project_index, entry_path);
    let report = BrowserRenderReport {
        status: "success",
        source_kind: project_index.source_kind.clone(),
        selected_entry: entry_path.to_string(),
        entry_candidates: project_index
            .entry_candidates
            .iter()
            .map(|candidate| candidate.path.clone())
            .collect(),
        warnings: manifest.warnings.clone(),
        errors: manifest.path_errors.clone(),
        options: normalized_options,
        runtime_info: browser_runtime_info(),
    };

    let mut files = vec![
        PdfArchiveFile {
            path: "debug.html".to_string(),
            bytes: preview.html.clone().into_bytes(),
        },
        PdfArchiveFile {
            path: "asset-manifest.json".to_string(),
            bytes: serde_json::to_vec_pretty(&manifest)
                .map_err(|error| format!("Failed to encode asset manifest: {error}"))?,
        },
        PdfArchiveFile {
            path: "render-report.json".to_string(),
            bytes: serde_json::to_vec_pretty(&report)
                .map_err(|error| format!("Failed to encode render report: {error}"))?,
        },
    ];
    files.extend(runtime_asset_files_for_html(&preview.html));

    build_pdf_archive_model(&files)
}

fn runtime_asset_files_for_html(html: &str) -> Vec<PdfArchiveFile> {
    let mut files: Vec<PdfArchiveFile> = Vec::new();

    if html.contains(MERMAID_SCRIPT_URL) {
        files.push(PdfArchiveFile {
            path: "runtime-assets/mermaid/mermaid.min.js".to_string(),
            bytes: MERMAID_RUNTIME_ASSET_BYTES.to_vec(),
        });
    }
    if html.contains(MATHJAX_SCRIPT_URL) {
        files.push(PdfArchiveFile {
            path: "runtime-assets/mathjax/es5/tex-svg.js".to_string(),
            bytes: MATHJAX_RUNTIME_ASSET_BYTES.to_vec(),
        });
    }

    files
}

fn build_asset_manifest(project_index: &ProjectIndex, entry_path: &str) -> BrowserAssetManifest {
    let entry_selector = format!("{entry_path} -> ");
    BrowserAssetManifest {
        entry_path: entry_path.to_string(),
        assets: project_index
            .assets
            .iter()
            .filter(|asset| asset.entry_path == entry_path)
            .cloned()
            .collect(),
        missing_assets: project_index
            .diagnostic
            .missing_assets
            .iter()
            .filter(|message| message.contains(&entry_selector))
            .cloned()
            .collect(),
        path_errors: project_index
            .diagnostic
            .path_errors
            .iter()
            .filter(|message| message.contains(&entry_selector))
            .cloned()
            .collect(),
        warnings: project_index
            .diagnostic
            .warnings
            .iter()
            .filter(|message| message.contains(&entry_selector))
            .cloned()
            .collect(),
    }
}

fn build_pdf_archive_model(files: &[PdfArchiveFile]) -> Result<Vec<u8>, String> {
    let mut cursor = Cursor::new(Vec::new());
    {
        let mut writer = zip::ZipWriter::new(&mut cursor);
        for file in files {
            let normalized_path = normalize_archive_relative_path(&file.path)?;
            writer
                .start_file(normalized_path, SimpleFileOptions::default())
                .map_err(|error| format!("Failed to create ZIP entry: {error}"))?;
            writer
                .write_all(&file.bytes)
                .map_err(|error| format!("Failed to write ZIP entry contents: {error}"))?;
        }
        writer
            .finish()
            .map_err(|error| format!("Failed to finalize ZIP archive: {error}"))?;
    }

    Ok(cursor.into_inner())
}

fn normalize_archive_relative_path(path: &str) -> Result<String, String> {
    let trimmed_path = path.trim();
    if trimmed_path.is_empty() {
        return Err("Archive path cannot be empty.".to_string());
    }

    let candidate_path = Path::new(trimmed_path);
    let mut segments: Vec<String> = Vec::new();

    for component in candidate_path.components() {
        match component {
            Component::Normal(segment) => {
                segments.push(segment.to_string_lossy().replace('\\', "/"))
            }
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(format!("Archive path is invalid: {trimmed_path}"));
            }
        }
    }

    if segments.is_empty() {
        return Err(format!("Archive path is invalid: {trimmed_path}"));
    }

    Ok(segments.join("/"))
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};

    use super::{
        BrowserOutputOptions, PdfArchiveFile, analyze_zip_model, build_debug_bundle_model,
        build_pdf_archive_model, render_preview_batch_model, render_preview_model,
    };
    use marknest_core::{MathMode, MermaidMode, ThemePreset};
    use zip::write::SimpleFileOptions;

    fn build_zip(entries: &[(&str, &str)]) -> Vec<u8> {
        let mut cursor = std::io::Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut cursor);
            for (path, contents) in entries {
                writer
                    .start_file(path, SimpleFileOptions::default())
                    .expect("zip entry should be created");
                writer
                    .write_all(contents.as_bytes())
                    .expect("zip contents should be written");
            }
            writer.finish().expect("zip should finish");
        }

        cursor.into_inner()
    }

    #[test]
    fn analyzes_zip_bytes_for_the_browser_flow() {
        let zip_bytes = build_zip(&[
            ("docs/README.md", "# Guide\n"),
            ("docs/tutorial.md", "# Tutorial\n"),
        ]);

        let project_index = analyze_zip_model(&zip_bytes).expect("zip should analyze");

        assert_eq!(project_index.entry_candidates.len(), 2);
        assert_eq!(
            project_index.selected_entry.as_deref(),
            Some("docs/README.md")
        );
    }

    #[test]
    fn renders_zip_preview_html_for_the_browser_flow() {
        let zip_bytes = build_zip(&[
            (
                "docs/README.md",
                "# Guide\n\n![Architecture](../images/diagram.svg)\n",
            ),
            (
                "images/diagram.svg",
                "<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>",
            ),
        ]);

        let preview = render_preview_model(
            &zip_bytes,
            "docs/README.md",
            &BrowserOutputOptions::default(),
        )
        .expect("preview should render");

        assert_eq!(preview.title, "README");
        assert!(preview.html.contains("theme-github"));
        assert!(preview.html.contains("data:image/svg+xml;base64,"));
    }

    #[test]
    fn renders_zip_preview_html_with_browser_output_options() {
        let zip_bytes = build_zip(&[(
            "docs/README.md",
            "# Guide\n\n<div onclick=\"alert('x')\">Preview</div>\n\n<script>alert(\"preview\")</script>\n\n```mermaid\ngraph TD\n  A --> B\n```\n\n$$x + y$$\n",
        )]);

        let preview = render_preview_model(
            &zip_bytes,
            "docs/README.md",
            &BrowserOutputOptions {
                theme: ThemePreset::Docs,
                custom_css: Some("body { color: rgb(5, 4, 3); }".to_string()),
                enable_toc: true,
                author: Some("Docs Team".to_string()),
                subject: Some("Architecture".to_string()),
                sanitize_html: false,
                mermaid_mode: MermaidMode::Auto,
                math_mode: MathMode::Auto,
                mermaid_timeout_ms: 4200,
                math_timeout_ms: 2400,
                ..BrowserOutputOptions::default()
            },
        )
        .expect("preview should render with browser output options");

        assert!(preview.html.contains("theme-docs"));
        assert!(preview.html.contains("rgb(5, 4, 3)"));
        assert!(
            preview
                .html
                .contains("<meta name=\"author\" content=\"Docs Team\">")
        );
        assert!(
            preview
                .html
                .contains("<meta name=\"subject\" content=\"Architecture\">")
        );
        assert!(preview.html.contains("mermaid.min.js"));
        assert!(preview.html.contains("tex-svg.js"));
        assert!(preview.html.contains("marknest-toc"));
        assert!(preview.html.contains("<script>alert(\"preview\")</script>"));
        assert!(preview.html.contains("onclick=\"alert('x')\""));
        assert!(preview.html.contains("\"mermaidTimeoutMs\":4200"));
        assert!(preview.html.contains("\"mathTimeoutMs\":2400"));
    }

    #[test]
    fn renders_a_batch_of_zip_previews_for_browser_export() {
        let zip_bytes = build_zip(&[
            ("docs/README.md", "# Guide\n"),
            ("docs/tutorial.md", "# Tutorial\n"),
        ]);

        let previews = render_preview_batch_model(
            &zip_bytes,
            &["docs/README.md".to_string(), "docs/tutorial.md".to_string()],
            &BrowserOutputOptions::default(),
        )
        .expect("batch preview should render");

        assert_eq!(previews.len(), 2);
        assert_eq!(previews[0].entry_path, "docs/README.md");
        assert!(
            previews[1]
                .html
                .contains("<h1 id=\"tutorial\">Tutorial</h1>")
        );
    }

    #[test]
    fn builds_a_zip_archive_from_browser_generated_pdfs() {
        let archive_bytes = build_pdf_archive_model(&[
            PdfArchiveFile {
                path: "docs/README.pdf".to_string(),
                bytes: b"%PDF-1.4\nREADME\n".to_vec(),
            },
            PdfArchiveFile {
                path: "docs/tutorial.pdf".to_string(),
                bytes: b"%PDF-1.4\nTUTORIAL\n".to_vec(),
            },
        ])
        .expect("archive should build");

        let reader = std::io::Cursor::new(archive_bytes);
        let mut archive = zip::ZipArchive::new(reader).expect("zip archive should open");
        assert_eq!(archive.len(), 2);

        let mut readme_pdf = String::new();
        archive
            .by_name("docs/README.pdf")
            .expect("README pdf should exist")
            .read_to_string(&mut readme_pdf)
            .expect("README pdf should be readable");
        assert!(readme_pdf.contains("README"));
    }

    #[test]
    fn builds_a_debug_bundle_zip_for_the_selected_entry() {
        let zip_bytes = build_zip(&[
            ("docs/README.md", "# Guide\n\n![Missing](./missing.png)\n"),
            ("docs/notes.md", "# Notes\n"),
        ]);

        let archive_bytes = build_debug_bundle_model(
            &zip_bytes,
            "docs/README.md",
            &BrowserOutputOptions {
                theme: ThemePreset::Plain,
                custom_css: Some("body { color: rgb(1, 2, 3); }".to_string()),
                margin_top_mm: 20,
                margin_right_mm: 12,
                margin_bottom_mm: 24,
                margin_left_mm: 10,
                ..BrowserOutputOptions::default()
            },
        )
        .expect("debug bundle should be created");

        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(archive_bytes))
            .expect("debug bundle should be a zip archive");

        let mut debug_html = String::new();
        archive
            .by_name("debug.html")
            .expect("debug html should exist")
            .read_to_string(&mut debug_html)
            .expect("debug html should be readable");
        assert!(debug_html.contains("theme-plain"));
        assert!(debug_html.contains("rgb(1, 2, 3)"));

        let mut manifest_json = String::new();
        archive
            .by_name("asset-manifest.json")
            .expect("asset manifest should exist")
            .read_to_string(&mut manifest_json)
            .expect("asset manifest should be readable");
        let manifest: serde_json::Value =
            serde_json::from_str(&manifest_json).expect("asset manifest should be valid json");
        assert_eq!(manifest["entry_path"], "docs/README.md");
        assert_eq!(
            manifest["missing_assets"][0],
            "docs/README.md -> ./missing.png"
        );

        let mut report_json = String::new();
        archive
            .by_name("render-report.json")
            .expect("render report should exist")
            .read_to_string(&mut report_json)
            .expect("render report should be readable");
        let report: serde_json::Value =
            serde_json::from_str(&report_json).expect("render report should be valid json");
        assert_eq!(report["selected_entry"], "docs/README.md");
        assert_eq!(report["runtime_info"]["renderer"], "browser-wasm");
        assert_eq!(report["runtime_info"]["asset_mode"], "bundled_local");
        assert_eq!(report["runtime_info"]["mermaid_version"], "11.11.0");
        assert_eq!(report["runtime_info"]["mathjax_version"], "3.2.2");
        assert_eq!(report["runtime_info"]["html2pdf_version"], "0.10.1");
        assert_eq!(
            report["runtime_info"]["mermaid_script_url"],
            "./runtime-assets/mermaid/mermaid.min.js"
        );
        assert_eq!(
            report["runtime_info"]["math_script_url"],
            "./runtime-assets/mathjax/es5/tex-svg.js"
        );
        assert_eq!(
            report["runtime_info"]["html2pdf_script_url"],
            "./runtime-assets/html2pdf/html2pdf.bundle.min.js"
        );
        assert_eq!(report["options"]["theme"], "plain");
        assert_eq!(report["options"]["margin_top_mm"], 20);
        assert_eq!(report["options"]["margin_right_mm"], 12);
        assert_eq!(report["options"]["margin_bottom_mm"], 24);
        assert_eq!(report["options"]["margin_left_mm"], 10);
        assert_eq!(report["options"]["enable_toc"], false);
        assert_eq!(report["options"]["sanitize_html"], true);
        assert_eq!(report["options"]["mermaid_timeout_ms"], 5000);
        assert_eq!(report["options"]["math_timeout_ms"], 3000);
    }

    #[test]
    fn debug_bundle_includes_runtime_assets_when_mermaid_and_math_are_enabled() {
        let zip_bytes = build_zip(&[(
            "docs/README.md",
            "# Runtime\n\n```mermaid\ngraph TD\n  A-->B\n```\n\nInline math $x+y$.\n",
        )]);

        let archive_bytes = build_debug_bundle_model(
            &zip_bytes,
            "docs/README.md",
            &BrowserOutputOptions {
                mermaid_mode: MermaidMode::Auto,
                math_mode: MathMode::Auto,
                ..BrowserOutputOptions::default()
            },
        )
        .expect("debug bundle should be created");

        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(archive_bytes))
            .expect("debug bundle should be a zip archive");

        assert!(
            archive
                .by_name("runtime-assets/mermaid/mermaid.min.js")
                .is_ok()
        );
        assert!(
            archive
                .by_name("runtime-assets/mathjax/es5/tex-svg.js")
                .is_ok()
        );
    }
}
