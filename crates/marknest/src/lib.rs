use std::collections::BTreeMap;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use marknest_core::{
    AnalyzeError, AssetRef, DEFAULT_MATH_TIMEOUT_MS, DEFAULT_MERMAID_TIMEOUT_MS, EntryCandidate,
    EntrySelectionReason, MATHJAX_SCRIPT_URL, MATHJAX_VERSION, MERMAID_SCRIPT_URL, MERMAID_VERSION,
    MathMode, MermaidMode, PdfMetadata, ProjectIndex, ProjectSourceKind, RUNTIME_ASSET_MODE,
    RenderHtmlError, RenderOptions, ThemePreset, analyze_workspace, analyze_zip,
    analyze_zip_strip_prefix, render_workspace_entry_with_options, rewrite_html_img_sources,
};
use serde::{Deserialize, Serialize};
use tempfile::TempDir;

const EXIT_SUCCESS: i32 = 0;
const EXIT_WARNING: i32 = 1;
const EXIT_VALIDATION_FAILURE: i32 = 2;
const EXIT_SYSTEM_FAILURE: i32 = 3;
const PLAYWRIGHT_PRINT_SCRIPT: &str = include_str!("playwright_print.js");
const PLAYWRIGHT_VERSION: &str = "1.58.2";
const MERMAID_RUNTIME_ASSET_BYTES: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../runtime-assets/mermaid/mermaid.min.js"
));
const MATHJAX_RUNTIME_ASSET_BYTES: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../runtime-assets/mathjax/es5/tex-svg.js"
));
const REMOTE_ASSET_TIMEOUT_SECONDS: u64 = 15;
const REMOTE_ASSET_MAX_REDIRECTS: u32 = 5;
const REMOTE_ASSET_MAX_BYTES: usize = 16 * 1024 * 1024;
const REMOTE_ASSET_MAX_TOTAL_BYTES: usize = 64 * 1024 * 1024;
const GITHUB_ARCHIVE_MAX_BYTES: usize = 256 * 1024 * 1024;
const GITHUB_API_TIMEOUT_SECONDS: u64 = 30;
const GITHUB_API_MAX_REDIRECTS: u32 = 5;

pub fn run<I, T>(args: I) -> i32
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let renderer = NodeBrowserPdfRenderer;
    run_with_pdf_renderer(args, &renderer)
}

fn run_with_pdf_renderer<I, T>(args: I, renderer: &dyn PdfRenderer) -> i32
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let argv: Vec<String> = args
        .into_iter()
        .map(|value| value.into().to_string_lossy().into_owned())
        .collect();

    match parse_cli(argv) {
        Ok(ParseResult::Help(help_text)) => {
            print!("{help_text}");
            EXIT_SUCCESS
        }
        Ok(ParseResult::Validate(validate_args)) => run_validate(validate_args),
        Ok(ParseResult::Convert(convert_cli_args)) => run_convert(convert_cli_args, renderer),
        Err(parse_failure) => {
            eprintln!("{}", parse_failure.message);
            EXIT_VALIDATION_FAILURE
        }
    }
}

fn run_validate(args: ValidateArgs) -> i32 {
    let analyzed_input = match analyze_input(&args) {
        Ok(analyzed_input) => analyzed_input,
        Err(failure) => {
            print_failure(&failure);
            return failure.exit_code();
        }
    };

    let selection = determine_selection(&analyzed_input, &args);
    let diagnostics =
        filter_diagnostics(&analyzed_input.project_index, &selection.selected_entries);
    let remote_assets = match materialize_remote_assets_for_entry(
        None,
        &diagnostics.assets,
        RemoteAssetApplyMode::KeepExternal,
    ) {
        Ok(remote_assets) => remote_assets,
        Err(error) => {
            eprintln!("System failure.\n- Failed to inspect remote assets: {error}");
            return EXIT_SYSTEM_FAILURE;
        }
    };
    let report = build_validation_report(
        &analyzed_input,
        &args,
        selection,
        diagnostics,
        remote_assets,
    );

    if let Some(report_path) = &args.report
        && let Err(error) = write_json_report(report_path, &report, "validation report")
    {
        eprintln!("System failure.\n- {error}");
        return EXIT_SYSTEM_FAILURE;
    }

    let console_output = render_console_report(&report, args.report.as_deref());
    if report.exit_code == EXIT_VALIDATION_FAILURE {
        eprint!("{console_output}");
    } else {
        print!("{console_output}");
    }

    report.exit_code
}

fn run_convert(cli_args: ConvertCliArgs, renderer: &dyn PdfRenderer) -> i32 {
    let args = match resolve_convert_args(cli_args) {
        Ok(args) => args,
        Err(failure) => {
            print_failure_with_label("Conversion failed.", &failure);
            return failure.exit_code();
        }
    };

    let analyzed_input = match analyze_convert_input(&args) {
        Ok(analyzed_input) => analyzed_input,
        Err(failure) => {
            print_failure_with_label("Conversion failed.", &failure);
            return failure.exit_code();
        }
    };

    let selection = determine_selection(
        &analyzed_input,
        &ValidateArgs {
            input: None,
            entry: args.entry.clone(),
            all: args.all,
            strict: false,
            report: None,
        },
    );

    if !selection.errors.is_empty() {
        let failure = AppFailure::validation(selection.errors.join(" "));
        print_failure_with_label("Conversion failed.", &failure);
        return failure.exit_code();
    }

    let convert_mode = convert_mode_from_selection(&selection);
    if let Err(failure) = validate_convert_request(&analyzed_input, &args, convert_mode) {
        print_failure_with_label("Conversion failed.", &failure);
        return failure.exit_code();
    }

    let prepared_workspace = match prepare_render_workspace(&analyzed_input) {
        Ok(prepared_workspace) => prepared_workspace,
        Err(failure) => {
            print_failure_with_label("Conversion failed.", &failure);
            return failure.exit_code();
        }
    };

    let render_support_files = match load_render_support_files(&args) {
        Ok(render_support_files) => render_support_files,
        Err(failure) => {
            print_failure_with_label("Conversion failed.", &failure);
            return failure.exit_code();
        }
    };

    match convert_mode {
        ConvertMode::Single => run_single_convert(
            &args,
            &analyzed_input,
            &selection,
            &prepared_workspace,
            &render_support_files,
            renderer,
        ),
        ConvertMode::Batch => run_batch_convert(
            &args,
            &analyzed_input,
            &selection,
            &prepared_workspace,
            &render_support_files,
            renderer,
        ),
    }
}

fn convert_mode_from_selection(selection: &SelectionDecision) -> ConvertMode {
    if matches!(selection.mode, SelectionMode::All) {
        ConvertMode::Batch
    } else {
        ConvertMode::Single
    }
}

fn validate_convert_request(
    analyzed_input: &AnalyzedInput,
    args: &ConvertArgs,
    convert_mode: ConvertMode,
) -> Result<(), AppFailure> {
    if matches!(analyzed_input.input_kind, ValidationInputKind::MarkdownFile) {
        if args.entry.is_some() {
            return Err(AppFailure::validation(
                "--entry cannot be used with a Markdown file input.".to_string(),
            ));
        }
        if args.all {
            return Err(AppFailure::validation(
                "--all cannot be used with a Markdown file input.".to_string(),
            ));
        }
    }

    match convert_mode {
        ConvertMode::Single => {
            if args.out_dir.is_some() {
                return Err(AppFailure::validation(
                    "--out-dir can be used only with batch conversion.".to_string(),
                ));
            }
        }
        ConvertMode::Batch => {
            if args.output.is_some() {
                return Err(AppFailure::validation(
                    "--output cannot be used with batch conversion. Use --out-dir.".to_string(),
                ));
            }
            if args.out_dir.is_none() {
                return Err(AppFailure::validation(
                    "Batch conversion requires --out-dir.".to_string(),
                ));
            }
            if args.debug_html.is_some() {
                return Err(AppFailure::validation(
                    "--debug-html can be used only with single conversion.".to_string(),
                ));
            }
            if args.asset_manifest.is_some() {
                return Err(AppFailure::validation(
                    "--asset-manifest can be used only with single conversion.".to_string(),
                ));
            }
        }
    }

    Ok(())
}

fn prepare_render_workspace(
    analyzed_input: &AnalyzedInput,
) -> Result<PreparedWorkspace, AppFailure> {
    if let Some(workspace_root) = &analyzed_input.workspace_root {
        return Ok(PreparedWorkspace {
            root: workspace_root.clone(),
            _temp_dir: None,
        });
    }

    if matches!(analyzed_input.input_kind, ValidationInputKind::Zip) {
        let temp_dir = materialize_zip_workspace(
            &analyzed_input.resolved_input_path,
            analyzed_input.strip_zip_prefix,
        )?;
        let root = temp_dir.path().to_path_buf();
        return Ok(PreparedWorkspace {
            root,
            _temp_dir: Some(temp_dir),
        });
    }

    Err(AppFailure::system(
        "Conversion requires a workspace-backed input.".to_string(),
    ))
}

fn materialize_zip_workspace(zip_path: &Path, strip_prefix: bool) -> Result<TempDir, AppFailure> {
    let file = fs::File::open(zip_path).map_err(|error| {
        AppFailure::system(format!(
            "Failed to open ZIP input {}: {error}",
            zip_path.display()
        ))
    })?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|error| AppFailure::validation(format!("Invalid ZIP input: {error}")))?;
    let temp_dir = TempDir::new().map_err(|error| {
        AppFailure::system(format!(
            "Failed to create a temporary workspace for ZIP conversion: {error}"
        ))
    })?;

    // Collect all entries with normalized paths first (needed for prefix detection)
    let mut collected_entries: Vec<(String, Vec<u8>)> = Vec::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|error| {
            AppFailure::validation(format!("Failed to read ZIP entry {index}: {error}"))
        })?;

        if entry.is_dir() {
            continue;
        }

        let raw_path = entry.name().to_string();
        let normalized_path = normalize_relative_string(&raw_path).map_err(|_| {
            AppFailure::validation(format!("Unsafe ZIP entry path detected: {raw_path}"))
        })?;

        let mut contents: Vec<u8> = Vec::new();
        entry.read_to_end(&mut contents).map_err(|error| {
            AppFailure::validation(format!("Failed to extract ZIP entry {raw_path}: {error}"))
        })?;

        collected_entries.push((normalized_path, contents));
    }

    // Only strip the common prefix for GitHub-style archives
    let prefix_len: usize = if strip_prefix {
        detect_common_prefix_len(&collected_entries)
    } else {
        0
    };

    for (normalized_path, contents) in &collected_entries {
        let stripped_path = &normalized_path[prefix_len..];
        let output_path = normalized_path_to_filesystem_path(temp_dir.path(), stripped_path);

        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                AppFailure::system(format!(
                    "Failed to create the ZIP extraction directory {}: {error}",
                    parent.display()
                ))
            })?;
        }

        fs::write(&output_path, contents).map_err(|error| {
            AppFailure::system(format!(
                "Failed to write the extracted ZIP entry {}: {error}",
                output_path.display()
            ))
        })?;
    }

    Ok(temp_dir)
}

/// Returns the length (including trailing `/`) of the common first path segment
/// shared by all entries, or 0 if no common prefix exists.
fn detect_common_prefix_len(entries: &[(String, Vec<u8>)]) -> usize {
    if entries.is_empty() {
        return 0;
    }

    let common = match entries[0].0.split('/').next() {
        Some(segment) => segment,
        None => return 0,
    };

    let all_share_prefix = entries.iter().all(|(path, _)| {
        path.starts_with(common)
            && path.len() > common.len()
            && path.as_bytes()[common.len()] == b'/'
    });

    if all_share_prefix {
        common.len() + 1
    } else {
        0
    }
}

fn run_single_convert(
    args: &ConvertArgs,
    analyzed_input: &AnalyzedInput,
    selection: &SelectionDecision,
    prepared_workspace: &PreparedWorkspace,
    render_support_files: &LoadedRenderSupportFiles,
    renderer: &dyn PdfRenderer,
) -> i32 {
    let Some(selected_entry) = selection.selected_entries.first().cloned() else {
        let failure = AppFailure::validation("No entry markdown file was selected.".to_string());
        print_failure_with_label("Conversion failed.", &failure);
        return failure.exit_code();
    };

    let output_path =
        match resolve_single_convert_output_path(args, analyzed_input, &selected_entry) {
            Ok(output_path) => output_path,
            Err(failure) => {
                print_failure_with_label("Conversion failed.", &failure);
                return failure.exit_code();
            }
        };

    let converted_entry = match convert_entry_to_pdf(
        &prepared_workspace.root,
        &analyzed_input.project_index,
        &selected_entry,
        output_path,
        args,
        render_support_files,
        renderer,
    ) {
        Ok(converted_entry) => converted_entry,
        Err(failure) => {
            print_failure_with_label("Conversion failed.", &failure);
            return failure.exit_code();
        }
    };

    let report = build_single_convert_report(analyzed_input, selection, &converted_entry);
    if let Some(report_path) = &args.render_report
        && let Err(error) = write_json_report(report_path, &report, "conversion report")
    {
        eprintln!("System failure.\n- {error}");
        return EXIT_SYSTEM_FAILURE;
    }

    let console_output = render_convert_console_output(
        &converted_entry.entry_path,
        &converted_entry.output_path,
        &converted_entry.warnings,
    );
    print!("{console_output}");
    report.exit_code
}

fn run_batch_convert(
    args: &ConvertArgs,
    analyzed_input: &AnalyzedInput,
    selection: &SelectionDecision,
    prepared_workspace: &PreparedWorkspace,
    render_support_files: &LoadedRenderSupportFiles,
    renderer: &dyn PdfRenderer,
) -> i32 {
    let out_dir = args
        .out_dir
        .as_deref()
        .expect("batch conversion should require --out-dir");
    let mut report = build_convert_report(analyzed_input, selection, ConvertMode::Batch);

    let batch_targets = match plan_batch_output_targets(out_dir, &selection.selected_entries) {
        Ok(batch_targets) => batch_targets,
        Err(collisions) => {
            report.collisions = collisions;
            report
                .errors
                .extend(report.collisions.iter().map(|collision| {
                    format!(
                        "Output path collision: {} <= {}",
                        collision.output_path,
                        collision.entry_paths.join(", ")
                    )
                }));
            finalize_convert_report(&mut report);

            if let Some(report_path) = &args.render_report
                && let Err(error) = write_json_report(report_path, &report, "conversion report")
            {
                eprintln!("System failure.\n- {error}");
                return EXIT_SYSTEM_FAILURE;
            }

            let console_output =
                render_batch_convert_console_output(&report, args.render_report.as_deref());
            eprint!("{console_output}");
            return report.exit_code;
        }
    };

    for batch_target in batch_targets {
        match convert_entry_to_pdf(
            &prepared_workspace.root,
            &analyzed_input.project_index,
            &batch_target.entry_path,
            batch_target.output_path.clone(),
            args,
            render_support_files,
            renderer,
        ) {
            Ok(converted_entry) => {
                report
                    .warnings
                    .extend(converted_entry.warnings.iter().cloned());
                report
                    .remote_assets
                    .extend(converted_entry.remote_assets.clone());
                report.outputs.push(ConvertOutputReport {
                    entry_path: converted_entry.entry_path,
                    output_path: converted_entry.output_path.display().to_string(),
                    warnings: converted_entry.warnings,
                });
            }
            Err(failure) => {
                report
                    .errors
                    .push(format!("{}: {}", batch_target.entry_path, failure.message));
                report.failures.push(ConvertFailureReport {
                    entry_path: Some(batch_target.entry_path),
                    output_path: Some(batch_target.output_path.display().to_string()),
                    kind: failure.kind,
                    message: failure.message,
                });
            }
        }
    }

    finalize_convert_report(&mut report);

    if let Some(report_path) = &args.render_report
        && let Err(error) = write_json_report(report_path, &report, "conversion report")
    {
        eprintln!("System failure.\n- {error}");
        return EXIT_SYSTEM_FAILURE;
    }

    let console_output =
        render_batch_convert_console_output(&report, args.render_report.as_deref());
    if report.exit_code >= EXIT_VALIDATION_FAILURE {
        eprint!("{console_output}");
    } else {
        print!("{console_output}");
    }

    report.exit_code
}

fn convert_entry_to_pdf(
    workspace_root: &Path,
    project_index: &ProjectIndex,
    entry_path: &str,
    output_path: PathBuf,
    args: &ConvertArgs,
    render_support_files: &LoadedRenderSupportFiles,
    renderer: &dyn PdfRenderer,
) -> Result<EntryConvertSuccess, AppFailure> {
    let render_options = RenderOptions {
        theme: args.theme,
        metadata: args.metadata.clone(),
        custom_css: render_support_files.custom_css.clone(),
        enable_toc: args.enable_toc,
        sanitize_html: args.sanitize_html,
        mermaid_mode: args.mermaid_mode,
        math_mode: args.math_mode,
        mermaid_timeout_ms: args.mermaid_timeout_ms,
        math_timeout_ms: args.math_timeout_ms,
    };
    let rendered_document =
        render_workspace_entry_with_options(workspace_root, entry_path, &render_options)
            .map_err(map_render_error)?;

    let selected_entry = entry_path.to_string();
    let diagnostics = filter_diagnostics(project_index, std::slice::from_ref(&selected_entry));
    let remote_asset_materialization = materialize_remote_assets_for_entry(
        Some(&rendered_document.html),
        &diagnostics.assets,
        RemoteAssetApplyMode::InlineHtml,
    )
    .map_err(|error| AppFailure::system(format!("Failed to materialize remote assets: {error}")))?;
    let rendered_html = remote_asset_materialization
        .html
        .clone()
        .unwrap_or(rendered_document.html.clone());
    let mut warnings = build_convert_warnings(&diagnostics, &remote_asset_materialization.warnings);

    ensure_parent_directory(&output_path)?;

    if let Some(debug_html_path) = &args.debug_html {
        write_text_artifact(debug_html_path, &rendered_html, "debug HTML")?;
        write_debug_runtime_assets(debug_html_path, &rendered_html)?;
    }
    if let Some(asset_manifest_path) = &args.asset_manifest {
        write_json_artifact(
            asset_manifest_path,
            &AssetManifest {
                entry_path: selected_entry.clone(),
                assets: diagnostics.assets.clone(),
                remote_assets: remote_asset_materialization.remote_assets.clone(),
                missing_assets: diagnostics.missing_assets.clone(),
                path_errors: diagnostics.path_errors.clone(),
                warnings: warnings.clone(),
            },
            "asset manifest",
        )?;
    }

    let render_request = PdfRenderRequest {
        title: rendered_document.title.clone(),
        html: rendered_html,
        output_path: output_path.clone(),
        page_size: args.page_size,
        margins_mm: args.margins_mm,
        landscape: args.landscape,
        metadata: args.metadata.clone(),
        header_template: prepare_print_template(
            render_support_files.header_template_source.as_deref(),
            &rendered_document.title,
            entry_path,
        )?,
        footer_template: prepare_print_template(
            render_support_files.footer_template_source.as_deref(),
            &rendered_document.title,
            entry_path,
        )?,
    };

    let render_outcome = renderer
        .render(&render_request)
        .map_err(|error| AppFailure {
            kind: error.kind,
            message: format!("PDF generation failed: {}", error.message),
        })?;
    warnings.extend(render_outcome.warnings);

    apply_pdf_metadata(&output_path, &args.metadata)
        .map_err(|error| AppFailure::system(format!("Failed to update PDF metadata: {error}")))?;

    Ok(EntryConvertSuccess {
        entry_path: selected_entry,
        output_path,
        warnings,
        remote_assets: remote_asset_materialization.remote_assets,
    })
}

fn ensure_parent_directory(output_path: &Path) -> Result<(), AppFailure> {
    if let Some(parent) = output_path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(|error| {
            AppFailure::system(format!(
                "Failed to create the output directory {}: {error}",
                parent.display()
            ))
        })?;
    }

    Ok(())
}

fn load_render_support_files(args: &ConvertArgs) -> Result<LoadedRenderSupportFiles, AppFailure> {
    Ok(LoadedRenderSupportFiles {
        custom_css: load_optional_text_file(args.css_path.as_deref(), "stylesheet file")?,
        header_template_source: load_optional_text_file(
            args.header_template_path.as_deref(),
            "header template",
        )?,
        footer_template_source: load_optional_text_file(
            args.footer_template_path.as_deref(),
            "footer template",
        )?,
    })
}

fn load_optional_text_file(path: Option<&Path>, label: &str) -> Result<Option<String>, AppFailure> {
    let Some(path) = path else {
        return Ok(None);
    };

    fs::read_to_string(path).map(Some).map_err(|error| {
        AppFailure::validation(format!(
            "{label} could not be read {}: {error}",
            path.display()
        ))
    })
}

fn write_text_artifact(path: &Path, contents: &str, label: &str) -> Result<(), AppFailure> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(|error| {
            AppFailure::system(format!(
                "Failed to create the {label} directory {}: {error}",
                parent.display()
            ))
        })?;
    }

    fs::write(path, contents).map_err(|error| {
        AppFailure::system(format!(
            "Failed to write the {label} {}: {error}",
            path.display()
        ))
    })
}

fn write_json_artifact<T: Serialize>(
    path: &Path,
    value: &T,
    label: &str,
) -> Result<(), AppFailure> {
    let json = serde_json::to_string_pretty(value)
        .map_err(|error| AppFailure::system(format!("Failed to serialize the {label}: {error}")))?;
    write_text_artifact(path, &json, label)
}

