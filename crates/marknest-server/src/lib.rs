use std::io::{Cursor, Write};
use std::sync::Arc;

use axum::{
    Router,
    extract::{DefaultBodyLimit, Multipart, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use marknest::{
    HtmlToPdfErrorKind, HtmlToPdfRequest, PdfMarginsMm, PdfPageSize,
    materialize_remote_assets_for_html, prepare_print_template_html, render_html_to_pdf_bytes,
};
use marknest_core::{
    DEFAULT_MATH_TIMEOUT_MS, DEFAULT_MERMAID_TIMEOUT_MS, MathMode, MermaidMode, PdfMetadata,
    RenderOptions, ThemePreset, analyze_zip, render_zip_entry_with_options,
};
use serde::Deserialize;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use zip::write::SimpleFileOptions;

#[derive(Debug, Clone, PartialEq)]
pub struct SelectedPdfRequest {
    pub zip_bytes: Vec<u8>,
    pub entry_path: String,
    pub options: FallbackRenderOptions,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BatchPdfRequest {
    pub zip_bytes: Vec<u8>,
    pub options: FallbackRenderOptions,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BinaryExport {
    pub bytes: Vec<u8>,
    pub file_name: String,
    pub content_type: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFailureKind {
    Validation,
    System,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportFailure {
    pub kind: ExportFailureKind,
    pub message: String,
}

impl ExportFailure {
    fn validation(message: String) -> Self {
        Self {
            kind: ExportFailureKind::Validation,
            message,
        }
    }

    fn system(message: String) -> Self {
        Self {
            kind: ExportFailureKind::System,
            message,
        }
    }
}

pub trait PdfFallbackExporter: Send + Sync + 'static {
    fn export_selected_pdf(
        &self,
        request: &SelectedPdfRequest,
    ) -> Result<BinaryExport, ExportFailure>;

    fn export_batch_archive(
        &self,
        request: &BatchPdfRequest,
    ) -> Result<BinaryExport, ExportFailure>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ChromiumFallbackExporter;

#[derive(Clone)]
struct AppState {
    exporter: Arc<dyn PdfFallbackExporter>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct FallbackRenderOptions {
    pub theme: ThemePreset,
    pub custom_css: Option<String>,
    pub enable_toc: bool,
    pub sanitize_html: bool,
    pub title: Option<String>,
    pub author: Option<String>,
    pub subject: Option<String>,
    pub page_size: PdfPageSize,
    pub margin_mm: Option<f64>,
    pub margin_top_mm: Option<f64>,
    pub margin_right_mm: Option<f64>,
    pub margin_bottom_mm: Option<f64>,
    pub margin_left_mm: Option<f64>,
    #[serde(skip)]
    pub margins_mm: PdfMarginsMm,
    pub landscape: bool,
    pub header_template: Option<String>,
    pub footer_template: Option<String>,
    pub mermaid_mode: MermaidMode,
    pub math_mode: MathMode,
    pub mermaid_timeout_ms: u32,
    pub math_timeout_ms: u32,
}

impl Default for FallbackRenderOptions {
    fn default() -> Self {
        Self {
            theme: ThemePreset::Github,
            custom_css: None,
            enable_toc: false,
            sanitize_html: true,
            title: None,
            author: None,
            subject: None,
            page_size: PdfPageSize::A4,
            margin_mm: None,
            margin_top_mm: None,
            margin_right_mm: None,
            margin_bottom_mm: None,
            margin_left_mm: None,
            margins_mm: PdfMarginsMm::default(),
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

impl FallbackRenderOptions {
    fn normalized(self) -> Self {
        let margin_mm = self.margin_mm.map(|value| value.max(0.0));
        let margin_top_mm = self.margin_top_mm.map(|value| value.max(0.0));
        let margin_right_mm = self.margin_right_mm.map(|value| value.max(0.0));
        let margin_bottom_mm = self.margin_bottom_mm.map(|value| value.max(0.0));
        let margin_left_mm = self.margin_left_mm.map(|value| value.max(0.0));

        Self {
            theme: self.theme,
            custom_css: normalize_optional_block(self.custom_css),
            enable_toc: self.enable_toc,
            sanitize_html: self.sanitize_html,
            title: normalize_optional_text(self.title),
            author: normalize_optional_text(self.author),
            subject: normalize_optional_text(self.subject),
            page_size: self.page_size,
            margin_mm,
            margin_top_mm,
            margin_right_mm,
            margin_bottom_mm,
            margin_left_mm,
            margins_mm: resolve_fallback_margins(
                margin_mm,
                margin_top_mm,
                margin_right_mm,
                margin_bottom_mm,
                margin_left_mm,
            ),
            landscape: self.landscape,
            header_template: normalize_optional_block(self.header_template),
            footer_template: normalize_optional_block(self.footer_template),
            mermaid_mode: self.mermaid_mode,
            math_mode: self.math_mode,
            mermaid_timeout_ms: self.mermaid_timeout_ms.max(1),
            math_timeout_ms: self.math_timeout_ms.max(1),
        }
    }

    fn render_options(&self) -> RenderOptions {
        RenderOptions {
            theme: self.theme,
            metadata: self.metadata(),
            custom_css: self.custom_css.clone(),
            enable_toc: self.enable_toc,
            sanitize_html: self.sanitize_html,
            mermaid_mode: self.mermaid_mode,
            math_mode: self.math_mode,
            mermaid_timeout_ms: self.mermaid_timeout_ms,
            math_timeout_ms: self.math_timeout_ms,
            runtime_assets_base_url: None,
        }
    }

    fn metadata(&self) -> PdfMetadata {
        PdfMetadata {
            title: self.title.clone(),
            author: self.author.clone(),
            subject: self.subject.clone(),
        }
    }
}

fn resolve_fallback_margins(
    margin_mm: Option<f64>,
    margin_top_mm: Option<f64>,
    margin_right_mm: Option<f64>,
    margin_bottom_mm: Option<f64>,
    margin_left_mm: Option<f64>,
) -> PdfMarginsMm {
    let base_margins = PdfMarginsMm::uniform(margin_mm.unwrap_or(16.0));
    PdfMarginsMm {
        top: margin_top_mm.unwrap_or(base_margins.top),
        right: margin_right_mm.unwrap_or(base_margins.right),
        bottom: margin_bottom_mm.unwrap_or(base_margins.bottom),
        left: margin_left_mm.unwrap_or(base_margins.left),
    }
}

pub fn app(exporter: Arc<dyn PdfFallbackExporter>) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/render/pdf", post(render_selected_pdf))
        .route("/api/render/batch", post(render_batch_archive))
        .layer(DefaultBodyLimit::max(64 * 1024 * 1024))
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(AppState { exporter })
}

impl PdfFallbackExporter for ChromiumFallbackExporter {
    fn export_selected_pdf(
        &self,
        request: &SelectedPdfRequest,
    ) -> Result<BinaryExport, ExportFailure> {
        let project_index = analyze_zip(&request.zip_bytes)
            .map_err(|error| ExportFailure::validation(error.to_string()))?;
        let rendered_document = render_zip_entry_with_options(
            &request.zip_bytes,
            &request.entry_path,
            &request.options.render_options(),
        )
        .map_err(|error| ExportFailure::validation(error.to_string()))?;
        let selected_assets: Vec<_> = project_index
            .assets
            .iter()
            .filter(|asset| asset.entry_path == request.entry_path)
            .cloned()
            .collect();
        let remote_html =
            materialize_remote_assets_for_html(&rendered_document.html, &selected_assets).map_err(
                |error| {
                    ExportFailure::system(format!("Failed to materialize remote assets: {error}"))
                },
            )?;

        let pdf = render_html_to_pdf_bytes(&HtmlToPdfRequest {
            title: rendered_document.title.clone(),
            html: remote_html.html,
            page_size: request.options.page_size,
            margins_mm: request.options.margins_mm,
            landscape: request.options.landscape,
            metadata: request.options.metadata(),
            header_template: prepare_print_template_html(
                request.options.header_template.as_deref(),
                &rendered_document.title,
                &request.entry_path,
            )
            .map_err(ExportFailure::validation)?,
            footer_template: prepare_print_template_html(
                request.options.footer_template.as_deref(),
                &rendered_document.title,
                &request.entry_path,
            )
            .map_err(ExportFailure::validation)?,
        })
        .map_err(|error| match error.kind {
            HtmlToPdfErrorKind::Validation => ExportFailure::validation(error.message),
            HtmlToPdfErrorKind::System => ExportFailure::system(error.message),
        })?;

        Ok(BinaryExport {
            bytes: pdf.bytes,
            file_name: pdf_file_name(&request.entry_path),
            content_type: "application/pdf",
        })
    }

    fn export_batch_archive(
        &self,
        request: &BatchPdfRequest,
    ) -> Result<BinaryExport, ExportFailure> {
        let project_index = analyze_zip(&request.zip_bytes)
            .map_err(|error| ExportFailure::validation(error.to_string()))?;

        if project_index.entry_candidates.is_empty() {
            return Err(ExportFailure::validation(
                "The archive does not contain any Markdown entry candidates.".to_string(),
            ));
        }

        let mut cursor = Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut cursor);
            for entry in &project_index.entry_candidates {
                let rendered_pdf = self.export_selected_pdf(&SelectedPdfRequest {
                    zip_bytes: request.zip_bytes.clone(),
                    entry_path: entry.path.clone(),
                    options: request.options.clone(),
                })?;
                writer
                    .start_file(derive_pdf_path(&entry.path), SimpleFileOptions::default())
                    .map_err(|error| {
                        ExportFailure::system(format!("Failed to create batch ZIP entry: {error}"))
                    })?;
                writer.write_all(&rendered_pdf.bytes).map_err(|error| {
                    ExportFailure::system(format!("Failed to write batch ZIP entry: {error}"))
                })?;
            }
            writer.finish().map_err(|error| {
                ExportFailure::system(format!("Failed to finalize the batch ZIP archive: {error}"))
            })?;
        }

        Ok(BinaryExport {
            bytes: cursor.into_inner(),
            file_name: "marknest-pdfs.zip".to_string(),
            content_type: "application/zip",
        })
    }
}

async fn health() -> impl IntoResponse {
    (StatusCode::OK, "ok")
}

async fn render_selected_pdf(State(state): State<AppState>, multipart: Multipart) -> Response {
    let exporter = state.exporter.clone();
    let request = match selected_request_from_multipart(multipart).await {
        Ok(request) => request,
        Err(error) => return error_response(error),
    };

    match tokio::task::spawn_blocking(move || exporter.export_selected_pdf(&request)).await {
        Ok(Ok(export)) => binary_response(export),
        Ok(Err(error)) => error_response(error),
        Err(error) => error_response(ExportFailure::system(format!(
            "Fallback renderer task failed: {error}"
        ))),
    }
}

async fn render_batch_archive(State(state): State<AppState>, multipart: Multipart) -> Response {
    let exporter = state.exporter.clone();
    let request = match batch_request_from_multipart(multipart).await {
        Ok(request) => request,
        Err(error) => return error_response(error),
    };

    match tokio::task::spawn_blocking(move || exporter.export_batch_archive(&request)).await {
        Ok(Ok(export)) => binary_response(export),
        Ok(Err(error)) => error_response(error),
        Err(error) => error_response(ExportFailure::system(format!(
            "Fallback renderer task failed: {error}"
        ))),
    }
}

async fn selected_request_from_multipart(
    mut multipart: Multipart,
) -> Result<SelectedPdfRequest, ExportFailure> {
    let mut entry_path: Option<String> = None;
    let mut zip_bytes: Option<Vec<u8>> = None;
    let mut options = FallbackRenderOptions::default();

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|error| ExportFailure::validation(format!("Invalid multipart payload: {error}")))?
    {
        match field.name() {
            Some("entry") => {
                let value = field.text().await.map_err(|error| {
                    ExportFailure::validation(format!("Failed to read selected entry: {error}"))
                })?;
                entry_path = Some(value);
            }
            Some("options") => {
                let value = field.text().await.map_err(|error| {
                    ExportFailure::validation(format!("Failed to read export options: {error}"))
                })?;
                if !value.trim().is_empty() {
                    options = serde_json::from_str::<FallbackRenderOptions>(&value)
                        .map_err(|error| {
                            ExportFailure::validation(format!(
                                "Failed to parse export options JSON: {error}"
                            ))
                        })?
                        .normalized();
                }
            }
            Some("archive") => {
                zip_bytes = Some(
                    field
                        .bytes()
                        .await
                        .map_err(|error| {
                            ExportFailure::validation(format!(
                                "Failed to read uploaded ZIP archive: {error}"
                            ))
                        })?
                        .to_vec(),
                );
            }
            _ => {}
        }
    }

    let entry_path = entry_path
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            ExportFailure::validation("The selected entry path is required.".to_string())
        })?;
    let zip_bytes = zip_bytes.ok_or_else(|| {
        ExportFailure::validation("A ZIP archive upload is required.".to_string())
    })?;

    Ok(SelectedPdfRequest {
        zip_bytes,
        entry_path,
        options,
    })
}

async fn batch_request_from_multipart(
    mut multipart: Multipart,
) -> Result<BatchPdfRequest, ExportFailure> {
    let mut zip_bytes: Option<Vec<u8>> = None;
    let mut options = FallbackRenderOptions::default();

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|error| ExportFailure::validation(format!("Invalid multipart payload: {error}")))?
    {
        match field.name() {
            Some("options") => {
                let value = field.text().await.map_err(|error| {
                    ExportFailure::validation(format!("Failed to read export options: {error}"))
                })?;
                if !value.trim().is_empty() {
                    options = serde_json::from_str::<FallbackRenderOptions>(&value)
                        .map_err(|error| {
                            ExportFailure::validation(format!(
                                "Failed to parse export options JSON: {error}"
                            ))
                        })?
                        .normalized();
                }
            }
            Some("archive") => {
                zip_bytes = Some(
                    field
                        .bytes()
                        .await
                        .map_err(|error| {
                            ExportFailure::validation(format!(
                                "Failed to read uploaded ZIP archive: {error}"
                            ))
                        })?
                        .to_vec(),
                );
            }
            _ => {}
        }
    }

    let zip_bytes = zip_bytes.ok_or_else(|| {
        ExportFailure::validation("A ZIP archive upload is required.".to_string())
    })?;

    Ok(BatchPdfRequest { zip_bytes, options })
}

fn normalize_optional_text(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn normalize_optional_block(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        if value.trim().is_empty() {
            None
        } else {
            Some(value)
        }
    })
}

fn binary_response(export: BinaryExport) -> Response {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static(export.content_type),
    );
    if let Ok(disposition) = HeaderValue::from_str(&format!(
        "attachment; filename=\"{}\"",
        sanitize_header_filename(&export.file_name)
    )) {
        headers.insert(header::CONTENT_DISPOSITION, disposition);
    }

    (StatusCode::OK, headers, export.bytes).into_response()
}

fn error_response(error: ExportFailure) -> Response {
    let status = match error.kind {
        ExportFailureKind::Validation => StatusCode::BAD_REQUEST,
        ExportFailureKind::System => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (status, error.message).into_response()
}

fn derive_pdf_path(entry_path: &str) -> String {
    if entry_path.ends_with(".md") {
        return format!("{}.pdf", entry_path.trim_end_matches(".md"));
    }

    if entry_path.ends_with(".markdown") {
        return format!("{}.pdf", entry_path.trim_end_matches(".markdown"));
    }

    format!("{entry_path}.pdf")
}

fn pdf_file_name(entry_path: &str) -> String {
    derive_pdf_path(entry_path)
        .rsplit('/')
        .next()
        .unwrap_or("document.pdf")
        .to_string()
}

fn sanitize_header_filename(file_name: &str) -> String {
    file_name
        .chars()
        .map(|character| {
            if matches!(character, '"' | '\r' | '\n') {
                '_'
            } else {
                character
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use axum::{
        body::{Body, to_bytes},
        http::{Method, Request, StatusCode, header},
    };
    use marknest::PdfPageSize;
    use marknest_core::ThemePreset;
    use tower::ServiceExt;

    use super::{
        BatchPdfRequest, BinaryExport, ExportFailure, ExportFailureKind, PdfFallbackExporter,
        SelectedPdfRequest, app,
    };

    fn build_multipart_body(boundary: &str, fields: &[(&str, &str)], archive: &[u8]) -> Vec<u8> {
        let mut body = Vec::new();
        for (name, value) in fields {
            body.extend_from_slice(
                format!(
                    "--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n"
                )
                .as_bytes(),
            );
        }
        body.extend_from_slice(
            format!(
                "--{boundary}\r\nContent-Disposition: form-data; name=\"archive\"; filename=\"docs.zip\"\r\nContent-Type: application/zip\r\n\r\n"
            )
            .as_bytes(),
        );
        body.extend_from_slice(archive);
        body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
        body
    }

    #[tokio::test]
    async fn selected_pdf_route_returns_pdf_bytes_with_cors_headers() {
        let exporter = Arc::new(MockExporter::default());
        let router = app(exporter.clone());
        let boundary = "marknest-selected";
        let body = build_multipart_body(boundary, &[("entry", "docs/README.md")], &[1_u8, 2, 3]);

        let response = router
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/render/pdf")
                    .header(
                        header::CONTENT_TYPE,
                        format!("multipart/form-data; boundary={boundary}"),
                    )
                    .header(header::ORIGIN, "http://127.0.0.1:8080")
                    .body(Body::from(body))
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get(header::CONTENT_TYPE)
                .expect("content type should be set"),
            "application/pdf"
        );
        assert_eq!(
            response
                .headers()
                .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
                .expect("cors header should be set"),
            "*"
        );

        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body should be readable");
        assert_eq!(body.as_ref(), b"%PDF-1.4\nphase7\n");

        let requests = exporter
            .selected_requests
            .lock()
            .expect("requests mutex should lock");
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].entry_path, "docs/README.md");
        assert_eq!(requests[0].zip_bytes, vec![1_u8, 2, 3]);
    }

    #[tokio::test]
    async fn batch_route_returns_zip_bytes() {
        let exporter = Arc::new(MockExporter::default());
        let router = app(exporter.clone());
        let boundary = "marknest-batch";
        let body = build_multipart_body(boundary, &[], &[9_u8, 8, 7]);

        let response = router
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/render/batch")
                    .header(
                        header::CONTENT_TYPE,
                        format!("multipart/form-data; boundary={boundary}"),
                    )
                    .body(Body::from(body))
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get(header::CONTENT_TYPE)
                .expect("content type should be set"),
            "application/zip"
        );

        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body should be readable");
        assert_eq!(body.as_ref(), b"PK\x03\x04phase7");

        let requests = exporter
            .batch_requests
            .lock()
            .expect("requests mutex should lock");
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].zip_bytes, vec![9_u8, 8, 7]);
    }

    #[tokio::test]
    async fn selected_pdf_route_accepts_multipart_render_options() {
        let exporter = Arc::new(MockExporter::default());
        let router = app(exporter.clone());
        let boundary = "marknest-boundary";
        let options_json = r#"{"theme":"docs","custom_css":"body { color: rgb(5, 4, 3); }","page_size":"letter","margin_top_mm":18.0,"margin_right_mm":12.0,"margin_bottom_mm":24.0,"margin_left_mm":10.0,"landscape":true,"enable_toc":true,"sanitize_html":false,"title":"Guide Pack","author":"Docs Team","subject":"Architecture","header_template":"<div>Header {{title}}</div>","footer_template":"<div>{{pageNumber}}</div>","mermaid_timeout_ms":4200,"math_timeout_ms":2400}"#;
        let body = build_multipart_body(
            boundary,
            &[("entry", "docs/README.md"), ("options", options_json)],
            b"zip-data",
        );

        let response = router
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/render/pdf")
                    .header(
                        header::CONTENT_TYPE,
                        format!("multipart/form-data; boundary={boundary}"),
                    )
                    .body(Body::from(body))
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");

        assert_eq!(response.status(), StatusCode::OK);

        let requests = exporter
            .selected_requests
            .lock()
            .expect("requests mutex should lock");
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].entry_path, "docs/README.md");
        assert_eq!(requests[0].zip_bytes, b"zip-data".to_vec());
        assert_eq!(requests[0].options.theme, ThemePreset::Docs);
        assert_eq!(requests[0].options.page_size, PdfPageSize::Letter);
        assert_eq!(requests[0].options.margins_mm.top, 18.0);
        assert_eq!(requests[0].options.margins_mm.right, 12.0);
        assert_eq!(requests[0].options.margins_mm.bottom, 24.0);
        assert_eq!(requests[0].options.margins_mm.left, 10.0);
        assert_eq!(requests[0].options.mermaid_timeout_ms, 4200);
        assert_eq!(requests[0].options.math_timeout_ms, 2400);
        assert!(requests[0].options.enable_toc);
        assert!(!requests[0].options.sanitize_html);
        assert!(requests[0].options.landscape);
        assert_eq!(requests[0].options.title.as_deref(), Some("Guide Pack"));
        assert_eq!(requests[0].options.author.as_deref(), Some("Docs Team"));
        assert_eq!(
            requests[0].options.header_template.as_deref(),
            Some("<div>Header {{title}}</div>")
        );
    }

    #[derive(Default)]
    struct MockExporter {
        selected_requests: Mutex<Vec<SelectedPdfRequest>>,
        batch_requests: Mutex<Vec<BatchPdfRequest>>,
    }

    impl PdfFallbackExporter for MockExporter {
        fn export_selected_pdf(
            &self,
            request: &SelectedPdfRequest,
        ) -> Result<BinaryExport, ExportFailure> {
            self.selected_requests
                .lock()
                .expect("requests mutex should lock")
                .push(request.clone());

            Ok(BinaryExport {
                bytes: b"%PDF-1.4\nphase7\n".to_vec(),
                file_name: "README.pdf".to_string(),
                content_type: "application/pdf",
            })
        }

        fn export_batch_archive(
            &self,
            request: &BatchPdfRequest,
        ) -> Result<BinaryExport, ExportFailure> {
            self.batch_requests
                .lock()
                .expect("requests mutex should lock")
                .push(request.clone());

            Ok(BinaryExport {
                bytes: b"PK\x03\x04phase7".to_vec(),
                file_name: "marknest-pdfs.zip".to_string(),
                content_type: "application/zip",
            })
        }
    }

    #[test]
    fn exporter_failures_map_to_http_statuses() {
        assert_eq!(
            super::error_response(ExportFailure {
                kind: ExportFailureKind::Validation,
                message: "bad zip".to_string(),
            })
            .status(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            super::error_response(ExportFailure {
                kind: ExportFailureKind::System,
                message: "spawn failed".to_string(),
            })
            .status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }
}