pub fn prepare_print_template_html(
    template_source: Option<&str>,
    title: &str,
    entry_path: &str,
) -> Result<Option<String>, String> {
    let Some(template_source) = template_source else {
        return Ok(None);
    };

    let normalized_template = template_source.to_ascii_lowercase();
    if normalized_template.contains("<script") || normalized_template.contains("javascript:") {
        return Err(
            "Header/footer templates cannot contain scripts or javascript URLs.".to_string(),
        );
    }

    let template = template_source
        .replace("{{title}}", &escape_print_template_text(title))
        .replace("{{entryPath}}", &escape_print_template_text(entry_path))
        .replace("{{pageNumber}}", "<span class=\"pageNumber\"></span>")
        .replace("{{totalPages}}", "<span class=\"totalPages\"></span>")
        .replace("{{date}}", "<span class=\"date\"></span>");

    Ok(Some(format!(
        "<div style=\"font-size:9px;width:100%;padding:0 8px;color:#374151;\">{template}</div>"
    )))
}

fn prepare_print_template(
    template_source: Option<&str>,
    title: &str,
    entry_path: &str,
) -> Result<Option<String>, AppFailure> {
    prepare_print_template_html(template_source, title, entry_path).map_err(AppFailure::validation)
}

fn escape_print_template_text(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn print_failure(failure: &AppFailure) {
    print_failure_with_label(
        match failure.kind {
            FailureKind::Validation => "Validation failed.",
            FailureKind::System => "System failure.",
        },
        failure,
    );
}

fn print_failure_with_label(label: &str, failure: &AppFailure) {
    match failure.kind {
        FailureKind::Validation | FailureKind::System => {
            eprintln!("{label}\n- {}", failure.message)
        }
    }
}

fn analyze_input(args: &ValidateArgs) -> Result<AnalyzedInput, AppFailure> {
    analyze_input_path(args.input.as_deref())
}

fn analyze_convert_input(args: &ConvertArgs) -> Result<AnalyzedInput, AppFailure> {
    analyze_input_path(args.input.as_deref())
}

fn analyze_input_path(input: Option<&Path>) -> Result<AnalyzedInput, AppFailure> {
    let resolved_input = resolve_input(input)?;

    match resolved_input {
        ResolvedInput::MarkdownFile { path, display_path } => {
            let canonical_file = path.canonicalize().map_err(|error| {
                AppFailure::system(format!(
                    "Failed to canonicalize markdown input {}: {error}",
                    path.display()
                ))
            })?;
            let parent = canonical_file.parent().ok_or_else(|| {
                AppFailure::system(format!(
                    "Markdown input {} has no parent directory.",
                    canonical_file.display()
                ))
            })?;
            let relative_path = canonical_file.strip_prefix(parent).map_err(|error| {
                AppFailure::system(format!(
                    "Failed to derive the markdown entry path for {}: {error}",
                    canonical_file.display()
                ))
            })?;
            let explicit_entry = normalize_path(relative_path).map_err(|_| {
                AppFailure::system(format!(
                    "Markdown input {} could not be normalized.",
                    canonical_file.display()
                ))
            })?;
            let workspace_root = parent.to_path_buf();
            let project_index = analyze_workspace(parent).map_err(map_analyze_error)?;

            Ok(AnalyzedInput {
                resolved_input_path: canonical_file,
                input_kind: ValidationInputKind::MarkdownFile,
                input_path: display_path,
                is_default_input: false,
                uses_implicit_all: false,
                explicit_entry: Some(explicit_entry),
                workspace_root: Some(workspace_root.clone()),
                default_output_directory: Some(workspace_root),
                project_index,
                strip_zip_prefix: false,
                _temp_dir: None,
            })
        }
        ResolvedInput::Zip { path, display_path } => {
            let canonical_zip_path = path.canonicalize().map_err(|error| {
                AppFailure::system(format!(
                    "Failed to canonicalize ZIP input {}: {error}",
                    path.display()
                ))
            })?;
            let bytes = fs::read(&path).map_err(|error| {
                AppFailure::system(format!(
                    "Failed to read ZIP input {}: {error}",
                    path.display()
                ))
            })?;
            let project_index = analyze_zip(&bytes).map_err(map_analyze_error)?;
            let default_output_directory = canonical_zip_path
                .parent()
                .map(Path::to_path_buf)
                .or_else(|| env::current_dir().ok());

            Ok(AnalyzedInput {
                resolved_input_path: canonical_zip_path,
                input_kind: ValidationInputKind::Zip,
                input_path: display_path,
                is_default_input: false,
                uses_implicit_all: false,
                explicit_entry: None,
                workspace_root: None,
                default_output_directory,
                project_index,
                strip_zip_prefix: false,
                _temp_dir: None,
            })
        }
        ResolvedInput::Folder {
            path,
            display_path,
            is_default_input,
        } => {
            let canonical_root = path.canonicalize().map_err(|error| {
                AppFailure::system(format!(
                    "Failed to canonicalize folder input {}: {error}",
                    path.display()
                ))
            })?;
            let project_index = analyze_workspace(&canonical_root).map_err(map_analyze_error)?;

            Ok(AnalyzedInput {
                resolved_input_path: canonical_root.clone(),
                input_kind: ValidationInputKind::Folder,
                input_path: display_path,
                is_default_input,
                uses_implicit_all: !is_default_input,
                explicit_entry: None,
                workspace_root: Some(canonical_root.clone()),
                default_output_directory: Some(canonical_root),
                project_index,
                strip_zip_prefix: false,
                _temp_dir: None,
            })
        }
        ResolvedInput::GitHubUrl {
            display_path,
            parsed,
        } => {
            let token: Option<String> = resolve_github_auth_token();

            let git_ref: String = match &parsed.git_ref {
                Some(r) => r.clone(),
                None => {
                    resolve_github_default_branch(&parsed.owner, &parsed.repo, token.as_deref())?
                }
            };

            eprintln!(
                "Downloading GitHub archive: {}/{} @ {} ...",
                parsed.owner, parsed.repo, git_ref
            );
            let zip_bytes: Vec<u8> =
                download_github_archive(&parsed.owner, &parsed.repo, &git_ref, token.as_deref())?;

            // Save to temp file so the existing ZIP pipeline can process it
            let temp_dir: TempDir = TempDir::new().map_err(|error| {
                AppFailure::system(format!("Failed to create temp directory: {error}"))
            })?;
            let temp_zip_path: PathBuf = temp_dir.path().join("github-archive.zip");
            fs::write(&temp_zip_path, &zip_bytes).map_err(|error| {
                AppFailure::system(format!("Failed to write temp archive: {error}"))
            })?;

            // GitHub archives nest files under {repo}-{ref}/, strip that prefix
            let project_index: ProjectIndex =
                analyze_zip_strip_prefix(&zip_bytes).map_err(map_analyze_error)?;

            // If URL pointed to a specific file (/blob/), use it as implicit entry
            let explicit_entry: Option<String> = if parsed.is_file_reference {
                parsed.subpath.clone()
            } else {
                None
            };

            Ok(AnalyzedInput {
                resolved_input_path: temp_zip_path,
                input_kind: ValidationInputKind::Zip,
                input_path: display_path,
                is_default_input: false,
                uses_implicit_all: false,
                explicit_entry,
                workspace_root: None,
                default_output_directory: Some(env::current_dir().unwrap_or_default()),
                project_index,
                strip_zip_prefix: true,
                _temp_dir: Some(temp_dir),
            })
        }
    }
}

fn resolve_input(input: Option<&Path>) -> Result<ResolvedInput, AppFailure> {
    let is_default_input = input.is_none();
    let path = match input {
        Some(path) => path.to_path_buf(),
        None => env::current_dir().map_err(|error| {
            AppFailure::system(format!("Failed to read the current directory: {error}"))
        })?,
    };

    // Check for GitHub URL before filesystem access
    if let Some(path_str) = path.to_str() {
        if let Some(parsed) = parse_github_url(path_str) {
            return Ok(ResolvedInput::GitHubUrl {
                display_path: path_str.to_string(),
                parsed,
            });
        }
    }

    let display_path = path.display().to_string();
    let metadata = fs::metadata(&path).map_err(|error| {
        AppFailure::validation(format!(
            "Input path {} could not be read: {error}",
            path.display()
        ))
    })?;

    if metadata.is_dir() {
        return Ok(ResolvedInput::Folder {
            path,
            display_path,
            is_default_input,
        });
    }

    if !metadata.is_file() {
        return Err(AppFailure::validation(format!(
            "Input path {} is neither a file nor a directory.",
            path.display()
        )));
    }

    if is_markdown_path(&path) {
        return Ok(ResolvedInput::MarkdownFile { path, display_path });
    }

    if is_zip_path(&path) {
        return Ok(ResolvedInput::Zip { path, display_path });
    }

    Err(AppFailure::validation(format!(
        "Unsupported input type: {}",
        path.display()
    )))
}

fn map_analyze_error(error: AnalyzeError) -> AppFailure {
    match error {
        AnalyzeError::Io(message) => AppFailure::system(message),
        AnalyzeError::UnsafePath { .. }
        | AnalyzeError::ZipArchive(_)
        | AnalyzeError::ZipLimitsExceeded(_) => AppFailure::validation(error.to_string()),
    }
}

fn map_render_error(error: RenderHtmlError) -> AppFailure {
    match error {
        RenderHtmlError::EntryNotFound { .. } | RenderHtmlError::InvalidEntryPath { .. } => {
            AppFailure::validation(error.to_string())
        }
        RenderHtmlError::Analyze(analyze_error) => map_analyze_error(analyze_error),
        RenderHtmlError::Io(_) | RenderHtmlError::InvalidUtf8 { .. } => {
            AppFailure::system(error.to_string())
        }
    }
}

fn determine_selection(analyzed_input: &AnalyzedInput, args: &ValidateArgs) -> SelectionDecision {
    if let Some(explicit_entry) = &analyzed_input.explicit_entry {
        return SelectionDecision {
            mode: SelectionMode::ExplicitMarkdownFile,
            requested_entry: Some(explicit_entry.clone()),
            selected_entries: vec![explicit_entry.clone()],
            errors: Vec::new(),
        };
    }

    if let Some(requested_entry) = &args.entry {
        return match normalize_relative_string(requested_entry) {
            Ok(normalized_entry) => {
                if analyzed_input
                    .project_index
                    .entry_candidates
                    .iter()
                    .any(|candidate| candidate.path == normalized_entry)
                {
                    SelectionDecision {
                        mode: SelectionMode::Entry,
                        requested_entry: Some(normalized_entry.clone()),
                        selected_entries: vec![normalized_entry],
                        errors: Vec::new(),
                    }
                } else {
                    SelectionDecision {
                        mode: SelectionMode::Entry,
                        requested_entry: Some(normalized_entry.clone()),
                        selected_entries: Vec::new(),
                        errors: vec![format!(
                            "Entry markdown file could not be found: {normalized_entry}"
                        )],
                    }
                }
            }
            Err(()) => SelectionDecision {
                mode: SelectionMode::Entry,
                requested_entry: Some(requested_entry.clone()),
                selected_entries: Vec::new(),
                errors: vec![format!("Invalid --entry path: {requested_entry}")],
            },
        };
    }

    if args.all || analyzed_input.uses_implicit_all {
        if analyzed_input.project_index.entry_candidates.is_empty() {
            return SelectionDecision {
                mode: SelectionMode::All,
                requested_entry: None,
                selected_entries: Vec::new(),
                errors: vec![missing_entry_message(analyzed_input)],
            };
        }

        return SelectionDecision {
            mode: SelectionMode::All,
            requested_entry: None,
            selected_entries: analyzed_input
                .project_index
                .entry_candidates
                .iter()
                .map(|candidate| candidate.path.clone())
                .collect(),
            errors: Vec::new(),
        };
    }

    if let Some(selected_entry) = &analyzed_input.project_index.selected_entry {
        return SelectionDecision {
            mode: SelectionMode::Auto,
            requested_entry: None,
            selected_entries: vec![selected_entry.clone()],
            errors: Vec::new(),
        };
    }

    let selection_error = match analyzed_input.project_index.entry_selection_reason {
        EntrySelectionReason::MultipleCandidates => {
            "Multiple markdown files were detected. Use --entry or --all.".to_string()
        }
        EntrySelectionReason::NoMarkdownFiles => missing_entry_message(analyzed_input),
        _ => "Entry markdown file could not be determined.".to_string(),
    };

    SelectionDecision {
        mode: SelectionMode::Auto,
        requested_entry: None,
        selected_entries: Vec::new(),
        errors: vec![selection_error],
    }
}

fn missing_entry_message(analyzed_input: &AnalyzedInput) -> String {
    match analyzed_input.input_kind {
        ValidationInputKind::Zip => {
            "No entry markdown file was found inside the ZIP input.".to_string()
        }
        ValidationInputKind::Folder if analyzed_input.is_default_input => {
            "No entry markdown file was found in the current directory.".to_string()
        }
        _ => "No entry markdown file was found.".to_string(),
    }
}

fn filter_diagnostics(
    project_index: &ProjectIndex,
    selected_entries: &[String],
) -> FilteredDiagnostics {
    if selected_entries.is_empty() {
        return FilteredDiagnostics {
            assets: Vec::new(),
            ignored_files: project_index.diagnostic.ignored_files.clone(),
            missing_assets: Vec::new(),
            path_errors: Vec::new(),
            warnings: Vec::new(),
        };
    }

    let assets: Vec<AssetRef> = project_index
        .assets
        .iter()
        .filter(|asset| {
            selected_entries
                .iter()
                .any(|entry| entry == &asset.entry_path)
        })
        .cloned()
        .collect();
    let missing_assets: Vec<String> = project_index
        .diagnostic
        .missing_assets
        .iter()
        .filter(|message| message_belongs_to_selected_entry(message, selected_entries))
        .cloned()
        .collect();
    let path_errors: Vec<String> = project_index
        .diagnostic
        .path_errors
        .iter()
        .filter(|message| message_belongs_to_selected_entry(message, selected_entries))
        .cloned()
        .collect();
    let warnings: Vec<String> = project_index
        .diagnostic
        .warnings
        .iter()
        .filter(|message| message_belongs_to_selected_entry(message, selected_entries))
        .cloned()
        .collect();

    FilteredDiagnostics {
        assets,
        ignored_files: project_index.diagnostic.ignored_files.clone(),
        missing_assets,
        path_errors,
        warnings,
    }
}

fn message_belongs_to_selected_entry(message: &str, selected_entries: &[String]) -> bool {
    selected_entries
        .iter()
        .any(|entry| message.starts_with(&format!("{entry} -> ")))
}

fn build_validation_report(
    analyzed_input: &AnalyzedInput,
    args: &ValidateArgs,
    selection: SelectionDecision,
    diagnostics: FilteredDiagnostics,
    remote_asset_materialization: RemoteAssetMaterialization,
) -> ValidationReport {
    let mut errors: Vec<String> = selection.errors;
    let mut warnings: Vec<String> = Vec::new();

    for path_error in &diagnostics.path_errors {
        errors.push(format!("Invalid asset path: {path_error}"));
    }

    if args.strict {
        errors.extend(
            diagnostics
                .missing_assets
                .iter()
                .map(|missing_asset| format!("Missing asset: {missing_asset}")),
        );
        errors.extend(diagnostics.warnings.iter().cloned());
        errors.extend(remote_asset_materialization.warnings.iter().cloned());
    } else {
        warnings.extend(
            diagnostics
                .missing_assets
                .iter()
                .map(|missing_asset| format!("Missing asset: {missing_asset}")),
        );
        warnings.extend(diagnostics.warnings.iter().cloned());
        warnings.extend(remote_asset_materialization.warnings.iter().cloned());
    }

    let exit_code = if !errors.is_empty() {
        EXIT_VALIDATION_FAILURE
    } else if !warnings.is_empty() {
        EXIT_WARNING
    } else {
        EXIT_SUCCESS
    };

    let status = match exit_code {
        EXIT_SUCCESS => ValidationStatus::Success,
        EXIT_WARNING => ValidationStatus::Warning,
        EXIT_VALIDATION_FAILURE => ValidationStatus::Failure,
        _ => ValidationStatus::Failure,
    };

    ValidationReport {
        status,
        exit_code,
        input_kind: analyzed_input.input_kind,
        input_path: analyzed_input.input_path.clone(),
        strict: args.strict,
        source_kind: analyzed_input.project_index.source_kind.clone(),
        selection_mode: selection.mode,
        requested_entry: selection.requested_entry,
        selected_entries: selection.selected_entries,
        entry_selection_reason: analyzed_input.project_index.entry_selection_reason.clone(),
        entry_candidates: analyzed_input.project_index.entry_candidates.clone(),
        assets: diagnostics.assets,
        remote_assets: remote_asset_materialization.remote_assets,
        ignored_files: diagnostics.ignored_files,
        missing_assets: diagnostics.missing_assets,
        path_errors: diagnostics.path_errors,
        warnings,
        errors,
        runtime_info: collect_runtime_info(),
    }
}

fn build_convert_warnings(
    diagnostics: &FilteredDiagnostics,
    remote_asset_warnings: &[String],
) -> Vec<String> {
    let mut warnings: Vec<String> = diagnostics
        .missing_assets
        .iter()
        .map(|missing_asset| format!("Missing asset: {missing_asset}"))
        .collect();
    warnings.extend(
        diagnostics
            .path_errors
            .iter()
            .map(|path_error| format!("Invalid asset path: {path_error}")),
    );
    warnings.extend(diagnostics.warnings.iter().cloned());
    warnings.extend(remote_asset_warnings.iter().cloned());
    warnings
}

fn resolve_single_convert_output_path(
    args: &ConvertArgs,
    analyzed_input: &AnalyzedInput,
    selected_entry: &str,
) -> Result<PathBuf, AppFailure> {
    if let Some(output_path) = &args.output {
        return Ok(output_path.clone());
    }

    let default_output_directory = analyzed_input
        .default_output_directory
        .as_ref()
        .ok_or_else(|| {
            AppFailure::system("No default output directory is available.".to_string())
        })?;
    let output_file_name = default_pdf_file_name(selected_entry);

    Ok(default_output_directory.join(output_file_name))
}

fn plan_batch_output_targets(
    out_dir: &Path,
    selected_entries: &[String],
) -> Result<Vec<BatchOutputTarget>, Vec<ConvertCollisionReport>> {
    let mut grouped_targets: BTreeMap<String, Vec<BatchOutputTarget>> = BTreeMap::new();

    for selected_entry in selected_entries {
        let output_path = batch_output_path(out_dir, selected_entry);
        grouped_targets
            .entry(output_collision_key(&output_path))
            .or_default()
            .push(BatchOutputTarget {
                entry_path: selected_entry.clone(),
                output_path,
            });
    }

    let mut batch_targets: Vec<BatchOutputTarget> = Vec::new();
    let mut collisions: Vec<ConvertCollisionReport> = Vec::new();

    for grouped_target in grouped_targets.into_values() {
        if grouped_target.len() > 1 {
            collisions.push(ConvertCollisionReport {
                output_path: grouped_target[0].output_path.display().to_string(),
                entry_paths: grouped_target
                    .iter()
                    .map(|target| target.entry_path.clone())
                    .collect(),
            });
            continue;
        }

        batch_targets.push(
            grouped_target
                .into_iter()
                .next()
                .expect("group should contain one target"),
        );
    }

    if collisions.is_empty() {
        batch_targets.sort_by(|left, right| left.entry_path.cmp(&right.entry_path));
        Ok(batch_targets)
    } else {
        Err(collisions)
    }
}

fn batch_output_path(out_dir: &Path, selected_entry: &str) -> PathBuf {
    let mut relative_output_path: PathBuf = PathBuf::new();
    for segment in selected_entry.split('/') {
        relative_output_path.push(segment);
    }
    relative_output_path.set_extension("pdf");
    out_dir.join(relative_output_path)
}

fn output_collision_key(path: &Path) -> String {
    let key = path.to_string_lossy().replace('\\', "/");
    if cfg!(windows) || cfg!(target_os = "macos") {
        key.to_ascii_lowercase()
    } else {
        key
    }
}

fn default_pdf_file_name(selected_entry: &str) -> String {
    let entry_file_name = Path::new(selected_entry)
        .file_name()
        .and_then(|file_name| file_name.to_str())
        .unwrap_or("document.md");
    let stem = Path::new(entry_file_name)
        .file_stem()
        .and_then(|file_stem| file_stem.to_str())
        .unwrap_or("document");

    format!("{stem}.pdf")
}

fn write_json_report<T: Serialize>(
    report_path: &Path,
    report: &T,
    report_label: &str,
) -> Result<(), String> {
    if let Some(parent) = report_path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "Failed to create the report directory {}: {error}",
                parent.display()
            )
        })?;
    }

    let report_json = serde_json::to_string_pretty(report)
        .map_err(|error| format!("Failed to serialize the {report_label}: {error}"))?;
    fs::write(report_path, report_json).map_err(|error| {
        format!(
            "Failed to write the {report_label} {}: {error}",
            report_path.display()
        )
    })
}

fn collect_runtime_info() -> RuntimeInfo {
    let node_path = env::var_os("MARKNEST_NODE_PATH").unwrap_or_else(|| "node".into());
    let node_path_text = node_path.to_string_lossy().into_owned();
    let browser_path = resolve_browser_path().ok();

    RuntimeInfo {
        renderer: "playwright-chromium",
        marknest_version: env!("CARGO_PKG_VERSION"),
        asset_mode: RUNTIME_ASSET_MODE,
        node_path: node_path_text.clone(),
        node_version: command_version(&node_path_text, "--version"),
        browser_path: browser_path.as_ref().map(|path| path.display().to_string()),
        browser_version: browser_path
            .as_ref()
            .and_then(|path| command_version(path.to_string_lossy().as_ref(), "--version")),
        playwright_version: PLAYWRIGHT_VERSION,
        mermaid_version: MERMAID_VERSION,
        mathjax_version: MATHJAX_VERSION,
        mermaid_script_url: MERMAID_SCRIPT_URL,
        math_script_url: MATHJAX_SCRIPT_URL,
    }
}

fn runtime_assets_for_html(html: &str) -> Vec<(&'static str, &'static [u8])> {
    let mut assets: Vec<(&'static str, &'static [u8])> = Vec::new();

    if html.contains(MERMAID_SCRIPT_URL) {
        assets.push((
            "runtime-assets/mermaid/mermaid.min.js",
            MERMAID_RUNTIME_ASSET_BYTES,
        ));
    }
    if html.contains(MATHJAX_SCRIPT_URL) {
        assets.push((
            "runtime-assets/mathjax/es5/tex-svg.js",
            MATHJAX_RUNTIME_ASSET_BYTES,
        ));
    }

    assets
}

fn write_runtime_assets_for_html(target_root: &Path, html: &str) -> Result<(), String> {
    for (relative_path, bytes) in runtime_assets_for_html(html) {
        let output_path = target_root.join(relative_path);
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "Failed to create the runtime asset directory {}: {error}",
                    parent.display()
                )
            })?;
        }

        fs::write(&output_path, bytes).map_err(|error| {
            format!(
                "Failed to write the runtime asset {}: {error}",
                output_path.display()
            )
        })?;
    }

    Ok(())
}

fn write_debug_runtime_assets(debug_html_path: &Path, html: &str) -> Result<(), AppFailure> {
    let parent_directory = debug_html_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    write_runtime_assets_for_html(parent_directory, html).map_err(|error| {
        AppFailure::system(format!("Failed to write bundled runtime assets: {error}"))
    })
}

fn materialize_remote_assets_for_entry(
    html: Option<&str>,
    assets: &[AssetRef],
    apply_mode: RemoteAssetApplyMode,
) -> Result<RemoteAssetMaterialization, String> {
    let mut cached_results: BTreeMap<String, Result<String, String>> = BTreeMap::new();
    let mut replacements: Vec<(String, String)> = Vec::new();
    let mut remote_assets: Vec<RemoteAssetReport> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();
    let mut total_downloaded_bytes: usize = 0;

    for asset in assets {
        let Some(fetch_url) = asset.fetch_url.as_ref() else {
            continue;
        };

        let data_uri_result = if let Some(result) = cached_results.get(fetch_url) {
            result.clone()
        } else {
            let result = fetch_remote_asset_data_uri(fetch_url, &mut total_downloaded_bytes);
            cached_results.insert(fetch_url.clone(), result.clone());
            result
        };

        match data_uri_result {
            Ok(data_uri) => {
                if matches!(apply_mode, RemoteAssetApplyMode::InlineHtml) {
                    replacements.push((asset.original_reference.clone(), data_uri));
                    remote_assets.push(RemoteAssetReport {
                        original_reference: asset.original_reference.clone(),
                        fetch_url: fetch_url.clone(),
                        status: RemoteAssetStatus::Inlined,
                        message: None,
                    });
                } else {
                    remote_assets.push(RemoteAssetReport {
                        original_reference: asset.original_reference.clone(),
                        fetch_url: fetch_url.clone(),
                        status: RemoteAssetStatus::LeftExternal,
                        message: None,
                    });
                }
            }
            Err(message) => {
                warnings.push(format!(
                    "Remote asset could not be materialized: {} -> {} ({message})",
                    asset.entry_path, asset.original_reference
                ));
                remote_assets.push(RemoteAssetReport {
                    original_reference: asset.original_reference.clone(),
                    fetch_url: fetch_url.clone(),
                    status: RemoteAssetStatus::Failed,
                    message: Some(message),
                });
            }
        }
    }

    Ok(RemoteAssetMaterialization {
        html: html.map(|value| rewrite_html_img_sources(value, &replacements)),
        remote_assets,
        warnings,
    })
}

pub fn materialize_remote_assets_for_html(
    html: &str,
    assets: &[AssetRef],
) -> Result<RemoteHtmlResult, String> {
    let materialization =
        materialize_remote_assets_for_entry(Some(html), assets, RemoteAssetApplyMode::InlineHtml)?;
    Ok(RemoteHtmlResult {
        html: materialization.html.unwrap_or_else(|| html.to_string()),
        warnings: materialization.warnings,
    })
}

fn fetch_remote_asset_data_uri(
    fetch_url: &str,
    total_downloaded_bytes: &mut usize,
) -> Result<String, String> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(REMOTE_ASSET_TIMEOUT_SECONDS))
        .timeout_read(Duration::from_secs(REMOTE_ASSET_TIMEOUT_SECONDS))
        .timeout_write(Duration::from_secs(REMOTE_ASSET_TIMEOUT_SECONDS))
        .redirects(REMOTE_ASSET_MAX_REDIRECTS)
        .build();
    let response = agent
        .get(fetch_url)
        .call()
        .map_err(map_remote_fetch_error)?;

    if let Some(content_length) = response.header("Content-Length") {
        if let Ok(content_length) = content_length.parse::<usize>() {
            if content_length > REMOTE_ASSET_MAX_BYTES {
                return Err(format!(
                    "response size {content_length} bytes exceeds the per-asset limit of {REMOTE_ASSET_MAX_BYTES} bytes"
                ));
            }
            if content_length > REMOTE_ASSET_MAX_TOTAL_BYTES.saturating_sub(*total_downloaded_bytes)
            {
                return Err(format!(
                    "response size {content_length} bytes exceeds the remaining per-entry remote asset budget"
                ));
            }
        }
    }

    let response_content_type = response
        .header("Content-Type")
        .map(normalize_content_type_header);
    let final_url = response.get_url().to_string();
    let mut reader = response.into_reader();
    let bytes = read_remote_asset_body_limited(&mut reader)?;

    if bytes.len() > REMOTE_ASSET_MAX_TOTAL_BYTES.saturating_sub(*total_downloaded_bytes) {
        return Err(format!(
            "response size {} bytes exceeds the remaining per-entry remote asset budget",
            bytes.len()
        ));
    }
    *total_downloaded_bytes += bytes.len();

    let mime_type = if let Some(content_type) = response_content_type {
        if content_type.starts_with("image/") {
            content_type
        } else if content_type == "application/octet-stream" {
            infer_remote_mime_type(&final_url)
                .or_else(|| infer_remote_mime_type(fetch_url))
                .or_else(|| infer_svg_mime_type(&bytes))
                .ok_or_else(|| {
                    format!("response content type {content_type} is not a supported image")
                })?
                .to_string()
        } else {
            infer_remote_mime_type(&final_url)
                .or_else(|| infer_remote_mime_type(fetch_url))
                .or_else(|| infer_svg_mime_type(&bytes))
                .ok_or_else(|| {
                    format!("response content type {content_type} is not a supported image")
                })?
                .to_string()
        }
    } else {
        infer_remote_mime_type(&final_url)
            .or_else(|| infer_remote_mime_type(fetch_url))
            .or_else(|| infer_svg_mime_type(&bytes))
            .ok_or_else(|| "response did not declare a supported image type".to_string())?
            .to_string()
    };

    Ok(format!(
        "data:{mime_type};base64,{}",
        encode_base64_bytes(&bytes)
    ))
}

fn map_remote_fetch_error(error: ureq::Error) -> String {
    match error {
        ureq::Error::Status(status_code, response) => {
            format!("HTTP {status_code} from {}", response.get_url())
        }
        ureq::Error::Transport(transport) => transport.to_string(),
    }
}

fn read_remote_asset_body_limited(reader: &mut dyn Read) -> Result<Vec<u8>, String> {
    let mut bytes: Vec<u8> = Vec::new();
    let mut buffer = [0_u8; 8192];

    loop {
        let bytes_read = reader
            .read(&mut buffer)
            .map_err(|error| format!("failed to read the response body: {error}"))?;
        if bytes_read == 0 {
            break;
        }

        if bytes.len() + bytes_read > REMOTE_ASSET_MAX_BYTES {
            return Err(format!(
                "response size exceeds the per-asset limit of {REMOTE_ASSET_MAX_BYTES} bytes"
            ));
        }

        bytes.extend_from_slice(&buffer[..bytes_read]);
    }

    Ok(bytes)
}

fn normalize_content_type_header(value: &str) -> String {
    value
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
}

fn infer_remote_mime_type(url: &str) -> Option<&'static str> {
    let stripped_url = url
        .split('#')
        .next()
        .unwrap_or(url)
        .split('?')
        .next()
        .unwrap_or(url)
        .to_ascii_lowercase();

    if stripped_url.ends_with(".png") {
        Some("image/png")
    } else if stripped_url.ends_with(".jpg") || stripped_url.ends_with(".jpeg") {
        Some("image/jpeg")
    } else if stripped_url.ends_with(".gif") {
        Some("image/gif")
    } else if stripped_url.ends_with(".svg") {
        Some("image/svg+xml")
    } else if stripped_url.ends_with(".webp") {
        Some("image/webp")
    } else if stripped_url.ends_with(".bmp") {
        Some("image/bmp")
    } else if stripped_url.ends_with(".avif") {
        Some("image/avif")
    } else {
        None
    }
}

fn infer_svg_mime_type(bytes: &[u8]) -> Option<&'static str> {
    let text = String::from_utf8_lossy(bytes);
    if text.contains("<svg") {
        Some("image/svg+xml")
    } else {
        None
    }
}

fn encode_base64_bytes(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut encoded = String::with_capacity(bytes.len().div_ceil(3) * 4);
    let mut index = 0;

    while index + 3 <= bytes.len() {
        let chunk = &bytes[index..index + 3];
        encoded.push(ALPHABET[(chunk[0] >> 2) as usize] as char);
        encoded
            .push(ALPHABET[(((chunk[0] & 0b0000_0011) << 4) | (chunk[1] >> 4)) as usize] as char);
        encoded
            .push(ALPHABET[(((chunk[1] & 0b0000_1111) << 2) | (chunk[2] >> 6)) as usize] as char);
        encoded.push(ALPHABET[(chunk[2] & 0b0011_1111) as usize] as char);
        index += 3;
    }

    match bytes.len() - index {
        1 => {
            let byte = bytes[index];
            encoded.push(ALPHABET[(byte >> 2) as usize] as char);
            encoded.push(ALPHABET[((byte & 0b0000_0011) << 4) as usize] as char);
            encoded.push('=');
            encoded.push('=');
        }
        2 => {
            let first = bytes[index];
            let second = bytes[index + 1];
            encoded.push(ALPHABET[(first >> 2) as usize] as char);
            encoded.push(ALPHABET[(((first & 0b0000_0011) << 4) | (second >> 4)) as usize] as char);
            encoded.push(ALPHABET[((second & 0b0000_1111) << 2) as usize] as char);
            encoded.push('=');
        }
        _ => {}
    }

    encoded
}

fn command_version(program: &str, argument: &str) -> Option<String> {
    let output = Command::new(program).arg(argument).output().ok()?;
    if !output.status.success() {
        return None;
    }

    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text.is_empty() { None } else { Some(text) }
}

fn build_convert_report(
    analyzed_input: &AnalyzedInput,
    selection: &SelectionDecision,
    convert_mode: ConvertMode,
) -> ConvertReport {
    ConvertReport {
        status: ConvertStatus::Success,
        exit_code: EXIT_SUCCESS,
        input_kind: analyzed_input.input_kind,
        input_path: analyzed_input.input_path.clone(),
        source_kind: analyzed_input.project_index.source_kind.clone(),
        selection_mode: selection.mode,
        requested_entry: selection.requested_entry.clone(),
        selected_entries: selection.selected_entries.clone(),
        entry_selection_reason: analyzed_input.project_index.entry_selection_reason.clone(),
        convert_mode,
        outputs: Vec::new(),
        failures: Vec::new(),
        collisions: Vec::new(),
        remote_assets: Vec::new(),
        warnings: Vec::new(),
        errors: Vec::new(),
        runtime_info: collect_runtime_info(),
    }
}

fn build_single_convert_report(
    analyzed_input: &AnalyzedInput,
    selection: &SelectionDecision,
    converted_entry: &EntryConvertSuccess,
) -> ConvertReport {
    let mut report = build_convert_report(analyzed_input, selection, ConvertMode::Single);
    report.outputs.push(ConvertOutputReport {
        entry_path: converted_entry.entry_path.clone(),
        output_path: converted_entry.output_path.display().to_string(),
        warnings: converted_entry.warnings.clone(),
    });
    report.remote_assets = converted_entry.remote_assets.clone();
    report.warnings = converted_entry.warnings.clone();
    finalize_convert_report(&mut report);
    report
}

fn finalize_convert_report(report: &mut ConvertReport) {
    let has_system_failure = report
        .failures
        .iter()
        .any(|failure| matches!(failure.kind, FailureKind::System));
    let has_validation_failure = !report.errors.is_empty()
        || !report.collisions.is_empty()
        || report
            .failures
            .iter()
            .any(|failure| matches!(failure.kind, FailureKind::Validation));

    if has_system_failure {
        report.status = ConvertStatus::Failure;
        report.exit_code = EXIT_SYSTEM_FAILURE;
    } else if has_validation_failure {
        report.status = ConvertStatus::Failure;
        report.exit_code = EXIT_VALIDATION_FAILURE;
    } else if !report.warnings.is_empty() {
        report.status = ConvertStatus::Warning;
        report.exit_code = EXIT_WARNING;
    } else {
        report.status = ConvertStatus::Success;
        report.exit_code = EXIT_SUCCESS;
    }
}

fn render_console_report(report: &ValidationReport, report_path: Option<&Path>) -> String {
    let mut lines: Vec<String> = Vec::new();

    match report.status {
        ValidationStatus::Success => lines.push("Validation succeeded.".to_string()),
        ValidationStatus::Warning => lines.push("Validation completed with warnings.".to_string()),
        ValidationStatus::Failure => lines.push("Validation failed.".to_string()),
    }

    if report.selected_entries.is_empty() {
        lines.push("Selected entries: (none)".to_string());
    } else {
        lines.push(format!(
            "Selected entries: {}",
            report.selected_entries.join(", ")
        ));
    }

    if !report.warnings.is_empty() {
        lines.push("Warnings:".to_string());
        lines.extend(report.warnings.iter().map(|warning| format!("- {warning}")));
    }

    if !report.errors.is_empty() {
        lines.push("Errors:".to_string());
        lines.extend(report.errors.iter().map(|error| format!("- {error}")));
    }

    if let Some(report_path) = report_path {
        lines.push(format!("Report: {}", report_path.display()));
    }

    let mut output = lines.join("\n");
    output.push('\n');
    output
}

fn render_batch_convert_console_output(
    report: &ConvertReport,
    report_path: Option<&Path>,
) -> String {
    let mut lines: Vec<String> = Vec::new();

    match report.status {
        ConvertStatus::Success => lines.push("Batch conversion succeeded.".to_string()),
        ConvertStatus::Warning => {
            lines.push("Batch conversion completed with warnings.".to_string())
        }
        ConvertStatus::Failure => lines.push("Batch conversion failed.".to_string()),
    }

    lines.push(format!("Converted: {}", report.outputs.len()));
    lines.push(format!("Failed: {}", report.failures.len()));

    if !report.collisions.is_empty() {
        lines.push("Collisions:".to_string());
        lines.extend(report.collisions.iter().map(|collision| {
            format!(
                "- {} <= {}",
                collision.output_path,
                collision.entry_paths.join(", ")
            )
        }));
    }

    if !report.warnings.is_empty() {
        lines.push("Warnings:".to_string());
        lines.extend(report.warnings.iter().map(|warning| format!("- {warning}")));
    }

    if !report.errors.is_empty() {
        lines.push("Errors:".to_string());
        lines.extend(report.errors.iter().map(|error| format!("- {error}")));
    }

    if let Some(report_path) = report_path {
        lines.push(format!("Report: {}", report_path.display()));
    }

    let mut output = lines.join("\n");
    output.push('\n');
    output
}

fn render_convert_console_output(
    selected_entry: &str,
    output_path: &Path,
    warnings: &[String],
) -> String {
    let mut lines: Vec<String> = Vec::new();

    if warnings.is_empty() {
        lines.push("Conversion succeeded.".to_string());
    } else {
        lines.push("Conversion completed with warnings.".to_string());
    }

    lines.push(format!("Entry: {selected_entry}"));
    lines.push(format!("Output: {}", output_path.display()));

    if !warnings.is_empty() {
        lines.push("Warnings:".to_string());
        lines.extend(warnings.iter().map(|warning| format!("- {warning}")));
    }

    let mut output = lines.join("\n");
    output.push('\n');
    output
}

fn normalize_path(path: &Path) -> Result<String, ()> {
    let mut segments: Vec<String> = Vec::new();

    for component in path.components() {
        match component {
            Component::Normal(value) => {
                segments.push(value.to_string_lossy().replace('\\', "/"));
            }
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return Err(()),
        }
    }

    if segments.is_empty() {
        return Err(());
    }

    Ok(segments.join("/"))
}

fn normalize_relative_string(path: &str) -> Result<String, ()> {
    let trimmed_path = path.trim();
    if trimmed_path.is_empty() {
        return Err(());
    }

    if trimmed_path.starts_with('/') || trimmed_path.starts_with('\\') {
        return Err(());
    }

    if has_windows_drive_prefix(trimmed_path) {
        return Err(());
    }

    let normalized_input = trimmed_path.replace('\\', "/");
    let mut segments: Vec<&str> = Vec::new();
    for segment in normalized_input.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                if segments.pop().is_none() {
                    return Err(());
                }
            }
            _ => segments.push(segment),
        }
    }

    if segments.is_empty() {
        return Err(());
    }

    Ok(segments.join("/"))
}

fn normalized_path_to_filesystem_path(root: &Path, normalized_path: &str) -> PathBuf {
    let mut path = root.to_path_buf();
    for segment in normalized_path.split('/') {
        path.push(segment);
    }
    path
}

fn is_markdown_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| {
            extension.eq_ignore_ascii_case("md") || extension.eq_ignore_ascii_case("markdown")
        })
        .unwrap_or(false)
}

fn is_zip_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.eq_ignore_ascii_case("zip"))
        .unwrap_or(false)
}

fn has_windows_drive_prefix(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

fn parse_cli(argv: Vec<String>) -> Result<ParseResult, ParseFailure> {
    let binary_name = argv
        .first()
        .cloned()
        .unwrap_or_else(|| "marknest".to_string());

    if argv.len() == 1 {
        return Err(ParseFailure::new(format!(
            "A subcommand is required.\n\n{}",
            root_help(&binary_name)
        )));
    }

    match argv[1].as_str() {
        "-h" | "--help" | "help" => Ok(ParseResult::Help(root_help(&binary_name))),
        "convert" => parse_convert_args(&binary_name, &argv[2..]),
        "validate" => parse_validate_args(&binary_name, &argv[2..]),
        other => Err(ParseFailure::new(format!(
            "Unknown subcommand: {other}\n\n{}",
            root_help(&binary_name)
        ))),
    }
}

fn parse_validate_args(binary_name: &str, args: &[String]) -> Result<ParseResult, ParseFailure> {
    let mut validate_args = ValidateArgs::default();
    let mut index = 0;

    while index < args.len() {
        let argument = &args[index];
        match argument.as_str() {
            "-h" | "--help" => {
                return Ok(ParseResult::Help(validate_help(binary_name)));
            }
            "--entry" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| ParseFailure::new("Missing value for --entry.".to_string()))?;
                validate_args.entry = Some(value.clone());
            }
            "--report" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| ParseFailure::new("Missing value for --report.".to_string()))?;
                validate_args.report = Some(PathBuf::from(value));
            }
            "--all" => {
                validate_args.all = true;
            }
            "--strict" => {
                validate_args.strict = true;
            }
            _ if argument.starts_with('-') => {
                return Err(ParseFailure::new(format!("Unknown option: {argument}")));
            }
            _ => {
                if validate_args.input.is_some() {
                    return Err(ParseFailure::new(
                        "Only one input path may be provided.".to_string(),
                    ));
                }
                validate_args.input = Some(PathBuf::from(argument));
            }
        }

        index += 1;
    }

    if validate_args.all && validate_args.entry.is_some() {
        return Err(ParseFailure::new(
            "--entry cannot be used together with --all.".to_string(),
        ));
    }

    Ok(ParseResult::Validate(validate_args))
}

fn parse_convert_args(binary_name: &str, args: &[String]) -> Result<ParseResult, ParseFailure> {
    let mut convert_args = ConvertCliArgs::default();
    let mut index = 0;

    while index < args.len() {
        let argument = &args[index];
        match argument.as_str() {
            "-h" | "--help" => return Ok(ParseResult::Help(convert_help(binary_name))),
            "--entry" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| ParseFailure::new("Missing value for --entry.".to_string()))?;
                convert_args.entry = Some(value.clone());
            }
            "--all" => {
                convert_args.all = true;
            }
            "-o" | "--output" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| ParseFailure::new("Missing value for --output.".to_string()))?;
                convert_args.output = Some(PathBuf::from(value));
            }
            "--out-dir" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| ParseFailure::new("Missing value for --out-dir.".to_string()))?;
                convert_args.out_dir = Some(PathBuf::from(value));
            }
            "--render-report" => {
                index += 1;
                let value = args.get(index).ok_or_else(|| {
                    ParseFailure::new("Missing value for --render-report.".to_string())
                })?;
                convert_args.render_report = Some(PathBuf::from(value));
            }
            "--config" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| ParseFailure::new("Missing value for --config.".to_string()))?;
                convert_args.config = Some(PathBuf::from(value));
            }
            "--debug-html" => {
                index += 1;
                let value = args.get(index).ok_or_else(|| {
                    ParseFailure::new("Missing value for --debug-html.".to_string())
                })?;
                convert_args.debug_html = Some(PathBuf::from(value));
            }
            "--asset-manifest" => {
                index += 1;
                let value = args.get(index).ok_or_else(|| {
                    ParseFailure::new("Missing value for --asset-manifest.".to_string())
                })?;
                convert_args.asset_manifest = Some(PathBuf::from(value));
            }
            "--css" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| ParseFailure::new("Missing value for --css.".to_string()))?;
                convert_args.css = Some(PathBuf::from(value));
            }
            "--header-template" => {
                index += 1;
                let value = args.get(index).ok_or_else(|| {
                    ParseFailure::new("Missing value for --header-template.".to_string())
                })?;
                convert_args.header_template = Some(PathBuf::from(value));
            }
            "--footer-template" => {
                index += 1;
                let value = args.get(index).ok_or_else(|| {
                    ParseFailure::new("Missing value for --footer-template.".to_string())
                })?;
                convert_args.footer_template = Some(PathBuf::from(value));
            }
            "--page-size" => {
                index += 1;
                let value = args.get(index).ok_or_else(|| {
                    ParseFailure::new("Missing value for --page-size.".to_string())
                })?;
                convert_args.page_size = Some(PdfPageSize::parse(value)?);
            }
            "--margin" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| ParseFailure::new("Missing value for --margin.".to_string()))?;
                convert_args.margin_mm = Some(parse_margin_mm(value)?);
            }
            "--margin-top" => {
                index += 1;
                let value = args.get(index).ok_or_else(|| {
                    ParseFailure::new("Missing value for --margin-top.".to_string())
                })?;
                convert_args.margin_top_mm = Some(parse_margin_mm(value)?);
            }
            "--margin-right" => {
                index += 1;
                let value = args.get(index).ok_or_else(|| {
                    ParseFailure::new("Missing value for --margin-right.".to_string())
                })?;
                convert_args.margin_right_mm = Some(parse_margin_mm(value)?);
            }
            "--margin-bottom" => {
                index += 1;
                let value = args.get(index).ok_or_else(|| {
                    ParseFailure::new("Missing value for --margin-bottom.".to_string())
                })?;
                convert_args.margin_bottom_mm = Some(parse_margin_mm(value)?);
            }
            "--margin-left" => {
                index += 1;
                let value = args.get(index).ok_or_else(|| {
                    ParseFailure::new("Missing value for --margin-left.".to_string())
                })?;
                convert_args.margin_left_mm = Some(parse_margin_mm(value)?);
            }
            "--theme" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| ParseFailure::new("Missing value for --theme.".to_string()))?;
                convert_args.theme = Some(parse_theme_preset(value)?);
            }
            "--landscape" => {
                convert_args.landscape = Some(true);
            }
            "--toc" => {
                convert_args.enable_toc = Some(true);
            }
            "--no-toc" => {
                convert_args.enable_toc = Some(false);
            }
            "--sanitize-html" => {
                convert_args.sanitize_html = Some(true);
            }
            "--no-sanitize-html" => {
                convert_args.sanitize_html = Some(false);
            }
            "--title" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| ParseFailure::new("Missing value for --title.".to_string()))?;
                convert_args.metadata_title = Some(value.clone());
            }
            "--author" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| ParseFailure::new("Missing value for --author.".to_string()))?;
                convert_args.metadata_author = Some(value.clone());
            }
            "--subject" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| ParseFailure::new("Missing value for --subject.".to_string()))?;
                convert_args.metadata_subject = Some(value.clone());
            }
            "--mermaid" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| ParseFailure::new("Missing value for --mermaid.".to_string()))?;
                convert_args.mermaid_mode = Some(parse_mermaid_mode(value)?);
            }
            "--math" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| ParseFailure::new("Missing value for --math.".to_string()))?;
                convert_args.math_mode = Some(parse_math_mode(value)?);
            }
            "--mermaid-timeout-ms" => {
                index += 1;
                let value = args.get(index).ok_or_else(|| {
                    ParseFailure::new("Missing value for --mermaid-timeout-ms.".to_string())
                })?;
                convert_args.mermaid_timeout_ms = Some(parse_timeout_ms(value, "Mermaid timeout")?);
            }
            "--math-timeout-ms" => {
                index += 1;
                let value = args.get(index).ok_or_else(|| {
                    ParseFailure::new("Missing value for --math-timeout-ms.".to_string())
                })?;
                convert_args.math_timeout_ms = Some(parse_timeout_ms(value, "Math timeout")?);
            }
            _ if argument.starts_with('-') => {
                return Err(ParseFailure::new(format!("Unknown option: {argument}")));
            }
            _ => {
                if convert_args.input.is_some() {
                    return Err(ParseFailure::new(
                        "Only one input path may be provided.".to_string(),
                    ));
                }
                convert_args.input = Some(PathBuf::from(argument));
            }
        }

        index += 1;
    }

    if convert_args.all && convert_args.entry.is_some() {
        return Err(ParseFailure::new(
            "--entry cannot be used together with --all.".to_string(),
        ));
    }

    Ok(ParseResult::Convert(convert_args))
}

fn root_help(binary_name: &str) -> String {
    format!(
        "Convert and validate Markdown workspaces.\n\nUsage:\n  {binary_name} convert [INPUT] [--entry <PATH> | --all] [-o <PATH> | --out-dir <PATH>] [--config <PATH>] [--render-report <PATH>] [--debug-html <PATH>] [--asset-manifest <PATH>] [--css <PATH>] [--header-template <PATH>] [--footer-template <PATH>] [--page-size <a4|letter>] [--margin <MM>] [--margin-top <MM>] [--margin-right <MM>] [--margin-bottom <MM>] [--margin-left <MM>] [--theme <default|github|docs|plain>] [--landscape] [--toc | --no-toc] [--sanitize-html | --no-sanitize-html] [--title <TEXT>] [--author <TEXT>] [--subject <TEXT>] [--mermaid <off|auto|on>] [--math <off|auto|on>] [--mermaid-timeout-ms <MS>] [--math-timeout-ms <MS>]\n  {binary_name} validate [INPUT] [--entry <PATH> | --all] [--strict] [--report <PATH>]\n  {binary_name} --help\n\nINPUT can be a Markdown file, ZIP archive, folder, or GitHub URL.\n\nGitHub URL examples:\n  {binary_name} convert https://github.com/user/repo -o output.pdf\n  {binary_name} convert https://github.com/user/repo/blob/main/guide.md -o guide.pdf\n  {binary_name} convert https://github.com/user/repo --all --out-dir ./pdf\n\nEnvironment:\n  GITHUB_TOKEN / GH_TOKEN    GitHub auth token for private repos and higher rate limits\n"
    )
}

fn validate_help(binary_name: &str) -> String {
    format!(
        "Validate Markdown workspaces and ZIP inputs.\n\nUsage:\n  {binary_name} validate [INPUT] [OPTIONS]\n\nINPUT can be a Markdown file, ZIP archive, folder, or GitHub URL.\n\nOptions:\n  --entry <PATH>   Validate a single Markdown entry inside a folder or ZIP input.\n  --all            Validate all Markdown entries.\n  --strict         Treat warnings as validation failures.\n  --report <PATH>  Write a JSON validation report.\n  -h, --help       Show this help message.\n\nEnvironment:\n  GITHUB_TOKEN / GH_TOKEN    GitHub auth token for private repos and higher rate limits\n"
    )
}

fn convert_help(binary_name: &str) -> String {
    format!(
        "Convert Markdown entries into PDF files.\n\nUsage:\n  {binary_name} convert [INPUT] [OPTIONS]\n\nINPUT can be a Markdown file, ZIP archive, folder, or GitHub URL.\n\nGitHub URL examples:\n  {binary_name} convert https://github.com/user/repo -o output.pdf\n  {binary_name} convert https://github.com/user/repo/blob/main/guide.md -o guide.pdf\n  {binary_name} convert https://github.com/user/repo --all --out-dir ./pdf\n\nOptions:\n  --entry <PATH>               Convert one Markdown entry inside a folder or ZIP input.\n  --all                        Convert all Markdown entries.\n  -o, --output <PATH>          Write a single PDF to a specific path.\n  --out-dir <PATH>             Write batch PDF output under a directory.\n  --config <PATH>              Load conversion defaults from a TOML config file.\n  --render-report <PATH>       Write a JSON conversion report.\n  --debug-html <PATH>          Write the rendered HTML used for PDF generation.\n  --asset-manifest <PATH>      Write the selected entry asset manifest as JSON.\n  --css <PATH>                 Append a custom CSS file after the theme stylesheet.\n  --header-template <PATH>     Load an HTML header template for Chromium print output.\n  --footer-template <PATH>     Load an HTML footer template for Chromium print output.\n  --page-size <a4|letter>      Set the output page size.\n  --margin <MM>                Set the same margin on all sides in millimeters.\n  --margin-top <MM>            Override the top page margin in millimeters.\n  --margin-right <MM>          Override the right page margin in millimeters.\n  --margin-bottom <MM>         Override the bottom page margin in millimeters.\n  --margin-left <MM>           Override the left page margin in millimeters.\n  --theme <default|github|docs|plain>\n                               Apply a built-in document theme.\n  --landscape                  Render the PDF in landscape orientation.\n  --toc                        Insert a generated table of contents near the top of the document.\n  --no-toc                     Skip the generated table of contents.\n  --sanitize-html              Sanitize rendered document HTML before PDF generation.\n  --no-sanitize-html           Trust document HTML and skip sanitization.\n  --title <TEXT>               Override the document title.\n  --author <TEXT>              Set the PDF author metadata.\n  --subject <TEXT>             Set the PDF subject metadata.\n  --mermaid <off|auto|on>      Control Mermaid rendering.\n  --math <off|auto|on>         Control Math rendering.\n  --mermaid-timeout-ms <MS>    Set the per-diagram Mermaid render timeout.\n  --math-timeout-ms <MS>       Set the per-expression Math render timeout.\n  -h, --help                   Show this help message.\n\nEnvironment:\n  GITHUB_TOKEN / GH_TOKEN    GitHub auth token for private repos and higher rate limits\n"
    )
}

#[derive(Debug, Default)]
struct ValidateArgs {
    input: Option<PathBuf>,
    entry: Option<String>,
    all: bool,
    strict: bool,
    report: Option<PathBuf>,
}

#[derive(Debug, Clone, Default)]
struct ConvertCliArgs {
    input: Option<PathBuf>,
    entry: Option<String>,
    all: bool,
    output: Option<PathBuf>,
    out_dir: Option<PathBuf>,
    render_report: Option<PathBuf>,
    config: Option<PathBuf>,
    debug_html: Option<PathBuf>,
    asset_manifest: Option<PathBuf>,
    css: Option<PathBuf>,
    header_template: Option<PathBuf>,
    footer_template: Option<PathBuf>,
    page_size: Option<PdfPageSize>,
    margin_mm: Option<f64>,
    margin_top_mm: Option<f64>,
    margin_right_mm: Option<f64>,
    margin_bottom_mm: Option<f64>,
    margin_left_mm: Option<f64>,
    theme: Option<ThemePreset>,
    landscape: Option<bool>,
    enable_toc: Option<bool>,
    sanitize_html: Option<bool>,
    metadata_title: Option<String>,
    metadata_author: Option<String>,
    metadata_subject: Option<String>,
    mermaid_mode: Option<MermaidMode>,
    math_mode: Option<MathMode>,
    mermaid_timeout_ms: Option<u32>,
    math_timeout_ms: Option<u32>,
}

#[derive(Debug, Clone)]
struct ConvertArgs {
    input: Option<PathBuf>,
    entry: Option<String>,
    all: bool,
    output: Option<PathBuf>,
    out_dir: Option<PathBuf>,
    render_report: Option<PathBuf>,
    debug_html: Option<PathBuf>,
    asset_manifest: Option<PathBuf>,
    css_path: Option<PathBuf>,
    header_template_path: Option<PathBuf>,
    footer_template_path: Option<PathBuf>,
    page_size: PdfPageSize,
    margins_mm: PdfMarginsMm,
    theme: ThemePreset,
    landscape: bool,
    enable_toc: bool,
    sanitize_html: bool,
    metadata: PdfMetadata,
    mermaid_mode: MermaidMode,
    math_mode: MathMode,
    mermaid_timeout_ms: u32,
    math_timeout_ms: u32,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct MarknestConfigFile {
    convert: Option<ConvertConfigFile>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct ConvertConfigFile {
    page_size: Option<String>,
    margin: Option<f64>,
    margin_top: Option<f64>,
    margin_right: Option<f64>,
    margin_bottom: Option<f64>,
    margin_left: Option<f64>,
    theme: Option<String>,
    landscape: Option<bool>,
    toc: Option<bool>,
    sanitize_html: Option<bool>,
    title: Option<String>,
    author: Option<String>,
    subject: Option<String>,
    mermaid: Option<String>,
    math: Option<String>,
    mermaid_timeout_ms: Option<u32>,
    math_timeout_ms: Option<u32>,
    css: Option<String>,
    header_template: Option<String>,
    footer_template: Option<String>,
    debug_html: Option<String>,
    asset_manifest: Option<String>,
    render_report: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct ConvertEnvironment {
    config_path: Option<PathBuf>,
    theme: Option<String>,
    css_path: Option<PathBuf>,
    enable_toc: Option<String>,
    sanitize_html: Option<String>,
    mermaid_timeout_ms: Option<String>,
    math_timeout_ms: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct LoadedRenderSupportFiles {
    custom_css: Option<String>,
    header_template_source: Option<String>,
    footer_template_source: Option<String>,
}

#[derive(Debug)]
enum ResolvedInput {
    MarkdownFile {
        path: PathBuf,
        display_path: String,
    },
    Zip {
        path: PathBuf,
        display_path: String,
    },
    Folder {
        path: PathBuf,
        display_path: String,
        is_default_input: bool,
    },
    GitHubUrl {
        display_path: String,
        parsed: ParsedGitHubUrl,
    },
}

#[derive(Debug)]
struct AnalyzedInput {
    resolved_input_path: PathBuf,
    input_kind: ValidationInputKind,
    input_path: String,
    is_default_input: bool,
    uses_implicit_all: bool,
    explicit_entry: Option<String>,
    workspace_root: Option<PathBuf>,
    default_output_directory: Option<PathBuf>,
    project_index: ProjectIndex,
    /// Strip common prefix from ZIP paths during materialization.
    /// Enabled for GitHub archive downloads where files are nested under `{repo}-{ref}/`.
    strip_zip_prefix: bool,
    /// Keeps temporary directory alive for the duration of analysis/conversion.
    /// Used by GitHub URL downloads to hold the temp archive file.
    _temp_dir: Option<TempDir>,
}

#[derive(Debug, Clone)]
struct SelectionDecision {
    mode: SelectionMode,
    requested_entry: Option<String>,
    selected_entries: Vec<String>,
    errors: Vec<String>,
}

#[derive(Debug)]
struct FilteredDiagnostics {
    assets: Vec<AssetRef>,
    ignored_files: Vec<String>,
    missing_assets: Vec<String>,
    path_errors: Vec<String>,
    warnings: Vec<String>,
}

struct PreparedWorkspace {
    root: PathBuf,
    _temp_dir: Option<TempDir>,
}

struct EntryConvertSuccess {
    entry_path: String,
    output_path: PathBuf,
    warnings: Vec<String>,
    remote_assets: Vec<RemoteAssetReport>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum RemoteAssetStatus {
    Inlined,
    LeftExternal,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct RemoteAssetReport {
    original_reference: String,
    fetch_url: String,
    status: RemoteAssetStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RemoteAssetMaterialization {
    html: Option<String>,
    remote_assets: Vec<RemoteAssetReport>,
    warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteHtmlResult {
    pub html: String,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RemoteAssetApplyMode {
    InlineHtml,
    KeepExternal,
}

#[derive(Debug, Serialize)]
struct AssetManifest {
    entry_path: String,
    assets: Vec<AssetRef>,
    remote_assets: Vec<RemoteAssetReport>,
    missing_assets: Vec<String>,
    path_errors: Vec<String>,
    warnings: Vec<String>,
}

struct BatchOutputTarget {
    entry_path: String,
    output_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HtmlToPdfRequest {
    pub title: String,
    pub html: String,
    pub page_size: PdfPageSize,
    pub margins_mm: PdfMarginsMm,
    pub landscape: bool,
    pub metadata: PdfMetadata,
    pub header_template: Option<String>,
    pub footer_template: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HtmlToPdfResult {
    pub bytes: Vec<u8>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HtmlToPdfErrorKind {
    Validation,
    System,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HtmlToPdfError {
    pub kind: HtmlToPdfErrorKind,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq)]
struct PdfRenderRequest {
    title: String,
    html: String,
    output_path: PathBuf,
    page_size: PdfPageSize,
    margins_mm: PdfMarginsMm,
    landscape: bool,
    metadata: PdfMetadata,
    header_template: Option<String>,
    footer_template: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct PdfRenderOutcome {
    warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PdfRenderFailure {
    kind: FailureKind,
    message: String,
}

impl PdfRenderFailure {
    fn validation(message: String) -> Self {
        Self {
            kind: FailureKind::Validation,
            message,
        }
    }

    fn system(message: String) -> Self {
        Self {
            kind: FailureKind::System,
            message,
        }
    }
}

trait PdfRenderer {
    fn render(&self, request: &PdfRenderRequest) -> Result<PdfRenderOutcome, PdfRenderFailure>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PdfPageSize {
    A4,
    Letter,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize)]
pub struct PdfMarginsMm {
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
    pub left: f64,
}

impl Default for PdfMarginsMm {
    fn default() -> Self {
        Self::uniform(16.0)
    }
}

impl PdfMarginsMm {
    pub fn uniform(all_sides_mm: f64) -> Self {
        Self {
            top: all_sides_mm,
            right: all_sides_mm,
            bottom: all_sides_mm,
            left: all_sides_mm,
        }
    }
}

impl PdfPageSize {
    fn parse(value: &str) -> Result<Self, ParseFailure> {
        if value.eq_ignore_ascii_case("a4") {
            Ok(Self::A4)
        } else if value.eq_ignore_ascii_case("letter") {
            Ok(Self::Letter)
        } else {
            Err(ParseFailure::new(format!(
                "Unsupported page size: {value}. Use a4 or letter."
            )))
        }
    }

    fn playwright_format_name(self) -> &'static str {
        match self {
            Self::A4 => "A4",
            Self::Letter => "Letter",
        }
    }
}

struct NodeBrowserPdfRenderer;

#[derive(Debug)]
struct AppFailure {
    kind: FailureKind,
    message: String,
}

impl AppFailure {
    fn validation(message: String) -> Self {
        Self {
            kind: FailureKind::Validation,
            message,
        }
    }

    fn system(message: String) -> Self {
        Self {
            kind: FailureKind::System,
            message,
        }
    }

    fn exit_code(&self) -> i32 {
        match self.kind {
            FailureKind::Validation => EXIT_VALIDATION_FAILURE,
            FailureKind::System => EXIT_SYSTEM_FAILURE,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum FailureKind {
    Validation,
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum ValidationInputKind {
    MarkdownFile,
    Zip,
    Folder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum SelectionMode {
    Auto,
    Entry,
    All,
    ExplicitMarkdownFile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum ValidationStatus {
    Success,
    Warning,
    Failure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum ConvertMode {
    Single,
    Batch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum ConvertStatus {
    Success,
    Warning,
    Failure,
}

#[derive(Debug, Serialize)]
struct ValidationReport {
    status: ValidationStatus,
    exit_code: i32,
    input_kind: ValidationInputKind,
    input_path: String,
    strict: bool,
    source_kind: ProjectSourceKind,
    selection_mode: SelectionMode,
    requested_entry: Option<String>,
    selected_entries: Vec<String>,
    entry_selection_reason: EntrySelectionReason,
    entry_candidates: Vec<EntryCandidate>,
    assets: Vec<AssetRef>,
    remote_assets: Vec<RemoteAssetReport>,
    ignored_files: Vec<String>,
    missing_assets: Vec<String>,
    path_errors: Vec<String>,
    warnings: Vec<String>,
    errors: Vec<String>,
    runtime_info: RuntimeInfo,
}

#[derive(Debug, Serialize)]
struct ConvertReport {
    status: ConvertStatus,
    exit_code: i32,
    input_kind: ValidationInputKind,
    input_path: String,
    source_kind: ProjectSourceKind,
    selection_mode: SelectionMode,
    requested_entry: Option<String>,
    selected_entries: Vec<String>,
    entry_selection_reason: EntrySelectionReason,
    convert_mode: ConvertMode,
    outputs: Vec<ConvertOutputReport>,
    failures: Vec<ConvertFailureReport>,
    collisions: Vec<ConvertCollisionReport>,
    remote_assets: Vec<RemoteAssetReport>,
    warnings: Vec<String>,
    errors: Vec<String>,
    runtime_info: RuntimeInfo,
}

#[derive(Debug, Serialize)]
struct ConvertOutputReport {
    entry_path: String,
    output_path: String,
    warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ConvertFailureReport {
    entry_path: Option<String>,
    output_path: Option<String>,
    kind: FailureKind,
    message: String,
}

#[derive(Debug, Serialize)]
struct ConvertCollisionReport {
    output_path: String,
    entry_paths: Vec<String>,
}

#[derive(Debug, Serialize)]
struct RuntimeInfo {
    renderer: &'static str,
    marknest_version: &'static str,
    asset_mode: &'static str,
    node_path: String,
    node_version: Option<String>,
    browser_path: Option<String>,
    browser_version: Option<String>,
    playwright_version: &'static str,
    mermaid_version: &'static str,
    mathjax_version: &'static str,
    mermaid_script_url: &'static str,
    math_script_url: &'static str,
}

impl PdfRenderer for NodeBrowserPdfRenderer {
    fn render(&self, request: &PdfRenderRequest) -> Result<PdfRenderOutcome, PdfRenderFailure> {
        let node_path: OsString =
            env::var_os("MARKNEST_NODE_PATH").unwrap_or_else(|| "node".into());
        let browser_path: PathBuf = resolve_browser_path().map_err(PdfRenderFailure::system)?;
        let playwright_runtime_dir = resolve_playwright_runtime_dir();
        validate_playwright_runtime_dir(&playwright_runtime_dir)
            .map_err(PdfRenderFailure::system)?;
        let temp_dir = TempDir::new().map_err(|error| {
            PdfRenderFailure::system(format!(
                "Failed to create the PDF render temp directory: {error}"
            ))
        })?;
        let html_path = temp_dir.path().join("document.html");
        let script_path = temp_dir.path().join("playwright_print.js");
        let options_path = temp_dir.path().join("print-options.json");

        write_runtime_assets_for_html(temp_dir.path(), &request.html)
            .map_err(PdfRenderFailure::system)?;
        fs::write(&html_path, &request.html).map_err(|error| {
            PdfRenderFailure::system(format!(
                "Failed to write the temporary HTML document {}: {error}",
                html_path.display()
            ))
        })?;
        fs::write(&script_path, PLAYWRIGHT_PRINT_SCRIPT).map_err(|error| {
            PdfRenderFailure::system(format!(
                "Failed to write the browser helper script {}: {error}",
                script_path.display()
            ))
        })?;
        let print_options = serde_json::json!({
            "pageFormat": request.page_size.playwright_format_name(),
            "marginTopMm": request.margins_mm.top,
            "marginRightMm": request.margins_mm.right,
            "marginBottomMm": request.margins_mm.bottom,
            "marginLeftMm": request.margins_mm.left,
            "landscape": request.landscape,
            "headerTemplate": request.header_template.clone(),
            "footerTemplate": request.footer_template.clone(),
        });
        fs::write(
            &options_path,
            serde_json::to_string(&print_options).map_err(|error| {
                PdfRenderFailure::system(format!(
                    "Failed to serialize the Playwright print options: {error}"
                ))
            })?,
        )
        .map_err(|error| {
            PdfRenderFailure::system(format!(
                "Failed to write the Playwright print options {}: {error}",
                options_path.display()
            ))
        })?;
        let output = Command::new(node_path)
            .arg(script_path)
            .arg(browser_path)
            .arg(&html_path)
            .arg(&request.output_path)
            .arg(&options_path)
            .arg(&playwright_runtime_dir)
            .output()
            .map_err(|error| {
                PdfRenderFailure::system(format!(
                    "Failed to start the Node Playwright PDF renderer: {error}"
                ))
            })?;

        if output.status.success() {
            return Ok(render_outcome_from_output(&output));
        }

        Err(render_failure_from_output(&output))
    }
}

pub fn render_html_to_pdf_bytes(
    request: &HtmlToPdfRequest,
) -> Result<HtmlToPdfResult, HtmlToPdfError> {
    let renderer = NodeBrowserPdfRenderer;
    render_html_to_pdf_bytes_with_renderer(request, &renderer)
}

fn render_html_to_pdf_bytes_with_renderer(
    request: &HtmlToPdfRequest,
    renderer: &dyn PdfRenderer,
) -> Result<HtmlToPdfResult, HtmlToPdfError> {
    let temp_dir = TempDir::new().map_err(|error| HtmlToPdfError {
        kind: HtmlToPdfErrorKind::System,
        message: format!("Failed to create a temporary PDF output directory: {error}"),
    })?;
    let output_path = temp_dir.path().join("document.pdf");

    let render_outcome = renderer
        .render(&PdfRenderRequest {
            title: request.title.clone(),
            html: request.html.clone(),
            output_path: output_path.clone(),
            page_size: request.page_size,
            margins_mm: request.margins_mm,
            landscape: request.landscape,
            metadata: request.metadata.clone(),
            header_template: request.header_template.clone(),
            footer_template: request.footer_template.clone(),
        })
        .map_err(|failure| HtmlToPdfError {
            kind: match failure.kind {
                FailureKind::Validation => HtmlToPdfErrorKind::Validation,
                FailureKind::System => HtmlToPdfErrorKind::System,
            },
            message: failure.message,
        })?;

    apply_pdf_metadata(&output_path, &request.metadata).map_err(|error| HtmlToPdfError {
        kind: HtmlToPdfErrorKind::System,
        message: format!("Failed to update PDF metadata: {error}"),
    })?;

    let bytes = fs::read(&output_path).map_err(|error| HtmlToPdfError {
        kind: HtmlToPdfErrorKind::System,
        message: format!(
            "Failed to read the generated PDF {}: {error}",
            output_path.display()
        ),
    })?;

    Ok(HtmlToPdfResult {
        bytes,
        warnings: render_outcome.warnings,
    })
}

fn resolve_playwright_runtime_dir() -> PathBuf {
    env::var_os("MARKNEST_PLAYWRIGHT_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("playwright-runtime"))
}

fn validate_playwright_runtime_dir(runtime_dir: &Path) -> Result<(), String> {
    let package_json_path = runtime_dir.join("package.json");
    if !package_json_path.exists() {
        return Err(format!(
            "Playwright runtime package was not found at {}. Set MARKNEST_PLAYWRIGHT_RUNTIME_DIR or run `npm ci --prefix {}`.",
            runtime_dir.display(),
            runtime_dir.display()
        ));
    }

    let package_dependency_path = runtime_dir
        .join("node_modules")
        .join("playwright")
        .join("package.json");
    if !package_dependency_path.exists() {
        return Err(format!(
            "Playwright runtime dependencies are not installed at {}. Run `npm ci --prefix {}`.",
            runtime_dir.display(),
            runtime_dir.display()
        ));
    }

    Ok(())
}

fn browser_candidate_paths() -> Vec<PathBuf> {
    vec![
        PathBuf::from(r"C:\Program Files\Google\Chrome\Application\chrome.exe"),
        PathBuf::from(r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe"),
        PathBuf::from(r"C:\Program Files\Microsoft\Edge\Application\msedge.exe"),
        PathBuf::from(r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe"),
        PathBuf::from("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"),
        PathBuf::from("/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge"),
        PathBuf::from("/Applications/Chromium.app/Contents/MacOS/Chromium"),
        PathBuf::from("/usr/bin/chromium"),
        PathBuf::from("/usr/bin/chromium-browser"),
        PathBuf::from("/usr/bin/google-chrome"),
        PathBuf::from("/usr/bin/microsoft-edge"),
    ]
}

fn resolve_browser_path() -> Result<PathBuf, String> {
    if let Some(configured_path) = env::var_os("MARKNEST_BROWSER_PATH") {
        let configured_path = PathBuf::from(configured_path);
        if configured_path.exists() {
            return Ok(configured_path);
        }

        return Err(format!(
            "Configured browser path does not exist: {}",
            configured_path.display()
        ));
    }

    for candidate in browser_candidate_paths() {
        if candidate.exists() {
            return Ok(candidate);
        }
    }

    Err(
        "No supported browser executable was found. Set MARKNEST_BROWSER_PATH to Chrome, Edge, or Chromium."
            .to_string(),
    )
}

fn parse_margin_mm(value: &str) -> Result<f64, ParseFailure> {
    let margin_mm: f64 = value
        .parse()
        .map_err(|_| ParseFailure::new(format!("Invalid margin value: {value}")))?;
    if margin_mm < 0.0 {
        return Err(ParseFailure::new(
            "Margin must be zero or greater.".to_string(),
        ));
    }

    Ok(margin_mm)
}

fn resolve_pdf_margins(
    cli_args: &ConvertCliArgs,
    config: &ConvertConfigFile,
) -> Result<PdfMarginsMm, AppFailure> {
    if cli_args.margin_mm.is_some()
        || cli_args.margin_top_mm.is_some()
        || cli_args.margin_right_mm.is_some()
        || cli_args.margin_bottom_mm.is_some()
        || cli_args.margin_left_mm.is_some()
    {
        let base_margins = PdfMarginsMm::uniform(cli_args.margin_mm.unwrap_or(16.0));
        return Ok(PdfMarginsMm {
            top: cli_args.margin_top_mm.unwrap_or(base_margins.top),
            right: cli_args.margin_right_mm.unwrap_or(base_margins.right),
            bottom: cli_args.margin_bottom_mm.unwrap_or(base_margins.bottom),
            left: cli_args.margin_left_mm.unwrap_or(base_margins.left),
        });
    }

    if config.margin.is_some()
        || config.margin_top.is_some()
        || config.margin_right.is_some()
        || config.margin_bottom.is_some()
        || config.margin_left.is_some()
    {
        let base_margins = PdfMarginsMm::uniform(config.margin.unwrap_or(16.0));
        return Ok(PdfMarginsMm {
            top: config.margin_top.unwrap_or(base_margins.top),
            right: config.margin_right.unwrap_or(base_margins.right),
            bottom: config.margin_bottom.unwrap_or(base_margins.bottom),
            left: config.margin_left.unwrap_or(base_margins.left),
        });
    }

    Ok(PdfMarginsMm::default())
}

fn parse_theme_preset(value: &str) -> Result<ThemePreset, ParseFailure> {
    if value.eq_ignore_ascii_case("default") {
        Ok(ThemePreset::Default)
    } else if value.eq_ignore_ascii_case("github") {
        Ok(ThemePreset::Github)
    } else if value.eq_ignore_ascii_case("docs") {
        Ok(ThemePreset::Docs)
    } else if value.eq_ignore_ascii_case("plain") {
        Ok(ThemePreset::Plain)
    } else {
        Err(ParseFailure::new(format!(
            "Unsupported theme preset: {value}. Use default, github, docs, or plain."
        )))
    }
}

fn parse_mermaid_mode(value: &str) -> Result<MermaidMode, ParseFailure> {
    if value.eq_ignore_ascii_case("off") {
        Ok(MermaidMode::Off)
    } else if value.eq_ignore_ascii_case("auto") {
        Ok(MermaidMode::Auto)
    } else if value.eq_ignore_ascii_case("on") {
        Ok(MermaidMode::On)
    } else {
        Err(ParseFailure::new(format!(
            "Unsupported mermaid mode: {value}. Use off, auto, or on."
        )))
    }
}

fn parse_math_mode(value: &str) -> Result<MathMode, ParseFailure> {
    if value.eq_ignore_ascii_case("off") {
        Ok(MathMode::Off)
    } else if value.eq_ignore_ascii_case("auto") {
        Ok(MathMode::Auto)
    } else if value.eq_ignore_ascii_case("on") {
        Ok(MathMode::On)
    } else {
        Err(ParseFailure::new(format!(
            "Unsupported math mode: {value}. Use off, auto, or on."
        )))
    }
}

fn resolve_convert_args(cli_args: ConvertCliArgs) -> Result<ConvertArgs, AppFailure> {
    let current_dir = env::current_dir().map_err(|error| {
        AppFailure::system(format!("Failed to read the current directory: {error}"))
    })?;
    let environment = collect_convert_environment(&current_dir);
    resolve_convert_args_with_environment(cli_args, &environment, &current_dir)
}

fn collect_convert_environment(current_dir: &Path) -> ConvertEnvironment {
    ConvertEnvironment {
        config_path: env::var_os("MARKNEST_CONFIG")
            .map(PathBuf::from)
            .map(|path| resolve_path_against(current_dir, path)),
        theme: env::var("MARKNEST_THEME").ok(),
        css_path: env::var_os("MARKNEST_CSS")
            .map(PathBuf::from)
            .map(|path| resolve_path_against(current_dir, path)),
        enable_toc: env::var("MARKNEST_TOC").ok(),
        sanitize_html: env::var("MARKNEST_SANITIZE_HTML").ok(),
        mermaid_timeout_ms: env::var("MARKNEST_MERMAID_TIMEOUT_MS").ok(),
        math_timeout_ms: env::var("MARKNEST_MATH_TIMEOUT_MS").ok(),
    }
}

fn resolve_convert_args_with_environment(
    cli_args: ConvertCliArgs,
    environment: &ConvertEnvironment,
    current_dir: &Path,
) -> Result<ConvertArgs, AppFailure> {
    let config_path = resolve_convert_config_path(
        cli_args.config.as_deref(),
        environment.config_path.as_deref(),
        cli_args.input.as_deref(),
        current_dir,
    )?;
    let config = load_convert_config_file(config_path.as_deref())?;
    let config_directory = config_path
        .as_deref()
        .and_then(Path::parent)
        .unwrap_or(current_dir);

    let config_theme = parse_optional_theme(config.theme.as_deref(), "config file")?;
    let environment_theme = parse_optional_theme(environment.theme.as_deref(), "environment")?;
    let config_page_size = parse_optional_page_size(config.page_size.as_deref(), "config file")?;
    let config_mermaid_mode =
        parse_optional_mermaid_mode(config.mermaid.as_deref(), "config file")?;
    let config_math_mode = parse_optional_math_mode(config.math.as_deref(), "config file")?;
    let environment_enable_toc =
        parse_optional_bool_text(environment.enable_toc.as_deref(), "environment", "toc")?;
    let environment_sanitize_html = parse_optional_bool_text(
        environment.sanitize_html.as_deref(),
        "environment",
        "sanitize_html",
    )?;
    let environment_mermaid_timeout = parse_optional_timeout_ms_text(
        environment.mermaid_timeout_ms.as_deref(),
        "environment",
        "mermaid timeout",
    )?;
    let environment_math_timeout = parse_optional_timeout_ms_text(
        environment.math_timeout_ms.as_deref(),
        "environment",
        "math timeout",
    )?;
    let margins_mm = resolve_pdf_margins(&cli_args, &config)?;

    Ok(ConvertArgs {
        input: cli_args.input,
        entry: cli_args.entry,
        all: cli_args.all,
        output: cli_args.output,
        out_dir: cli_args.out_dir,
        render_report: cli_args.render_report.or_else(|| {
            resolve_optional_config_path(config.render_report.as_deref(), config_directory)
        }),
        debug_html: cli_args.debug_html.or_else(|| {
            resolve_optional_config_path(config.debug_html.as_deref(), config_directory)
        }),
        asset_manifest: cli_args.asset_manifest.or_else(|| {
            resolve_optional_config_path(config.asset_manifest.as_deref(), config_directory)
        }),
        css_path: cli_args
            .css
            .or_else(|| resolve_optional_config_path(config.css.as_deref(), config_directory))
            .or_else(|| environment.css_path.clone()),
        header_template_path: cli_args.header_template.or_else(|| {
            resolve_optional_config_path(config.header_template.as_deref(), config_directory)
        }),
        footer_template_path: cli_args.footer_template.or_else(|| {
            resolve_optional_config_path(config.footer_template.as_deref(), config_directory)
        }),
        page_size: cli_args
            .page_size
            .or(config_page_size)
            .unwrap_or(PdfPageSize::A4),
        margins_mm,
        theme: cli_args
            .theme
            .or(config_theme)
            .or(environment_theme)
            .unwrap_or(ThemePreset::Default),
        landscape: cli_args.landscape.or(config.landscape).unwrap_or(false),
        enable_toc: cli_args
            .enable_toc
            .or(config.toc)
            .or(environment_enable_toc)
            .unwrap_or(false),
        sanitize_html: cli_args
            .sanitize_html
            .or(config.sanitize_html)
            .or(environment_sanitize_html)
            .unwrap_or(true),
        metadata: PdfMetadata {
            title: cli_args.metadata_title.or(config.title),
            author: cli_args.metadata_author.or(config.author),
            subject: cli_args.metadata_subject.or(config.subject),
        },
        mermaid_mode: cli_args
            .mermaid_mode
            .or(config_mermaid_mode)
            .unwrap_or(MermaidMode::Auto),
        math_mode: cli_args
            .math_mode
            .or(config_math_mode)
            .unwrap_or(MathMode::Auto),
        mermaid_timeout_ms: cli_args
            .mermaid_timeout_ms
            .or(config.mermaid_timeout_ms)
            .or(environment_mermaid_timeout)
            .unwrap_or(DEFAULT_MERMAID_TIMEOUT_MS),
        math_timeout_ms: cli_args
            .math_timeout_ms
            .or(config.math_timeout_ms)
            .or(environment_math_timeout)
            .unwrap_or(DEFAULT_MATH_TIMEOUT_MS),
    })
}

fn resolve_convert_config_path(
    cli_config_path: Option<&Path>,
    environment_config_path: Option<&Path>,
    input_path: Option<&Path>,
    current_dir: &Path,
) -> Result<Option<PathBuf>, AppFailure> {
    if let Some(path) = cli_config_path {
        let resolved_path = resolve_path_against(current_dir, path.to_path_buf());
        if !resolved_path.exists() {
            return Err(AppFailure::validation(format!(
                "Config file could not be found: {}",
                resolved_path.display()
            )));
        }
        return Ok(Some(resolved_path));
    }

    if let Some(path) = environment_config_path {
        if !path.exists() {
            return Err(AppFailure::validation(format!(
                "Config file could not be found: {}",
                path.display()
            )));
        }
        return Ok(Some(path.to_path_buf()));
    }

    discover_default_config_path(input_path, current_dir)
}

fn discover_default_config_path(
    input_path: Option<&Path>,
    current_dir: &Path,
) -> Result<Option<PathBuf>, AppFailure> {
    let mut candidate_directories: Vec<PathBuf> = Vec::new();
    if let Some(input_path) = input_path {
        let resolved_input_path = resolve_path_against(current_dir, input_path.to_path_buf());
        if resolved_input_path.is_dir() {
            candidate_directories.push(resolved_input_path);
        } else if let Some(parent) = resolved_input_path.parent() {
            candidate_directories.push(parent.to_path_buf());
        }
    }
    candidate_directories.push(current_dir.to_path_buf());

    for candidate_directory in candidate_directories {
        for file_name in [".marknest.toml", "marknest.toml"] {
            let candidate_path = candidate_directory.join(file_name);
            if candidate_path.exists() {
                return Ok(Some(candidate_path));
            }
        }
    }

    Ok(None)
}

fn load_convert_config_file(config_path: Option<&Path>) -> Result<ConvertConfigFile, AppFailure> {
    let Some(config_path) = config_path else {
        return Ok(ConvertConfigFile::default());
    };

    let config_text = fs::read_to_string(config_path).map_err(|error| {
        AppFailure::validation(format!(
            "Config file could not be read {}: {error}",
            config_path.display()
        ))
    })?;
    let config_file: MarknestConfigFile = toml::from_str(&config_text).map_err(|error| {
        AppFailure::validation(format!(
            "Config file could not be parsed {}: {error}",
            config_path.display()
        ))
    })?;

    Ok(config_file.convert.unwrap_or_default())
}

fn parse_optional_theme(
    value: Option<&str>,
    source_label: &str,
) -> Result<Option<ThemePreset>, AppFailure> {
    match value {
        Some(value) => parse_theme_preset(value).map(Some).map_err(|error| {
            AppFailure::validation(format!("Invalid {source_label} theme: {}", error.message))
        }),
        None => Ok(None),
    }
}

fn parse_optional_page_size(
    value: Option<&str>,
    source_label: &str,
) -> Result<Option<PdfPageSize>, AppFailure> {
    match value {
        Some(value) => PdfPageSize::parse(value).map(Some).map_err(|error| {
            AppFailure::validation(format!(
                "Invalid {source_label} page size: {}",
                error.message
            ))
        }),
        None => Ok(None),
    }
}

fn parse_optional_mermaid_mode(
    value: Option<&str>,
    source_label: &str,
) -> Result<Option<MermaidMode>, AppFailure> {
    match value {
        Some(value) => parse_mermaid_mode(value).map(Some).map_err(|error| {
            AppFailure::validation(format!(
                "Invalid {source_label} mermaid mode: {}",
                error.message
            ))
        }),
        None => Ok(None),
    }
}

fn parse_optional_math_mode(
    value: Option<&str>,
    source_label: &str,
) -> Result<Option<MathMode>, AppFailure> {
    match value {
        Some(value) => parse_math_mode(value).map(Some).map_err(|error| {
            AppFailure::validation(format!(
                "Invalid {source_label} math mode: {}",
                error.message
            ))
        }),
        None => Ok(None),
    }
}

fn parse_bool(value: &str, label: &str) -> Result<bool, ParseFailure> {
    if value.eq_ignore_ascii_case("true")
        || value.eq_ignore_ascii_case("1")
        || value.eq_ignore_ascii_case("yes")
        || value.eq_ignore_ascii_case("on")
    {
        Ok(true)
    } else if value.eq_ignore_ascii_case("false")
        || value.eq_ignore_ascii_case("0")
        || value.eq_ignore_ascii_case("no")
        || value.eq_ignore_ascii_case("off")
    {
        Ok(false)
    } else {
        Err(ParseFailure::new(format!(
            "Invalid {label} value: {value}. Use true or false."
        )))
    }
}

fn parse_timeout_ms(value: &str, label: &str) -> Result<u32, ParseFailure> {
    let timeout_ms: u32 = value
        .parse()
        .map_err(|_| ParseFailure::new(format!("Invalid {label} value: {value}")))?;
    if timeout_ms == 0 {
        return Err(ParseFailure::new(format!(
            "{label} must be greater than zero."
        )));
    }

    Ok(timeout_ms)
}

fn parse_optional_bool_text(
    value: Option<&str>,
    source_label: &str,
    option_label: &str,
) -> Result<Option<bool>, AppFailure> {
    match value {
        Some(value) => parse_bool(value, option_label).map(Some).map_err(|error| {
            AppFailure::validation(format!(
                "Invalid {source_label} {option_label}: {}",
                error.message
            ))
        }),
        None => Ok(None),
    }
}

fn parse_optional_timeout_ms_text(
    value: Option<&str>,
    source_label: &str,
    option_label: &str,
) -> Result<Option<u32>, AppFailure> {
    match value {
        Some(value) => parse_timeout_ms(value, option_label)
            .map(Some)
            .map_err(|error| {
                AppFailure::validation(format!(
                    "Invalid {source_label} {option_label}: {}",
                    error.message
                ))
            }),
        None => Ok(None),
    }
}

fn resolve_optional_config_path(value: Option<&str>, base_directory: &Path) -> Option<PathBuf> {
    value.map(|value| resolve_path_against(base_directory, PathBuf::from(value)))
}

fn resolve_path_against(base_directory: &Path, path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        base_directory.join(path)
    }
}

#[derive(Debug, Deserialize)]
struct BrowserRenderDiagnostics {
    kind: String,
    warnings: Vec<String>,
    errors: Vec<String>,
    message: Option<String>,
}

fn extract_render_diagnostics(output: &[u8]) -> Option<BrowserRenderDiagnostics> {
    let text = String::from_utf8_lossy(output);
    for line in text.lines().rev() {
        let trimmed = line.trim();
        if trimmed.starts_with('{') {
            if let Ok(diagnostics) = serde_json::from_str::<BrowserRenderDiagnostics>(trimmed) {
                return Some(diagnostics);
            }
        }
    }

    None
}

fn diagnostics_message(
    diagnostics: Option<&BrowserRenderDiagnostics>,
    stdout: &str,
    stderr: &str,
) -> String {
    if let Some(diagnostics) = diagnostics {
        if let Some(message) = &diagnostics.message {
            return message.clone();
        }
        if !diagnostics.errors.is_empty() {
            return diagnostics.errors.join("; ");
        }
        if !diagnostics.warnings.is_empty() {
            return diagnostics.warnings.join("; ");
        }
    }

    if !stderr.is_empty() {
        stderr.to_string()
    } else if !stdout.is_empty() {
        stdout.to_string()
    } else {
        "The helper script exited without diagnostic output.".to_string()
    }
}

fn render_failure_from_output(output: &std::process::Output) -> PdfRenderFailure {
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let diagnostics = extract_render_diagnostics(&output.stdout)
        .or_else(|| extract_render_diagnostics(&output.stderr));
    let message = diagnostics_message(diagnostics.as_ref(), &stdout, &stderr);

    if matches!(
        diagnostics
            .as_ref()
            .map(|diagnostic| diagnostic.kind.as_str()),
        Some("validation")
    ) || output.status.code() == Some(2)
    {
        PdfRenderFailure::validation(message)
    } else {
        PdfRenderFailure::system(message)
    }
}

fn render_outcome_from_output(output: &std::process::Output) -> PdfRenderOutcome {
    extract_render_diagnostics(&output.stdout)
        .map(|diagnostics| PdfRenderOutcome {
            warnings: diagnostics.warnings,
        })
        .unwrap_or_default()
}

fn apply_pdf_metadata(pdf_path: &Path, metadata: &PdfMetadata) -> Result<(), String> {
    if metadata.title.is_none() && metadata.author.is_none() && metadata.subject.is_none() {
        return Ok(());
    }

    let pdf_bytes = fs::read(pdf_path)
        .map_err(|error| format!("Failed to read {}: {error}", pdf_path.display()))?;
    let pdf_text = String::from_utf8_lossy(&pdf_bytes);
    let previous_startxref = parse_startxref(&pdf_text)?;
    let trailer_text = last_trailer_section(&pdf_text)?;
    let size = parse_trailer_size(trailer_text)?;
    let root_reference = parse_trailer_reference(trailer_text, "/Root")?;
    let info_object_number = size;
    let info_object = build_pdf_info_object(info_object_number, metadata);
    let xref_offset = pdf_bytes.len() + info_object.len();
    let xref_section = format!(
        "xref\n{info_object_number} 1\n{:010} 00000 n \ntrailer\n<< /Size {} /Root {} /Info {} 0 R /Prev {} >>\nstartxref\n{xref_offset}\n%%EOF\n",
        pdf_bytes.len(),
        info_object_number + 1,
        root_reference,
        info_object_number,
        previous_startxref
    );

    let mut file = fs::OpenOptions::new()
        .append(true)
        .open(pdf_path)
        .map_err(|error| format!("Failed to reopen {}: {error}", pdf_path.display()))?;
    file.write_all(info_object.as_bytes())
        .and_then(|_| file.write_all(xref_section.as_bytes()))
        .map_err(|error| {
            format!(
                "Failed to append metadata to {}: {error}",
                pdf_path.display()
            )
        })
}

fn parse_startxref(pdf_text: &str) -> Result<usize, String> {
    let startxref_index = pdf_text
        .rfind("startxref")
        .ok_or_else(|| "The PDF does not contain startxref.".to_string())?;
    let after_startxref = &pdf_text[startxref_index + "startxref".len()..];
    let offset_line = after_startxref
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .ok_or_else(|| "The PDF startxref value is missing.".to_string())?;

    offset_line
        .parse::<usize>()
        .map_err(|error| format!("The PDF startxref value is invalid: {error}"))
}

fn last_trailer_section(pdf_text: &str) -> Result<&str, String> {
    let trailer_index = pdf_text
        .rfind("trailer")
        .ok_or_else(|| "The PDF does not contain a trailer dictionary.".to_string())?;
    let startxref_index = pdf_text[trailer_index..]
        .find("startxref")
        .map(|index| trailer_index + index)
        .ok_or_else(|| "The PDF trailer is incomplete.".to_string())?;

    Ok(&pdf_text[trailer_index..startxref_index])
}

fn parse_trailer_size(trailer_text: &str) -> Result<usize, String> {
    let normalized_trailer = trailer_text.replace("<<", "<< ").replace(">>", " >>");
    let tokens: Vec<&str> = normalized_trailer.split_ascii_whitespace().collect();
    let size_index = tokens
        .iter()
        .position(|token| *token == "/Size")
        .ok_or_else(|| "The PDF trailer is missing /Size.".to_string())?;
    let size_value = tokens
        .get(size_index + 1)
        .ok_or_else(|| "The PDF trailer size value is missing.".to_string())?;

    size_value
        .parse::<usize>()
        .map_err(|error| format!("The PDF trailer size is invalid: {error}"))
}

fn parse_trailer_reference(trailer_text: &str, key: &str) -> Result<String, String> {
    let normalized_trailer = trailer_text.replace("<<", "<< ").replace(">>", " >>");
    let tokens: Vec<&str> = normalized_trailer.split_ascii_whitespace().collect();
    let key_index = tokens
        .iter()
        .position(|token| *token == key)
        .ok_or_else(|| format!("The PDF trailer is missing {key}."))?;
    let object_number = tokens
        .get(key_index + 1)
        .ok_or_else(|| format!("The PDF trailer {key} object number is missing."))?;
    let generation_number = tokens
        .get(key_index + 2)
        .ok_or_else(|| format!("The PDF trailer {key} generation number is missing."))?;
    let reference_marker = tokens
        .get(key_index + 3)
        .ok_or_else(|| format!("The PDF trailer {key} reference marker is missing."))?;

    if *reference_marker != "R" {
        return Err(format!("The PDF trailer {key} reference is invalid."));
    }

    Ok(format!("{object_number} {generation_number} R"))
}

fn build_pdf_info_object(object_number: usize, metadata: &PdfMetadata) -> String {
    let mut fields: Vec<String> = Vec::new();

    if let Some(title) = &metadata.title {
        fields.push(format!("/Title ({})", escape_pdf_literal_string(title)));
    }
    if let Some(author) = &metadata.author {
        fields.push(format!("/Author ({})", escape_pdf_literal_string(author)));
    }
    if let Some(subject) = &metadata.subject {
        fields.push(format!("/Subject ({})", escape_pdf_literal_string(subject)));
    }

    format!(
        "\n{object_number} 0 obj\n<< {} >>\nendobj\n",
        fields.join(" ")
    )
}

fn escape_pdf_literal_string(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)")
        .replace('\r', "\\r")
        .replace('\n', "\\n")
}

enum ParseResult {
    Help(String),
    Convert(ConvertCliArgs),
    Validate(ValidateArgs),
}

struct ParseFailure {
    message: String,
}

impl ParseFailure {
    fn new(message: String) -> Self {
        Self { message }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedGitHubUrl {
    owner: String,
    repo: String,
    git_ref: Option<String>,
    subpath: Option<String>,
    is_file_reference: bool,
}

/// Parse a GitHub URL into its components. Returns `None` for non-GitHub URLs
/// or malformed input.
fn parse_github_url(input: &str) -> Option<ParsedGitHubUrl> {
    let trimmed: &str = input.trim();

    // Must start with http:// or https://
    let after_scheme: &str = trimmed
        .strip_prefix("https://")
        .or_else(|| trimmed.strip_prefix("http://"))?;

    // Must be github.com host (with optional www.)
    let after_host: &str = after_scheme
        .strip_prefix("github.com/")
        .or_else(|| after_scheme.strip_prefix("www.github.com/"))?;

    // Split remaining path segments
    let segments: Vec<&str> = after_host
        .trim_end_matches('/')
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();

    if segments.len() < 2 {
        return None;
    }

    let owner: String = segments[0].to_string();
    let repo: String = segments[1].trim_end_matches(".git").to_string();

    if owner.is_empty() || repo.is_empty() {
        return None;
    }

    // Bare repo URL: https://github.com/owner/repo
    if segments.len() == 2 {
        return Some(ParsedGitHubUrl {
            owner,
            repo,
            git_ref: None,
            subpath: None,
            is_file_reference: false,
        });
    }

    // Must have /tree/ or /blob/ as the third segment
    let path_type: &str = segments[2];
    let is_file_reference: bool = match path_type {
        "blob" => true,
        "tree" => false,
        _ => return None,
    };

    // Must have a ref after /tree/ or /blob/
    if segments.len() < 4 {
        return None;
    }

    let git_ref: String = segments[3].to_string();
    let subpath: Option<String> = if segments.len() > 4 {
        Some(segments[4..].join("/"))
    } else {
        None
    };

    Some(ParsedGitHubUrl {
        owner,
        repo,
        git_ref: Some(git_ref),
        subpath,
        is_file_reference,
    })
}

/// Resolve GitHub auth token from environment variables.
/// Checks GITHUB_TOKEN first, then falls back to GH_TOKEN.
fn resolve_github_auth_token() -> Option<String> {
    env::var("GITHUB_TOKEN")
        .ok()
        .or_else(|| env::var("GH_TOKEN").ok())
        .filter(|token| !token.is_empty())
}

fn build_github_api_agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(GITHUB_API_TIMEOUT_SECONDS))
        .timeout_read(Duration::from_secs(GITHUB_API_TIMEOUT_SECONDS))
        .timeout_write(Duration::from_secs(GITHUB_API_TIMEOUT_SECONDS))
        .redirects(GITHUB_API_MAX_REDIRECTS)
        .build()
}

/// Query the GitHub API for the default branch of a repository.
fn resolve_github_default_branch(
    owner: &str,
    repo: &str,
    token: Option<&str>,
) -> Result<String, AppFailure> {
    let url: String = format!("https://api.github.com/repos/{owner}/{repo}");
    let agent: ureq::Agent = build_github_api_agent();
    let mut request = agent
        .get(&url)
        .set("Accept", "application/vnd.github+json")
        .set("User-Agent", "marknest");

    if let Some(token) = token {
        request = request.set("Authorization", &format!("Bearer {token}"));
    }

    let response = request.call().map_err(|error| match &error {
        ureq::Error::Status(404, _) => AppFailure::validation(
            "GitHub repository not found or access denied. Use GITHUB_TOKEN or GH_TOKEN for private repositories.".to_string(),
        ),
        ureq::Error::Status(403, response) => {
            if response
                .header("X-RateLimit-Remaining")
                .map(|value| value == "0")
                .unwrap_or(false)
            {
                AppFailure::validation(
                    "GitHub API rate limit exceeded. Set GITHUB_TOKEN or GH_TOKEN to increase the limit.".to_string(),
                )
            } else {
                AppFailure::system(format!("Failed to query the GitHub API: {error}"))
            }
        }
        _ => AppFailure::system(format!("Failed to query the GitHub API: {error}")),
    })?;

    let body: String = response.into_string().map_err(|error| {
        AppFailure::system(format!("Failed to read the GitHub API response: {error}"))
    })?;

    let json: serde_json::Value = serde_json::from_str(&body).map_err(|error| {
        AppFailure::system(format!("Failed to parse the GitHub API response: {error}"))
    })?;

    json["default_branch"]
        .as_str()
        .map(|value| value.to_string())
        .ok_or_else(|| {
            AppFailure::system(
                "GitHub API response did not include a default_branch field.".to_string(),
            )
        })
}

/// Download a GitHub repository archive as a ZIP file.
fn download_github_archive(
    owner: &str,
    repo: &str,
    git_ref: &str,
    token: Option<&str>,
) -> Result<Vec<u8>, AppFailure> {
    let url: String = format!("https://api.github.com/repos/{owner}/{repo}/zipball/{git_ref}");
    let agent: ureq::Agent = build_github_api_agent();
    let mut request = agent
        .get(&url)
        .set("Accept", "application/vnd.github+json")
        .set("User-Agent", "marknest");

    if let Some(token) = token {
        request = request.set("Authorization", &format!("Bearer {token}"));
    }

    let response = request.call().map_err(|error| match &error {
        ureq::Error::Status(404, _) => AppFailure::validation(
            "GitHub repository not found or access denied. Use GITHUB_TOKEN or GH_TOKEN for private repositories.".to_string(),
        ),
        ureq::Error::Status(403, response) => {
            if response
                .header("X-RateLimit-Remaining")
                .map(|value| value == "0")
                .unwrap_or(false)
            {
                AppFailure::validation(
                    "GitHub API rate limit exceeded. Set GITHUB_TOKEN or GH_TOKEN to increase the limit.".to_string(),
                )
            } else {
                AppFailure::system(format!("Failed to download the GitHub archive: {error}"))
            }
        }
        _ => AppFailure::system(format!("Failed to download the GitHub archive: {error}")),
    })?;

    let mut reader = response.into_reader();
    let mut bytes: Vec<u8> = Vec::new();
    let mut buffer = [0_u8; 8192];

    loop {
        let bytes_read: usize = reader.read(&mut buffer).map_err(|error| {
            AppFailure::system(format!("Failed to download the GitHub archive: {error}"))
        })?;
        if bytes_read == 0 {
            break;
        }

        if bytes.len() + bytes_read > GITHUB_ARCHIVE_MAX_BYTES {
            return Err(AppFailure::validation(
                "GitHub archive download exceeded the 256 MB limit.".to_string(),
            ));
        }

        bytes.extend_from_slice(&buffer[..bytes_read]);
    }

    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::fs;
    use std::io::Write;
    use std::net::{Shutdown, TcpListener};
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::thread;

    use tempfile::TempDir;
    use zip::write::SimpleFileOptions;

    fn fixtures_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("marknest-core")
            .join("tests")
            .join("fixtures")
    }

    fn copy_directory(source: &Path, destination: &Path) {
        fs::create_dir_all(destination).expect("destination directory should exist");

        for entry in fs::read_dir(source).expect("source directory should exist") {
            let entry = entry.expect("directory entry should be readable");
            let source_path = entry.path();
            let destination_path = destination.join(entry.file_name());
            let file_type = entry.file_type().expect("file type should be readable");

            if file_type.is_dir() {
                copy_directory(&source_path, &destination_path);
            } else {
                fs::copy(&source_path, &destination_path).expect("fixture file should be copied");
            }
        }
    }

    fn write_text_file(path: &Path, contents: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("parent directory should be created");
        }
        fs::write(path, contents).expect("fixture file should be written");
    }

    fn build_zip_file(entries: &[(&str, &str)]) -> (TempDir, PathBuf) {
        let temp_dir = TempDir::new().expect("temp dir should be created");
        let zip_path = temp_dir.path().join("workspace.zip");
        let file = fs::File::create(&zip_path).expect("zip file should be created");
        let mut writer = zip::ZipWriter::new(file);

        for (path, contents) in entries {
            writer
                .start_file(path, SimpleFileOptions::default())
                .expect("zip entry should be created");
            writer
                .write_all(contents.as_bytes())
                .expect("zip contents should be written");
        }

        writer.finish().expect("zip should finish");
        (temp_dir, zip_path)
    }

    #[derive(Clone)]
    struct TestHttpResponse {
        status_line: &'static str,
        content_type: &'static str,
        body: Vec<u8>,
        location: Option<String>,
    }

    impl TestHttpResponse {
        fn ok_png(body: &[u8]) -> Self {
            Self {
                status_line: "HTTP/1.1 200 OK",
                content_type: "image/png",
                body: body.to_vec(),
                location: None,
            }
        }

        fn not_found() -> Self {
            Self {
                status_line: "HTTP/1.1 404 Not Found",
                content_type: "text/plain; charset=utf-8",
                body: b"missing".to_vec(),
                location: None,
            }
        }
    }

    struct TestHttpServer {
        address: String,
        request_count: Arc<AtomicUsize>,
        should_stop: Arc<AtomicBool>,
        join_handle: Option<thread::JoinHandle<()>>,
    }

    impl TestHttpServer {
        fn start(routes: Vec<(&'static str, TestHttpResponse)>) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("listener should bind");
            listener
                .set_nonblocking(true)
                .expect("listener should switch to non-blocking mode");
            let address = listener
                .local_addr()
                .expect("local addr should resolve")
                .to_string();
            let request_count = Arc::new(AtomicUsize::new(0));
            let request_count_for_thread = Arc::clone(&request_count);
            let should_stop = Arc::new(AtomicBool::new(false));
            let should_stop_for_thread = Arc::clone(&should_stop);
            let route_map = Arc::new(
                routes
                    .into_iter()
                    .map(|(path, response)| (path.to_string(), response))
                    .collect::<std::collections::BTreeMap<_, _>>(),
            );
            let route_map_for_thread = Arc::clone(&route_map);

            let join_handle = thread::spawn(move || {
                loop {
                    if should_stop_for_thread.load(Ordering::SeqCst) {
                        break;
                    }

                    let mut stream = match listener.accept() {
                        Ok((stream, _)) => stream,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(std::time::Duration::from_millis(10));
                            continue;
                        }
                        Err(_) => break,
                    };
                    let mut buffer = [0_u8; 4096];
                    let bytes_read = std::io::Read::read(&mut stream, &mut buffer).unwrap_or(0);
                    if bytes_read == 0 {
                        continue;
                    }

                    let request = String::from_utf8_lossy(&buffer[..bytes_read]);
                    let Some(request_line) = request.lines().next() else {
                        continue;
                    };
                    let path = request_line
                        .split_ascii_whitespace()
                        .nth(1)
                        .unwrap_or("/")
                        .to_string();
                    request_count_for_thread.fetch_add(1, Ordering::SeqCst);

                    let response = route_map_for_thread
                        .get(&path)
                        .cloned()
                        .unwrap_or_else(TestHttpResponse::not_found);

                    let mut header = format!(
                        "{}\r\nContent-Length: {}\r\nContent-Type: {}\r\nConnection: close\r\n",
                        response.status_line,
                        response.body.len(),
                        response.content_type
                    );
                    if let Some(location) = response.location {
                        header.push_str(&format!("Location: {location}\r\n"));
                    }
                    header.push_str("\r\n");

                    let _ = stream.write_all(header.as_bytes());
                    let _ = stream.write_all(&response.body);
                    let _ = stream.flush();
                    let _ = stream.shutdown(Shutdown::Both);
                }
            });

            Self {
                address,
                request_count,
                should_stop,
                join_handle: Some(join_handle),
            }
        }

        fn url(&self, path: &str) -> String {
            format!("http://{}/{}", self.address, path.trim_start_matches('/'))
        }

        fn request_count(&self) -> usize {
            self.request_count.load(Ordering::SeqCst)
        }
    }

    impl Drop for TestHttpServer {
        fn drop(&mut self) {
            self.should_stop.store(true, Ordering::SeqCst);
            if let Some(join_handle) = self.join_handle.take() {
                let _ = join_handle.join();
            }
        }
    }

    fn request_output_paths(renderer: &MockPdfRenderer) -> Vec<PathBuf> {
        renderer
            .requests
            .lock()
            .expect("requests mutex should lock")
            .iter()
            .map(|request| request.output_path.clone())
            .collect()
    }

    #[test]
    fn materialize_remote_assets_inlines_successful_fetches_and_reuses_the_fetch_result() {
        let server = TestHttpServer::start(vec![(
            "/image.png",
            TestHttpResponse::ok_png(b"\x89PNG\r\n\x1a\nmarknest"),
        )]);
        let remote_url = server.url("/image.png");
        let assets = vec![
            AssetRef {
                entry_path: "README.md".to_string(),
                original_reference: remote_url.clone(),
                resolved_path: None,
                kind: marknest_core::AssetReferenceKind::MarkdownImage,
                status: marknest_core::AssetStatus::External,
                fetch_url: Some(remote_url.clone()),
            },
            AssetRef {
                entry_path: "README.md".to_string(),
                original_reference: remote_url.clone(),
                resolved_path: None,
                kind: marknest_core::AssetReferenceKind::RawHtmlImage,
                status: marknest_core::AssetStatus::External,
                fetch_url: Some(remote_url.clone()),
            },
        ];

        let outcome = materialize_remote_assets_for_entry(
            Some(&format!(
                "<html><body><img src=\"{remote_url}\"><img src=\"{remote_url}\"></body></html>"
            )),
            &assets,
            RemoteAssetApplyMode::InlineHtml,
        )
        .expect("remote assets should materialize");

        assert!(
            outcome
                .html
                .as_deref()
                .expect("html should be rewritten")
                .contains("data:image/png;base64,")
        );
        assert_eq!(outcome.warnings, Vec::<String>::new());
        assert_eq!(outcome.remote_assets.len(), 2);
        assert!(
            outcome
                .remote_assets
                .iter()
                .all(|asset| asset.status == RemoteAssetStatus::Inlined)
        );
        assert_eq!(server.request_count(), 1);
    }

    #[test]
    fn validate_strict_fails_when_remote_image_materialization_warns() {
        let server = TestHttpServer::start(vec![("/missing.png", TestHttpResponse::not_found())]);
        let temp_dir = TempDir::new().expect("temp dir should be created");
        let workspace_path = temp_dir.path().join("workspace");
        let report_path = temp_dir.path().join("report.json");
        write_text_file(
            &workspace_path.join("README.md"),
            &format!("# Remote\n\n![Missing]({})\n", server.url("/missing.png")),
        );

        let exit_code = run([
            "marknest",
            "validate",
            workspace_path.to_str().expect("path should be utf-8"),
            "--strict",
            "--report",
            report_path.to_str().expect("path should be utf-8"),
        ]);

        assert_eq!(exit_code, 2);

        let report_json: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(&report_path).expect("report should be readable"),
        )
        .expect("report should be valid json");
        assert!(
            report_json["errors"]
                .as_array()
                .expect("errors should be an array")
                .iter()
                .any(|value| value
                    .as_str()
                    .unwrap_or_default()
                    .contains("Remote asset could not be materialized"))
        );
        assert_eq!(report_json["remote_assets"][0]["status"], "failed");
    }

    #[test]
    fn convert_can_render_a_single_zip_entry_when_entry_is_requested() {
        let (_temp_dir, zip_path) = build_zip_file(&[
            (
                "docs/README.md",
                "# Zip Guide\n\n![Architecture](../images/architecture.svg)\n",
            ),
            (
                "images/architecture.svg",
                "<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>",
            ),
        ]);
        let output_temp_dir = TempDir::new().expect("temp dir should be created");
        let output_path = output_temp_dir.path().join("guide.pdf");
        let renderer = MockPdfRenderer::default();

        let exit_code = run_with_pdf_renderer(
            [
                "marknest",
                "convert",
                zip_path.to_str().expect("path should be utf-8"),
                "--entry",
                "docs/README.md",
                "-o",
                output_path.to_str().expect("path should be utf-8"),
            ],
            &renderer,
        );

        assert_eq!(exit_code, 0);
        assert!(output_path.exists());

        let requests = renderer
            .requests
            .lock()
            .expect("requests mutex should lock");
        let request = requests
            .first()
            .expect("zip convert should invoke the renderer once");
        assert!(request.html.contains("<h1 id=\"zip-guide\">Zip Guide</h1>"));
        assert!(request.html.contains("data:image/svg+xml;base64,"));
    }

    #[test]
    fn convert_explicit_folder_input_uses_batch_mode_and_preserves_relative_paths() {
        let temp_dir = TempDir::new().expect("temp dir should be created");
        let workspace_path = temp_dir.path().join("workspace");
        write_text_file(
            &workspace_path.join("guides/getting-started.md"),
            "# Getting Started\n",
        );
        write_text_file(&workspace_path.join("reference/api.markdown"), "# API\n");
        let output_dir = temp_dir.path().join("out");
        let renderer = MockPdfRenderer::default();

        let exit_code = run_with_pdf_renderer(
            [
                "marknest",
                "convert",
                workspace_path.to_str().expect("path should be utf-8"),
                "--out-dir",
                output_dir.to_str().expect("path should be utf-8"),
            ],
            &renderer,
        );

        assert_eq!(exit_code, 0);
        assert!(output_dir.join("guides/getting-started.pdf").exists());
        assert!(output_dir.join("reference/api.pdf").exists());

        let mut output_paths = request_output_paths(&renderer);
        output_paths.sort();
        assert_eq!(
            output_paths,
            vec![
                output_dir.join("guides/getting-started.pdf"),
                output_dir.join("reference/api.pdf"),
            ]
        );
    }

    #[test]
    fn convert_can_render_all_entries_from_a_zip_input() {
        let (_temp_dir, zip_path) = build_zip_file(&[
            ("guides/getting-started.md", "# Getting Started\n"),
            ("reference/api.markdown", "# API\n"),
        ]);
        let output_temp_dir = TempDir::new().expect("temp dir should be created");
        let output_dir = output_temp_dir.path().join("out");
        let renderer = MockPdfRenderer::default();

        let exit_code = run_with_pdf_renderer(
            [
                "marknest",
                "convert",
                zip_path.to_str().expect("path should be utf-8"),
                "--all",
                "--out-dir",
                output_dir.to_str().expect("path should be utf-8"),
            ],
            &renderer,
        );

        assert_eq!(exit_code, 0);
        assert!(output_dir.join("guides/getting-started.pdf").exists());
        assert!(output_dir.join("reference/api.pdf").exists());
        assert_eq!(request_output_paths(&renderer).len(), 2);
    }

    #[test]
    fn convert_batch_writes_a_render_report_with_outputs_and_warnings() {
        let temp_dir = TempDir::new().expect("temp dir should be created");
        let workspace_path = temp_dir.path().join("workspace");
        copy_directory(
            &fixtures_root().join("workspace_missing_asset"),
            &workspace_path,
        );
        let output_dir = temp_dir.path().join("out");
        let report_path = temp_dir.path().join("render-report.json");
        let renderer = MockPdfRenderer::default();

        let exit_code = run_with_pdf_renderer(
            [
                "marknest",
                "convert",
                workspace_path.to_str().expect("path should be utf-8"),
                "--out-dir",
                output_dir.to_str().expect("path should be utf-8"),
                "--render-report",
                report_path.to_str().expect("path should be utf-8"),
            ],
            &renderer,
        );

        assert_eq!(exit_code, 1);
        assert!(report_path.exists());

        let report_json: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(&report_path).expect("report should be readable"),
        )
        .expect("report should be valid json");

        assert_eq!(report_json["outputs"][0]["entry_path"], "guide.md");
        assert_eq!(
            report_json["outputs"][0]["output_path"],
            output_dir.join("guide.pdf").display().to_string()
        );
        assert!(
            report_json["warnings"]
                .as_array()
                .expect("warnings should be an array")
                .iter()
                .any(|warning| warning
                    .as_str()
                    .unwrap_or_default()
                    .contains("does-not-exist.svg"))
        );
    }

    #[test]
    fn convert_batch_detects_output_collisions_before_rendering() {
        let temp_dir = TempDir::new().expect("temp dir should be created");
        let workspace_path = temp_dir.path().join("workspace");
        write_text_file(&workspace_path.join("guide.md"), "# Guide\n");
        write_text_file(
            &workspace_path.join("guide.markdown"),
            "# Guide Duplicate\n",
        );
        let output_dir = temp_dir.path().join("out");
        let report_path = temp_dir.path().join("render-report.json");
        let renderer = MockPdfRenderer::default();

        let exit_code = run_with_pdf_renderer(
            [
                "marknest",
                "convert",
                workspace_path.to_str().expect("path should be utf-8"),
                "--out-dir",
                output_dir.to_str().expect("path should be utf-8"),
                "--render-report",
                report_path.to_str().expect("path should be utf-8"),
            ],
            &renderer,
        );

        assert_eq!(exit_code, 2);
        assert_eq!(request_output_paths(&renderer).len(), 0);
        assert!(report_path.exists());

        let report_json: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(&report_path).expect("report should be readable"),
        )
        .expect("report should be valid json");

        assert!(
            report_json["collisions"]
                .as_array()
                .expect("collisions should be an array")
                .iter()
                .any(|collision| collision["output_path"]
                    == output_dir.join("guide.pdf").display().to_string())
        );
    }

    #[derive(Default)]
    struct MockPdfRenderer {
        requests: Mutex<Vec<PdfRenderRequest>>,
    }

    impl PdfRenderer for MockPdfRenderer {
        fn render(&self, request: &PdfRenderRequest) -> Result<PdfRenderOutcome, PdfRenderFailure> {
            self.requests
                .lock()
                .expect("requests mutex should lock")
                .push(request.clone());

            if let Some(parent) = request.output_path.parent() {
                fs::create_dir_all(parent).expect("output directory should be created");
            }
            fs::write(&request.output_path, sample_pdf_bytes())
                .expect("mock renderer should write a pdf");
            Ok(PdfRenderOutcome::default())
        }
    }

    #[test]
    fn convert_renders_a_single_markdown_file_to_pdf() {
        let temp_dir = TempDir::new().expect("temp dir should be created");
        let workspace_path = temp_dir.path().join("workspace");
        copy_directory(&fixtures_root().join("workspace_valid"), &workspace_path);
        let markdown_path = workspace_path.join("README.md");
        let renderer = MockPdfRenderer::default();

        let exit_code = run_with_pdf_renderer(
            [
                "marknest",
                "convert",
                markdown_path.to_str().expect("path should be utf-8"),
                "--page-size",
                "letter",
                "--margin",
                "24",
            ],
            &renderer,
        );

        assert_eq!(exit_code, 0);

        let output_path = workspace_path.join("README.pdf");
        assert!(output_path.exists());

        let requests = renderer
            .requests
            .lock()
            .expect("requests mutex should lock");
        let request = requests
            .first()
            .expect("convert should invoke the renderer once");
        assert_eq!(
            request
                .output_path
                .canonicalize()
                .expect("output path should canonicalize"),
            output_path
                .canonicalize()
                .expect("expected output path should canonicalize")
        );
        assert_eq!(request.page_size, PdfPageSize::Letter);
        assert_eq!(request.margins_mm.top, 24.0);
        assert_eq!(request.margins_mm.right, 24.0);
        assert_eq!(request.margins_mm.bottom, 24.0);
        assert_eq!(request.margins_mm.left, 24.0);
        assert!(request.html.contains("<h1 id=\"marknest\">MarkNest</h1>"));
        assert!(request.html.contains("data:image/svg+xml;base64,"));
    }

    #[test]
    fn convert_supports_individual_page_margins() {
        let temp_dir = TempDir::new().expect("temp dir should be created");
        let workspace_path = temp_dir.path().join("workspace");
        copy_directory(&fixtures_root().join("workspace_valid"), &workspace_path);
        let markdown_path = workspace_path.join("README.md");
        let renderer = MockPdfRenderer::default();

        let exit_code = run_with_pdf_renderer(
            [
                "marknest",
                "convert",
                markdown_path.to_str().expect("path should be utf-8"),
                "--margin",
                "16",
                "--margin-top",
                "24",
                "--margin-right",
                "12",
                "--margin-bottom",
                "20",
                "--margin-left",
                "8",
            ],
            &renderer,
        );

        assert_eq!(exit_code, 0);

        let requests = renderer
            .requests
            .lock()
            .expect("requests mutex should lock");
        let request = requests
            .first()
            .expect("convert should invoke the renderer once");
        assert_eq!(request.margins_mm.top, 24.0);
        assert_eq!(request.margins_mm.right, 12.0);
        assert_eq!(request.margins_mm.bottom, 20.0);
        assert_eq!(request.margins_mm.left, 8.0);
    }

    #[test]
    fn convert_returns_a_warning_exit_code_when_assets_are_missing() {
        let temp_dir = TempDir::new().expect("temp dir should be created");
        let workspace_path = temp_dir.path().join("workspace");
        copy_directory(
            &fixtures_root().join("workspace_missing_asset"),
            &workspace_path,
        );
        let markdown_path = workspace_path.join("guide.md");
        let output_path = temp_dir.path().join("missing.pdf");
        let renderer = MockPdfRenderer::default();

        let exit_code = run_with_pdf_renderer(
            [
                "marknest",
                "convert",
                markdown_path.to_str().expect("path should be utf-8"),
                "-o",
                output_path.to_str().expect("path should be utf-8"),
            ],
            &renderer,
        );

        assert_eq!(exit_code, 1);
        assert!(output_path.exists());
        assert_eq!(
            renderer
                .requests
                .lock()
                .expect("requests mutex should lock")
                .len(),
            1
        );
    }

    #[test]
    fn convert_ignores_missing_assets_from_unselected_nested_readmes() {
        let temp_dir = TempDir::new().expect("temp dir should be created");
        let workspace_path = temp_dir.path().join("workspace");
        write_text_file(&workspace_path.join("README.md"), "# Root Guide\n");
        write_text_file(
            &workspace_path.join("nested/README.md"),
            "# Nested Guide\n\n![Missing](./missing.png)\n",
        );
        let output_path = temp_dir.path().join("root.pdf");
        let renderer = MockPdfRenderer::default();

        let exit_code = run_with_pdf_renderer(
            [
                "marknest",
                "convert",
                workspace_path
                    .join("README.md")
                    .to_str()
                    .expect("path should be utf-8"),
                "-o",
                output_path.to_str().expect("path should be utf-8"),
            ],
            &renderer,
        );

        assert_eq!(exit_code, 0);
        assert!(output_path.exists());
    }

    #[test]
    fn convert_fails_when_multiple_entries_are_detected_for_a_folder_input() {
        let renderer = MockPdfRenderer::default();
        let folder_path = fixtures_root().join("workspace_multiple_entries");

        let exit_code = run_with_pdf_renderer(
            [
                "marknest",
                "convert",
                folder_path.to_str().expect("path should be utf-8"),
            ],
            &renderer,
        );

        assert_eq!(exit_code, 2);
        assert_eq!(
            renderer
                .requests
                .lock()
                .expect("requests mutex should lock")
                .len(),
            0
        );
    }

    #[test]
    fn convert_passes_phase_3_options_to_the_pdf_renderer() {
        let temp_dir = TempDir::new().expect("temp dir should be created");
        let workspace_path = temp_dir.path().join("workspace");
        copy_directory(
            &fixtures_root().join("workspace_render_features"),
            &workspace_path,
        );
        let markdown_path = workspace_path.join("guide.md");
        let output_path = temp_dir.path().join("guide.pdf");
        let renderer = MockPdfRenderer::default();

        let exit_code = run_with_pdf_renderer(
            [
                "marknest",
                "convert",
                markdown_path.to_str().expect("path should be utf-8"),
                "--theme",
                "github",
                "--landscape",
                "--title",
                "Phase 3 Guide",
                "--author",
                "Docs Team",
                "--subject",
                "Rendering",
                "--mermaid",
                "auto",
                "--math",
                "auto",
                "-o",
                output_path.to_str().expect("path should be utf-8"),
            ],
            &renderer,
        );

        assert_eq!(exit_code, 0);

        let requests = renderer
            .requests
            .lock()
            .expect("requests mutex should lock");
        let request = requests
            .first()
            .expect("convert should invoke the renderer once");
        assert!(request.landscape);
        assert_eq!(request.metadata.title.as_deref(), Some("Phase 3 Guide"));
        assert_eq!(request.metadata.author.as_deref(), Some("Docs Team"));
        assert_eq!(request.metadata.subject.as_deref(), Some("Rendering"));
        assert!(request.html.contains("theme-github"));
        assert!(request.html.contains("\"mermaidMode\":\"auto\""));
        assert!(request.html.contains("\"mathMode\":\"auto\""));
    }

    #[test]
    fn convert_returns_a_warning_exit_code_when_runtime_rendering_warns() {
        let temp_dir = TempDir::new().expect("temp dir should be created");
        let workspace_path = temp_dir.path().join("workspace");
        copy_directory(
            &fixtures_root().join("workspace_render_features"),
            &workspace_path,
        );
        let markdown_path = workspace_path.join("guide.md");
        let renderer = OutcomePdfRenderer {
            outcome: PdfRenderOutcome {
                warnings: vec!["Mermaid rendering fell back to source.".to_string()],
            },
        };

        let exit_code = run_with_pdf_renderer(
            [
                "marknest",
                "convert",
                markdown_path.to_str().expect("path should be utf-8"),
                "--mermaid",
                "auto",
            ],
            &renderer,
        );

        assert_eq!(exit_code, 1);
    }

    #[test]
    fn convert_returns_a_validation_failure_when_runtime_rendering_is_required() {
        let temp_dir = TempDir::new().expect("temp dir should be created");
        let workspace_path = temp_dir.path().join("workspace");
        copy_directory(
            &fixtures_root().join("workspace_render_features"),
            &workspace_path,
        );
        let markdown_path = workspace_path.join("guide.md");
        let renderer = FailingPdfRenderer {
            failure: PdfRenderFailure::validation(
                "Mermaid rendering failed: diagram 1".to_string(),
            ),
        };

        let exit_code = run_with_pdf_renderer(
            [
                "marknest",
                "convert",
                markdown_path.to_str().expect("path should be utf-8"),
                "--mermaid",
                "on",
            ],
            &renderer,
        );

        assert_eq!(exit_code, 2);
    }

    #[test]
    fn apply_pdf_metadata_appends_author_and_subject_information() {
        let temp_dir = TempDir::new().expect("temp dir should be created");
        let pdf_path = temp_dir.path().join("document.pdf");
        fs::write(
            &pdf_path,
            b"%PDF-1.4\n1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n2 0 obj\n<< /Type /Pages /Count 0 >>\nendobj\nxref\n0 3\n0000000000 65535 f \n0000000009 00000 n \n0000000058 00000 n \ntrailer\n<< /Size 3 /Root 1 0 R >>\nstartxref\n101\n%%EOF\n",
        )
        .expect("sample pdf should be written");

        apply_pdf_metadata(
            &pdf_path,
            &PdfMetadata {
                title: Some("Phase 3 Guide".to_string()),
                author: Some("Docs Team".to_string()),
                subject: Some("Rendering".to_string()),
            },
        )
        .expect("metadata should be appended");

        let pdf_bytes = fs::read(&pdf_path).expect("pdf should be readable");
        let pdf_text = String::from_utf8_lossy(&pdf_bytes);
        assert!(pdf_text.contains("/Title (Phase 3 Guide)"));
        assert!(pdf_text.contains("/Author (Docs Team)"));
        assert!(pdf_text.contains("/Subject (Rendering)"));
        assert!(pdf_text.contains("/Info 3 0 R"));
        assert!(pdf_text.contains("/Prev 101"));
    }

    #[test]
    fn render_html_to_pdf_bytes_reads_the_generated_pdf() {
        let renderer = OutcomePdfRenderer {
            outcome: PdfRenderOutcome {
                warnings: vec!["Runtime warning".to_string()],
            },
        };

        let pdf = render_html_to_pdf_bytes_with_renderer(
            &HtmlToPdfRequest {
                title: "Guide".to_string(),
                html: "<html><body><h1>Guide</h1></body></html>".to_string(),
                page_size: PdfPageSize::A4,
                margins_mm: PdfMarginsMm::uniform(0.0),
                landscape: false,
                metadata: PdfMetadata {
                    title: Some("Guide".to_string()),
                    author: Some("Docs".to_string()),
                    subject: None,
                },
                header_template: None,
                footer_template: None,
            },
            &renderer,
        )
        .expect("pdf bytes should render");

        let pdf_text = String::from_utf8_lossy(&pdf.bytes);
        assert!(pdf_text.contains("%PDF-1.4"));
        assert!(pdf_text.contains("/Title (Guide)"));
        assert!(pdf_text.contains("/Author (Docs)"));
        assert_eq!(pdf.warnings, vec!["Runtime warning".to_string()]);
    }

    #[test]
    fn render_html_to_pdf_bytes_preserves_renderer_failure_kind() {
        let error = render_html_to_pdf_bytes_with_renderer(
            &HtmlToPdfRequest {
                title: "Guide".to_string(),
                html: "<html></html>".to_string(),
                page_size: PdfPageSize::A4,
                margins_mm: PdfMarginsMm::uniform(0.0),
                landscape: false,
                metadata: PdfMetadata::default(),
                header_template: None,
                footer_template: None,
            },
            &FailingPdfRenderer {
                failure: PdfRenderFailure::validation("browser failed".to_string()),
            },
        )
        .expect_err("renderer failure should surface");

        assert_eq!(error.kind, HtmlToPdfErrorKind::Validation);
        assert_eq!(error.message, "browser failed");
    }

    struct OutcomePdfRenderer {
        outcome: PdfRenderOutcome,
    }

    impl PdfRenderer for OutcomePdfRenderer {
        fn render(&self, request: &PdfRenderRequest) -> Result<PdfRenderOutcome, PdfRenderFailure> {
            if let Some(parent) = request.output_path.parent() {
                fs::create_dir_all(parent).expect("output directory should be created");
            }
            fs::write(&request.output_path, sample_pdf_bytes())
                .expect("mock renderer should write a pdf");
            Ok(self.outcome.clone())
        }
    }

    struct FailingPdfRenderer {
        failure: PdfRenderFailure,
    }

    impl PdfRenderer for FailingPdfRenderer {
        fn render(
            &self,
            _request: &PdfRenderRequest,
        ) -> Result<PdfRenderOutcome, PdfRenderFailure> {
            Err(self.failure.clone())
        }
    }

    fn sample_pdf_bytes() -> &'static [u8] {
        b"%PDF-1.4\n1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n2 0 obj\n<< /Type /Pages /Count 0 >>\nendobj\nxref\n0 3\n0000000000 65535 f \n0000000009 00000 n \n0000000058 00000 n \ntrailer\n<< /Size 3 /Root 1 0 R >>\nstartxref\n101\n%%EOF\n"
    }

    static ENVIRONMENT_MUTEX: Mutex<()> = Mutex::new(());

    #[test]
    fn convert_prefers_cli_then_config_then_environment_defaults() {
        let _environment_guard = ENVIRONMENT_MUTEX
            .lock()
            .expect("environment mutex should lock");
        let temp_dir = TempDir::new().expect("temp dir should be created");
        let workspace_path = temp_dir.path().join("workspace");
        copy_directory(
            &fixtures_root().join("workspace_raw_html_sanitize"),
            &workspace_path,
        );

        write_text_file(
            &workspace_path.join(".marknest.toml"),
            "[convert]\ntheme = \"docs\"\ncss = \"./config.css\"\nsanitize_html = false\nmermaid_timeout_ms = 4200\nmath_timeout_ms = 2400\n",
        );
        write_text_file(
            &workspace_path.join("config.css"),
            "body { color: rgb(12, 34, 56); }",
        );
        write_text_file(
            &workspace_path.join("env.css"),
            "body { color: rgb(77, 88, 99); }",
        );
        write_text_file(
            &workspace_path.join("cli.css"),
            "body { color: rgb(7, 8, 9); }",
        );

        let original_theme = env::var_os("MARKNEST_THEME");
        let original_css = env::var_os("MARKNEST_CSS");
        let original_sanitize_html = env::var_os("MARKNEST_SANITIZE_HTML");
        let original_mermaid_timeout = env::var_os("MARKNEST_MERMAID_TIMEOUT_MS");
        let original_math_timeout = env::var_os("MARKNEST_MATH_TIMEOUT_MS");
        let original_directory = env::current_dir().expect("cwd should be readable");
        let renderer = MockPdfRenderer::default();

        unsafe {
            env::set_var("MARKNEST_THEME", "plain");
            env::set_var(
                "MARKNEST_CSS",
                workspace_path
                    .join("env.css")
                    .to_str()
                    .expect("path should be utf-8"),
            );
            env::set_var("MARKNEST_SANITIZE_HTML", "true");
            env::set_var("MARKNEST_MERMAID_TIMEOUT_MS", "6100");
            env::set_var("MARKNEST_MATH_TIMEOUT_MS", "4100");
        }
        env::set_current_dir(&workspace_path).expect("cwd should change");

        let config_exit_code =
            run_with_pdf_renderer(["marknest", "convert", "README.md"], &renderer);
        assert_eq!(config_exit_code, 0);

        let cli_exit_code = run_with_pdf_renderer(
            [
                "marknest",
                "convert",
                "README.md",
                "--theme",
                "github",
                "--css",
                "cli.css",
                "--sanitize-html",
                "--mermaid-timeout-ms",
                "7300",
                "--math-timeout-ms",
                "5100",
            ],
            &renderer,
        );

        env::set_current_dir(&original_directory).expect("cwd should be restored");
        restore_env_var("MARKNEST_THEME", original_theme);
        restore_env_var("MARKNEST_CSS", original_css);
        restore_env_var("MARKNEST_SANITIZE_HTML", original_sanitize_html);
        restore_env_var("MARKNEST_MERMAID_TIMEOUT_MS", original_mermaid_timeout);
        restore_env_var("MARKNEST_MATH_TIMEOUT_MS", original_math_timeout);

        assert_eq!(cli_exit_code, 0);

        let requests = renderer
            .requests
            .lock()
            .expect("requests mutex should lock");
        let config_request = &requests[0];
        assert!(config_request.html.contains("theme-docs"));
        assert!(config_request.html.contains("rgb(12, 34, 56)"));
        assert!(
            config_request
                .html
                .contains("<script>alert(\"x\")</script>")
        );
        assert!(!config_request.html.contains("rgb(77, 88, 99)"));
        assert!(config_request.html.contains("\"mermaidTimeoutMs\":4200"));
        assert!(config_request.html.contains("\"mathTimeoutMs\":2400"));

        let cli_request = &requests[1];
        assert!(cli_request.html.contains("theme-github"));
        assert!(cli_request.html.contains("rgb(7, 8, 9)"));
        assert!(!cli_request.html.contains("<script>alert(\"x\")</script>"));
        assert!(!cli_request.html.contains("rgb(12, 34, 56)"));
        assert!(cli_request.html.contains("\"mermaidTimeoutMs\":7300"));
        assert!(cli_request.html.contains("\"mathTimeoutMs\":5100"));
    }

    #[test]
    fn convert_inserts_a_generated_table_of_contents_when_requested() {
        let temp_dir = TempDir::new().expect("temp dir should be created");
        let workspace_path = temp_dir.path().join("workspace");
        copy_directory(&fixtures_root().join("workspace_toc"), &workspace_path);
        let markdown_path = workspace_path.join("README.md");
        let renderer = MockPdfRenderer::default();

        let exit_code = run_with_pdf_renderer(
            [
                "marknest",
                "convert",
                markdown_path.to_str().expect("path should be utf-8"),
                "--toc",
            ],
            &renderer,
        );

        assert_eq!(exit_code, 0);

        let requests = renderer
            .requests
            .lock()
            .expect("requests mutex should lock");
        let request = requests
            .first()
            .expect("convert should invoke the renderer once");
        assert!(request.html.contains("marknest-toc"));
        assert!(request.html.contains("href=\"#guide\""));
        assert!(request.html.contains("href=\"#overview-2\""));
    }

    #[test]
    fn convert_writes_debug_artifacts_and_passes_print_templates_to_the_renderer() {
        let temp_dir = TempDir::new().expect("temp dir should be created");
        let workspace_path = temp_dir.path().join("workspace");
        copy_directory(&fixtures_root().join("workspace_valid"), &workspace_path);
        let markdown_path = workspace_path.join("README.md");
        let output_path = temp_dir.path().join("README.pdf");
        let debug_html_path = temp_dir.path().join("debug.html");
        let asset_manifest_path = temp_dir.path().join("assets.json");
        let render_report_path = temp_dir.path().join("report.json");
        let css_path = temp_dir.path().join("pdf.css");
        let header_path = temp_dir.path().join("header.html");
        let footer_path = temp_dir.path().join("footer.html");
        write_text_file(&css_path, "body { color: rgb(5, 4, 3); }");
        write_text_file(&header_path, "<div>Header {{title}} / {{entryPath}}</div>");
        write_text_file(
            &footer_path,
            "<div>{{pageNumber}} / {{totalPages}} / {{date}}</div>",
        );
        let renderer = MockPdfRenderer::default();

        let exit_code = run_with_pdf_renderer(
            [
                "marknest",
                "convert",
                markdown_path.to_str().expect("path should be utf-8"),
                "--css",
                css_path.to_str().expect("path should be utf-8"),
                "--header-template",
                header_path.to_str().expect("path should be utf-8"),
                "--footer-template",
                footer_path.to_str().expect("path should be utf-8"),
                "--debug-html",
                debug_html_path.to_str().expect("path should be utf-8"),
                "--asset-manifest",
                asset_manifest_path.to_str().expect("path should be utf-8"),
                "--render-report",
                render_report_path.to_str().expect("path should be utf-8"),
                "-o",
                output_path.to_str().expect("path should be utf-8"),
            ],
            &renderer,
        );

        assert_eq!(exit_code, 0);
        assert!(debug_html_path.exists());
        assert!(asset_manifest_path.exists());
        assert!(render_report_path.exists());

        let debug_html = fs::read_to_string(&debug_html_path).expect("debug html should exist");
        assert!(debug_html.contains("rgb(5, 4, 3)"));

        let asset_manifest: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(&asset_manifest_path).expect("asset manifest should exist"),
        )
        .expect("asset manifest should be valid json");
        assert_eq!(asset_manifest["entry_path"], "README.md");
        assert!(
            asset_manifest["assets"]
                .as_array()
                .expect("assets should be an array")
                .iter()
                .any(|asset| asset["resolved_path"] == "images/architecture.svg")
        );

        let report_json: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(&render_report_path).expect("report should be readable"),
        )
        .expect("report should be valid json");
        assert_eq!(
            report_json["runtime_info"]["renderer"],
            "playwright-chromium"
        );
        assert_eq!(report_json["runtime_info"]["playwright_version"], "1.58.2");
        assert!(
            report_json["runtime_info"]["mermaid_script_url"]
                .as_str()
                .expect("mermaid url should exist")
                .contains("./runtime-assets/mermaid/mermaid.min.js")
        );
        assert_eq!(report_json["runtime_info"]["asset_mode"], "bundled_local");
        assert_eq!(report_json["runtime_info"]["mermaid_version"], "11.11.0");
        assert_eq!(report_json["runtime_info"]["mathjax_version"], "3.2.2");

        let requests = renderer
            .requests
            .lock()
            .expect("requests mutex should lock");
        let request = requests
            .first()
            .expect("convert should invoke the renderer once");
        assert!(request.html.contains("rgb(5, 4, 3)"));
        assert!(
            request
                .header_template
                .as_deref()
                .expect("header template should exist")
                .contains("README")
        );
        assert!(
            request
                .header_template
                .as_deref()
                .expect("header template should exist")
                .contains("README.md")
        );
        assert!(
            request
                .footer_template
                .as_deref()
                .expect("footer template should exist")
                .contains("class=\"pageNumber\"")
        );
        assert!(
            request
                .footer_template
                .as_deref()
                .expect("footer template should exist")
                .contains("class=\"totalPages\"")
        );
    }

    #[test]
    fn convert_writes_remote_asset_results_into_debug_artifacts() {
        let server = TestHttpServer::start(vec![(
            "/diagram.png",
            TestHttpResponse::ok_png(b"\x89PNG\r\n\x1a\nremote-diagram"),
        )]);
        let temp_dir = TempDir::new().expect("temp dir should be created");
        let workspace_path = temp_dir.path().join("workspace");
        let output_path = temp_dir.path().join("README.pdf");
        let debug_html_path = temp_dir.path().join("debug.html");
        let asset_manifest_path = temp_dir.path().join("assets.json");
        let render_report_path = temp_dir.path().join("report.json");
        write_text_file(
            &workspace_path.join("README.md"),
            &format!("# Remote\n\n![Diagram]({})\n", server.url("/diagram.png")),
        );
        let markdown_path = workspace_path.join("README.md");
        let renderer = MockPdfRenderer::default();

        let exit_code = run_with_pdf_renderer(
            [
                "marknest",
                "convert",
                markdown_path.to_str().expect("path should be utf-8"),
                "--debug-html",
                debug_html_path.to_str().expect("path should be utf-8"),
                "--asset-manifest",
                asset_manifest_path.to_str().expect("path should be utf-8"),
                "--render-report",
                render_report_path.to_str().expect("path should be utf-8"),
                "-o",
                output_path.to_str().expect("path should be utf-8"),
            ],
            &renderer,
        );

        assert_eq!(exit_code, 0);

        let debug_html = fs::read_to_string(&debug_html_path).expect("debug html should exist");
        assert!(debug_html.contains("data:image/png;base64,"));

        let asset_manifest: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(&asset_manifest_path).expect("asset manifest should exist"),
        )
        .expect("asset manifest should be valid json");
        assert_eq!(asset_manifest["remote_assets"][0]["status"], "inlined");
        assert_eq!(
            asset_manifest["remote_assets"][0]["fetch_url"],
            server.url("/diagram.png")
        );

        let report_json: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(&render_report_path).expect("report should be readable"),
        )
        .expect("report should be valid json");
        assert_eq!(report_json["remote_assets"][0]["status"], "inlined");
        assert!(
            report_json["warnings"]
                .as_array()
                .expect("warnings should be an array")
                .is_empty()
        );
    }

    #[test]
    fn convert_writes_runtime_assets_next_to_debug_html_when_mermaid_or_math_is_enabled() {
        let temp_dir = TempDir::new().expect("temp dir should be created");
        let workspace_path = temp_dir.path().join("workspace");
        copy_directory(
            &fixtures_root().join("workspace_render_features"),
            &workspace_path,
        );
        let markdown_path = workspace_path.join("guide.md");
        let output_path = temp_dir.path().join("guide.pdf");
        let debug_html_path = temp_dir.path().join("debug.html");
        let renderer = MockPdfRenderer::default();

        let exit_code = run_with_pdf_renderer(
            [
                "marknest",
                "convert",
                markdown_path.to_str().expect("path should be utf-8"),
                "--mermaid",
                "auto",
                "--math",
                "auto",
                "--debug-html",
                debug_html_path.to_str().expect("path should be utf-8"),
                "-o",
                output_path.to_str().expect("path should be utf-8"),
            ],
            &renderer,
        );

        assert_eq!(exit_code, 0);
        assert!(
            temp_dir
                .path()
                .join("runtime-assets")
                .join("mermaid")
                .join("mermaid.min.js")
                .exists()
        );
        assert!(
            temp_dir
                .path()
                .join("runtime-assets")
                .join("mathjax")
                .join("es5")
                .join("tex-svg.js")
                .exists()
        );
    }

    #[test]
    fn convert_rejects_scriptable_header_templates() {
        let temp_dir = TempDir::new().expect("temp dir should be created");
        let workspace_path = temp_dir.path().join("workspace");
        copy_directory(&fixtures_root().join("workspace_valid"), &workspace_path);
        let markdown_path = workspace_path.join("README.md");
        let header_path = temp_dir.path().join("header.html");
        write_text_file(&header_path, "<script>alert('x')</script>");
        let renderer = MockPdfRenderer::default();

        let exit_code = run_with_pdf_renderer(
            [
                "marknest",
                "convert",
                markdown_path.to_str().expect("path should be utf-8"),
                "--header-template",
                header_path.to_str().expect("path should be utf-8"),
            ],
            &renderer,
        );

        assert_eq!(exit_code, 2);
        assert_eq!(
            renderer
                .requests
                .lock()
                .expect("requests mutex should lock")
                .len(),
            0
        );
    }

    #[test]
    fn resolve_playwright_runtime_dir_prefers_environment_override() {
        let _environment_guard = ENVIRONMENT_MUTEX
            .lock()
            .expect("environment mutex should lock");
        let temp_dir = TempDir::new().expect("temp dir should be created");
        let original_runtime_dir = env::var_os("MARKNEST_PLAYWRIGHT_RUNTIME_DIR");

        unsafe {
            env::set_var("MARKNEST_PLAYWRIGHT_RUNTIME_DIR", temp_dir.path());
        }

        assert_eq!(resolve_playwright_runtime_dir(), temp_dir.path());

        restore_env_var("MARKNEST_PLAYWRIGHT_RUNTIME_DIR", original_runtime_dir);
    }

    #[test]
    fn browser_candidate_paths_include_mac_and_linux_locations() {
        let candidates = browser_candidate_paths();

        assert!(candidates.contains(&PathBuf::from(
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
        )));
        assert!(candidates.contains(&PathBuf::from("/usr/bin/chromium")));
        assert!(candidates.contains(&PathBuf::from("/usr/bin/google-chrome")));
    }

    fn restore_env_var(key: &str, value: Option<OsString>) {
        match value {
            Some(value) => unsafe { env::set_var(key, value) },
            None => unsafe { env::remove_var(key) },
        }
    }

    // --- GitHub URL resolve_input tests ---

    #[test]
    fn resolve_input_returns_github_url_variant_for_github_urls() {
        let path = Path::new("https://github.com/user/repo");
        let result = resolve_input(Some(path)).expect("should resolve GitHub URL");
        match result {
            ResolvedInput::GitHubUrl {
                display_path,
                parsed,
            } => {
                assert_eq!(display_path, "https://github.com/user/repo");
                assert_eq!(parsed.owner, "user");
                assert_eq!(parsed.repo, "repo");
            }
            _ => panic!("expected GitHubUrl variant"),
        }
    }

    #[test]
    fn resolve_input_returns_local_type_for_non_url_paths() {
        let temp_dir = TempDir::new().expect("temp dir");
        let md_path = temp_dir.path().join("test.md");
        fs::write(&md_path, "# Test").expect("write");

        let result = resolve_input(Some(&md_path)).expect("should resolve markdown file");
        match result {
            ResolvedInput::MarkdownFile { .. } => {}
            _ => panic!("expected MarkdownFile variant"),
        }
    }

    // --- GitHub auth token resolution tests ---
    // Combined into one test to avoid env var race conditions in parallel test execution

    #[test]
    fn resolves_github_auth_token_from_environment() {
        let original_github = env::var_os("GITHUB_TOKEN");
        let original_gh = env::var_os("GH_TOKEN");

        // GITHUB_TOKEN takes priority
        unsafe {
            env::set_var("GITHUB_TOKEN", "token-from-github");
            env::remove_var("GH_TOKEN");
        }
        assert_eq!(
            resolve_github_auth_token(),
            Some("token-from-github".to_string())
        );

        // Falls back to GH_TOKEN
        unsafe {
            env::remove_var("GITHUB_TOKEN");
            env::set_var("GH_TOKEN", "token-from-gh");
        }
        assert_eq!(
            resolve_github_auth_token(),
            Some("token-from-gh".to_string())
        );

        // Returns None when neither is set
        unsafe {
            env::remove_var("GITHUB_TOKEN");
            env::remove_var("GH_TOKEN");
        }
        assert_eq!(resolve_github_auth_token(), None);

        // Ignores empty values
        unsafe {
            env::set_var("GITHUB_TOKEN", "");
            env::set_var("GH_TOKEN", "");
        }
        assert_eq!(resolve_github_auth_token(), None);

        restore_env_var("GITHUB_TOKEN", original_github);
        restore_env_var("GH_TOKEN", original_gh);
    }

    // --- GitHub URL parsing tests ---

    #[test]
    fn parses_bare_github_repo_url() {
        let result = parse_github_url("https://github.com/user/repo");
        assert_eq!(
            result,
            Some(ParsedGitHubUrl {
                owner: "user".to_string(),
                repo: "repo".to_string(),
                git_ref: None,
                subpath: None,
                is_file_reference: false,
            })
        );
    }

    #[test]
    fn parses_github_tree_url_with_branch() {
        let result = parse_github_url("https://github.com/user/repo/tree/main");
        assert_eq!(
            result,
            Some(ParsedGitHubUrl {
                owner: "user".to_string(),
                repo: "repo".to_string(),
                git_ref: Some("main".to_string()),
                subpath: None,
                is_file_reference: false,
            })
        );
    }

    #[test]
    fn parses_github_blob_url_with_file_path() {
        let result = parse_github_url("https://github.com/user/repo/blob/main/docs/guide.md");
        assert_eq!(
            result,
            Some(ParsedGitHubUrl {
                owner: "user".to_string(),
                repo: "repo".to_string(),
                git_ref: Some("main".to_string()),
                subpath: Some("docs/guide.md".to_string()),
                is_file_reference: true,
            })
        );
    }

    #[test]
    fn parses_github_tree_url_with_tag_and_directory() {
        let result = parse_github_url("https://github.com/user/repo/tree/v2.0/src");
        assert_eq!(
            result,
            Some(ParsedGitHubUrl {
                owner: "user".to_string(),
                repo: "repo".to_string(),
                git_ref: Some("v2.0".to_string()),
                subpath: Some("src".to_string()),
                is_file_reference: false,
            })
        );
    }

    #[test]
    fn parses_github_url_with_dot_git_suffix() {
        let result = parse_github_url("https://github.com/user/repo.git");
        assert_eq!(
            result,
            Some(ParsedGitHubUrl {
                owner: "user".to_string(),
                repo: "repo".to_string(),
                git_ref: None,
                subpath: None,
                is_file_reference: false,
            })
        );
    }

    #[test]
    fn parses_http_github_url() {
        let result = parse_github_url("http://github.com/user/repo");
        assert_eq!(
            result,
            Some(ParsedGitHubUrl {
                owner: "user".to_string(),
                repo: "repo".to_string(),
                git_ref: None,
                subpath: None,
                is_file_reference: false,
            })
        );
    }

    #[test]
    fn rejects_non_github_url() {
        assert_eq!(parse_github_url("https://gitlab.com/user/repo"), None);
    }

    #[test]
    fn rejects_malformed_github_url_missing_repo() {
        assert_eq!(parse_github_url("https://github.com/user"), None);
    }

    #[test]
    fn rejects_non_url_input() {
        assert_eq!(parse_github_url("README.md"), None);
        assert_eq!(parse_github_url("./docs.zip"), None);
        assert_eq!(parse_github_url("/some/path"), None);
    }

    #[test]
    fn parses_github_url_with_trailing_slash() {
        let result = parse_github_url("https://github.com/user/repo/");
        assert_eq!(
            result,
            Some(ParsedGitHubUrl {
                owner: "user".to_string(),
                repo: "repo".to_string(),
                git_ref: None,
                subpath: None,
                is_file_reference: false,
            })
        );
    }
}
