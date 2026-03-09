use std::collections::HashSet;
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Component, Path, PathBuf};

use ammonia::{Builder as HtmlSanitizerBuilder, UrlRelative};
use pulldown_cmark::{
    CowStr, Event, HeadingLevel, Options as MarkdownOptions, Parser, Tag, TagEnd, html,
};
use serde::{Deserialize, Serialize};

const MAX_ZIP_FILE_COUNT: usize = 4_096;
const MAX_ZIP_UNCOMPRESSED_BYTES: u64 = 256 * 1024 * 1024;
pub const RUNTIME_ASSET_MODE: &str = "bundled_local";
pub const DEFAULT_MERMAID_TIMEOUT_MS: u32 = 5_000;
pub const DEFAULT_MATH_TIMEOUT_MS: u32 = 3_000;
pub const MERMAID_VERSION: &str = "11.11.0";
pub const MATHJAX_VERSION: &str = "3.2.2";
pub const MERMAID_SCRIPT_URL: &str = "./runtime-assets/mermaid/mermaid.min.js";
pub const MATHJAX_SCRIPT_URL: &str = "./runtime-assets/mathjax/es5/tex-svg.js";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectSourceKind {
    Workspace,
    Zip,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EntrySelectionReason {
    Readme,
    Index,
    SingleMarkdownFile,
    MultipleCandidates,
    NoMarkdownFiles,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EntryCandidate {
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetReferenceKind {
    MarkdownImage,
    RawHtmlImage,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetStatus {
    Resolved,
    Missing,
    External,
    UnsupportedScheme,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AssetRef {
    pub entry_path: String,
    pub original_reference: String,
    pub resolved_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fetch_url: Option<String>,
    pub kind: AssetReferenceKind,
    pub status: AssetStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct Diagnostic {
    pub missing_assets: Vec<String>,
    pub ignored_files: Vec<String>,
    pub warnings: Vec<String>,
    pub path_errors: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProjectIndex {
    pub source_kind: ProjectSourceKind,
    pub selected_entry: Option<String>,
    pub entry_selection_reason: EntrySelectionReason,
    pub entry_candidates: Vec<EntryCandidate>,
    pub assets: Vec<AssetRef>,
    pub diagnostic: Diagnostic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ThemePreset {
    #[default]
    Default,
    Github,
    Docs,
    Plain,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MermaidMode {
    Off,
    #[default]
    Auto,
    On,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MathMode {
    Off,
    #[default]
    Auto,
    On,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PdfMetadata {
    pub title: Option<String>,
    pub author: Option<String>,
    pub subject: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderOptions {
    pub theme: ThemePreset,
    pub metadata: PdfMetadata,
    pub custom_css: Option<String>,
    pub enable_toc: bool,
    pub sanitize_html: bool,
    pub mermaid_mode: MermaidMode,
    pub math_mode: MathMode,
    pub mermaid_timeout_ms: u32,
    pub math_timeout_ms: u32,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            theme: ThemePreset::Default,
            metadata: PdfMetadata::default(),
            custom_css: None,
            enable_toc: false,
            sanitize_html: true,
            mermaid_mode: MermaidMode::Off,
            math_mode: MathMode::Off,
            mermaid_timeout_ms: DEFAULT_MERMAID_TIMEOUT_MS,
            math_timeout_ms: DEFAULT_MATH_TIMEOUT_MS,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnalyzeError {
    UnsafePath { path: String },
    ZipArchive(String),
    Io(String),
    ZipLimitsExceeded(String),
}

impl std::fmt::Display for AnalyzeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsafePath { path } => write!(f, "unsafe path detected: {path}"),
            Self::ZipArchive(message) => write!(f, "zip archive error: {message}"),
            Self::Io(message) => write!(f, "i/o error: {message}"),
            Self::ZipLimitsExceeded(message) => write!(f, "zip limits exceeded: {message}"),
        }
    }
}

impl std::error::Error for AnalyzeError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedHtmlDocument {
    pub title: String,
    pub html: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderHtmlError {
    EntryNotFound { entry_path: String },
    InvalidEntryPath { entry_path: String },
    Analyze(AnalyzeError),
    Io(String),
    InvalidUtf8 { entry_path: String },
}

impl std::fmt::Display for RenderHtmlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EntryNotFound { entry_path } => {
                write!(f, "entry markdown file could not be found: {entry_path}")
            }
            Self::InvalidEntryPath { entry_path } => {
                write!(f, "entry markdown file path is invalid: {entry_path}")
            }
            Self::Analyze(error) => write!(f, "{error}"),
            Self::Io(message) => write!(f, "i/o error: {message}"),
            Self::InvalidUtf8 { entry_path } => {
                write!(f, "entry markdown file is not valid UTF-8: {entry_path}")
            }
        }
    }
}

impl std::error::Error for RenderHtmlError {}

pub fn render_workspace_entry(
    root: &Path,
    entry_path: &str,
) -> Result<RenderedHtmlDocument, RenderHtmlError> {
    render_workspace_entry_with_options(root, entry_path, &RenderOptions::default())
}

pub fn render_zip_entry(
    bytes: &[u8],
    entry_path: &str,
) -> Result<RenderedHtmlDocument, RenderHtmlError> {
    render_zip_entry_with_options(bytes, entry_path, &RenderOptions::default())
}

pub fn render_workspace_entry_with_options(
    root: &Path,
    entry_path: &str,
    options: &RenderOptions,
) -> Result<RenderedHtmlDocument, RenderHtmlError> {
    let workspace_file_system = WorkspaceFileSystem::new(root).map_err(RenderHtmlError::Analyze)?;
    render_entry_with_options(&workspace_file_system, entry_path, options)
}

pub fn render_zip_entry_with_options(
    bytes: &[u8],
    entry_path: &str,
    options: &RenderOptions,
) -> Result<RenderedHtmlDocument, RenderHtmlError> {
    let zip_file_system = ZipMemoryFileSystem::new(bytes).map_err(RenderHtmlError::Analyze)?;
    render_entry_with_options(&zip_file_system, entry_path, options)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RenderedHeading {
    level: HeadingLevel,
    id: String,
    title: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RenderedMarkdownBody {
    html: String,
    headings: Vec<RenderedHeading>,
}

#[derive(Debug)]
struct PendingHeading<'a> {
    level: HeadingLevel,
    id: Option<CowStr<'a>>,
    classes: Vec<CowStr<'a>>,
    attrs: Vec<(CowStr<'a>, Option<CowStr<'a>>)>,
    events: Vec<Event<'a>>,
    title: String,
}

pub fn analyze_workspace(root: &Path) -> Result<ProjectIndex, AnalyzeError> {
    analyze_project(&WorkspaceFileSystem::new(root)?)
}

pub fn analyze_zip(bytes: &[u8]) -> Result<ProjectIndex, AnalyzeError> {
    analyze_project(&ZipMemoryFileSystem::new(bytes)?)
}

fn remote_fetch_url(reference: &str) -> Option<String> {
    if !is_http_reference(reference) {
        return None;
    }

    Some(normalize_remote_fetch_url(reference))
}

fn normalize_remote_fetch_url(reference: &str) -> String {
    normalize_github_repository_image_url(reference).unwrap_or_else(|| reference.trim().to_string())
}

fn normalize_github_repository_image_url(reference: &str) -> Option<String> {
    let trimmed_reference: &str = reference.trim();
    let (scheme, remainder) = if let Some(value) = trimmed_reference.strip_prefix("https://") {
        ("https://", value)
    } else if let Some(value) = trimmed_reference.strip_prefix("http://") {
        ("http://", value)
    } else {
        return None;
    };

    let host_path_separator: usize = remainder.find('/')?;
    let host: &str = &remainder[..host_path_separator];
    if !host.eq_ignore_ascii_case("github.com") {
        return None;
    }

    let path: &str = strip_reference_query_and_fragment(&remainder[host_path_separator..]);
    let segments: Vec<&str> = path.trim_start_matches('/').split('/').collect();
    if segments.len() < 5 {
        return None;
    }

    if segments[2] == "blob" {
        return Some(format!(
            "{scheme}{host}/{}/{}/raw/{}",
            segments[0],
            segments[1],
            segments[3..].join("/")
        ));
    }

    if segments[2] == "raw" {
        return Some(format!("{scheme}{host}{path}"));
    }

    None
}

trait IndexedFileSystem {
    fn source_kind(&self) -> ProjectSourceKind;
    fn files(&self) -> &[IndexedFile];

    fn file_contents(&self, normalized_path: &str) -> Option<&[u8]> {
        self.files()
            .iter()
            .find(|file| file.normalized_path == normalized_path)
            .map(|file| file.contents.as_slice())
    }
}

#[derive(Debug, Clone)]
struct IndexedFile {
    normalized_path: String,
    contents: Vec<u8>,
}

struct WorkspaceFileSystem {
    files: Vec<IndexedFile>,
}

impl WorkspaceFileSystem {
    fn new(root: &Path) -> Result<Self, AnalyzeError> {
        if !root.exists() {
            return Err(AnalyzeError::Io(format!(
                "workspace root does not exist: {}",
                root.display()
            )));
        }

        let canonical_root: PathBuf = root
            .canonicalize()
            .map_err(|error| AnalyzeError::Io(error.to_string()))?;
        let mut files: Vec<IndexedFile> = Vec::new();
        collect_workspace_files(&canonical_root, &canonical_root, &mut files)?;
        files.sort_by(|left, right| left.normalized_path.cmp(&right.normalized_path));
        Ok(Self { files })
    }
}

impl IndexedFileSystem for WorkspaceFileSystem {
    fn source_kind(&self) -> ProjectSourceKind {
        ProjectSourceKind::Workspace
    }

    fn files(&self) -> &[IndexedFile] {
        &self.files
    }
}

struct ZipMemoryFileSystem {
    files: Vec<IndexedFile>,
}

impl ZipMemoryFileSystem {
    fn new(bytes: &[u8]) -> Result<Self, AnalyzeError> {
        let reader: Cursor<&[u8]> = Cursor::new(bytes);
        let mut archive = zip::ZipArchive::new(reader)
            .map_err(|error| AnalyzeError::ZipArchive(error.to_string()))?;
        let mut files: Vec<IndexedFile> = Vec::new();
        let mut total_uncompressed_bytes: u64 = 0;

        if archive.len() > MAX_ZIP_FILE_COUNT {
            return Err(AnalyzeError::ZipLimitsExceeded(format!(
                "archive contains {} files, limit is {}",
                archive.len(),
                MAX_ZIP_FILE_COUNT
            )));
        }

        for index in 0..archive.len() {
            let mut entry = archive
                .by_index(index)
                .map_err(|error| AnalyzeError::ZipArchive(error.to_string()))?;

            if entry.is_dir() {
                continue;
            }

            let raw_path: String = entry.name().to_string();
            let normalized_path: String =
                normalize_relative_string(&raw_path).map_err(|_| AnalyzeError::UnsafePath {
                    path: raw_path.clone(),
                })?;

            total_uncompressed_bytes = total_uncompressed_bytes
                .checked_add(entry.size())
                .ok_or_else(|| {
                    AnalyzeError::ZipLimitsExceeded(
                        "archive uncompressed size overflowed the configured limit".to_string(),
                    )
                })?;

            if total_uncompressed_bytes > MAX_ZIP_UNCOMPRESSED_BYTES {
                return Err(AnalyzeError::ZipLimitsExceeded(format!(
                    "archive expands to {} bytes, limit is {} bytes",
                    total_uncompressed_bytes, MAX_ZIP_UNCOMPRESSED_BYTES
                )));
            }

            let mut contents: Vec<u8> = Vec::new();
            entry
                .read_to_end(&mut contents)
                .map_err(|error| AnalyzeError::ZipArchive(error.to_string()))?;

            files.push(IndexedFile {
                normalized_path,
                contents,
            });
        }

        files.sort_by(|left, right| left.normalized_path.cmp(&right.normalized_path));
        Ok(Self { files })
    }
}

impl IndexedFileSystem for ZipMemoryFileSystem {
    fn source_kind(&self) -> ProjectSourceKind {
        ProjectSourceKind::Zip
    }

    fn files(&self) -> &[IndexedFile] {
        &self.files
    }
}

fn analyze_project(file_system: &dyn IndexedFileSystem) -> Result<ProjectIndex, AnalyzeError> {
    let mut diagnostic: Diagnostic = Diagnostic::default();
    let mut markdown_files: Vec<&IndexedFile> = Vec::new();
    let mut known_paths: Vec<&str> = Vec::new();

    for indexed_file in file_system.files() {
        known_paths.push(indexed_file.normalized_path.as_str());

        if is_markdown_path(&indexed_file.normalized_path) {
            markdown_files.push(indexed_file);
        } else if !is_supported_image_path(&indexed_file.normalized_path) {
            diagnostic
                .ignored_files
                .push(indexed_file.normalized_path.clone());
        }
    }

    known_paths.sort_unstable();
    diagnostic.ignored_files.sort_unstable();

    let entry_candidates: Vec<EntryCandidate> = markdown_files
        .iter()
        .map(|file| EntryCandidate {
            path: file.normalized_path.clone(),
        })
        .collect();

    let (selected_entry, entry_selection_reason) = select_entry(&entry_candidates);
    let assets: Vec<AssetRef> = collect_assets(&markdown_files, &known_paths, &mut diagnostic);

    Ok(ProjectIndex {
        source_kind: file_system.source_kind(),
        selected_entry,
        entry_selection_reason,
        entry_candidates,
        assets,
        diagnostic,
    })
}

fn collect_workspace_files(
    root: &Path,
    directory: &Path,
    files: &mut Vec<IndexedFile>,
) -> Result<(), AnalyzeError> {
    let mut entries: Vec<fs::DirEntry> = fs::read_dir(directory)
        .map_err(|error| AnalyzeError::Io(error.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| AnalyzeError::Io(error.to_string()))?;

    entries.sort_by_key(|entry| entry.file_name().to_string_lossy().to_string());

    for entry in entries {
        let file_type: fs::FileType = entry
            .file_type()
            .map_err(|error| AnalyzeError::Io(error.to_string()))?;

        if file_type.is_dir() {
            let name: String = entry.file_name().to_string_lossy().to_string();
            if should_skip_directory(&name) {
                continue;
            }

            collect_workspace_files(root, &entry.path(), files)?;
            continue;
        }

        if !file_type.is_file() {
            continue;
        }

        let path: PathBuf = entry.path();
        let relative_path: &Path = path.strip_prefix(root).map_err(|error| {
            AnalyzeError::Io(format!(
                "failed to compute relative path for {}: {error}",
                path.display()
            ))
        })?;
        let normalized_path: String =
            normalize_path(relative_path).map_err(|_| AnalyzeError::UnsafePath {
                path: relative_path.display().to_string(),
            })?;
        let contents: Vec<u8> =
            fs::read(&path).map_err(|error| AnalyzeError::Io(error.to_string()))?;

        files.push(IndexedFile {
            normalized_path,
            contents,
        });
    }

    Ok(())
}

fn should_skip_directory(name: &str) -> bool {
    matches!(name, ".git" | "target" | "node_modules" | "__MACOSX")
}

fn select_entry(entry_candidates: &[EntryCandidate]) -> (Option<String>, EntrySelectionReason) {
    if entry_candidates.is_empty() {
        return (None, EntrySelectionReason::NoMarkdownFiles);
    }

    if let Some(root_readme) = entry_candidates
        .iter()
        .find(|candidate| candidate.path.eq_ignore_ascii_case("README.md"))
    {
        return (Some(root_readme.path.clone()), EntrySelectionReason::Readme);
    }

    let readme_candidates: Vec<&EntryCandidate> = entry_candidates
        .iter()
        .filter(|candidate| file_name(&candidate.path).eq_ignore_ascii_case("README.md"))
        .collect();
    if readme_candidates.len() == 1 {
        return (
            Some(readme_candidates[0].path.clone()),
            EntrySelectionReason::Readme,
        );
    }
    if readme_candidates.len() > 1 {
        return (None, EntrySelectionReason::MultipleCandidates);
    }

    if let Some(root_index) = entry_candidates
        .iter()
        .find(|candidate| candidate.path.eq_ignore_ascii_case("index.md"))
    {
        return (Some(root_index.path.clone()), EntrySelectionReason::Index);
    }

    let index_candidates: Vec<&EntryCandidate> = entry_candidates
        .iter()
        .filter(|candidate| file_name(&candidate.path).eq_ignore_ascii_case("index.md"))
        .collect();
    if index_candidates.len() == 1 {
        return (
            Some(index_candidates[0].path.clone()),
            EntrySelectionReason::Index,
        );
    }
    if index_candidates.len() > 1 {
        return (None, EntrySelectionReason::MultipleCandidates);
    }

    if entry_candidates.len() == 1 {
        return (
            Some(entry_candidates[0].path.clone()),
            EntrySelectionReason::SingleMarkdownFile,
        );
    }

    (None, EntrySelectionReason::MultipleCandidates)
}

fn collect_assets(
    markdown_files: &[&IndexedFile],
    known_paths: &[&str],
    diagnostic: &mut Diagnostic,
) -> Vec<AssetRef> {
    let mut assets: Vec<AssetRef> = Vec::new();

    for markdown_file in markdown_files {
        let contents: String = String::from_utf8_lossy(&markdown_file.contents).into_owned();

        for reference in extract_markdown_image_destinations(&contents) {
            assets.push(resolve_asset_reference(
                &markdown_file.normalized_path,
                &reference,
                AssetReferenceKind::MarkdownImage,
                known_paths,
                diagnostic,
            ));
        }

        for reference in extract_raw_html_img_sources(&contents) {
            assets.push(resolve_asset_reference(
                &markdown_file.normalized_path,
                &reference,
                AssetReferenceKind::RawHtmlImage,
                known_paths,
                diagnostic,
            ));
        }
    }

    assets
}

fn resolve_asset_reference(
    entry_path: &str,
    original_reference: &str,
    kind: AssetReferenceKind,
    known_paths: &[&str],
    diagnostic: &mut Diagnostic,
) -> AssetRef {
    let trimmed_reference: &str = original_reference.trim();

    if has_windows_drive_prefix(trimmed_reference) {
        diagnostic
            .path_errors
            .push(format!("{entry_path} -> {trimmed_reference}"));
        return AssetRef {
            entry_path: entry_path.to_string(),
            original_reference: trimmed_reference.to_string(),
            resolved_path: None,
            fetch_url: None,
            kind,
            status: AssetStatus::UnsupportedScheme,
        };
    }

    if is_external_reference(trimmed_reference) {
        return AssetRef {
            entry_path: entry_path.to_string(),
            original_reference: trimmed_reference.to_string(),
            resolved_path: None,
            fetch_url: remote_fetch_url(trimmed_reference),
            kind,
            status: AssetStatus::External,
        };
    }

    if has_uri_scheme(trimmed_reference) {
        diagnostic.warnings.push(format!(
            "unsupported asset scheme: {entry_path} -> {trimmed_reference}"
        ));
        return AssetRef {
            entry_path: entry_path.to_string(),
            original_reference: trimmed_reference.to_string(),
            resolved_path: None,
            fetch_url: None,
            kind,
            status: AssetStatus::UnsupportedScheme,
        };
    }

    let local_reference: &str = strip_reference_query_and_fragment(trimmed_reference);
    let normalized_path: String = match resolve_local_asset_path(entry_path, local_reference) {
        Ok(path) => path,
        Err(()) => {
            diagnostic
                .path_errors
                .push(format!("{entry_path} -> {trimmed_reference}"));
            return AssetRef {
                entry_path: entry_path.to_string(),
                original_reference: trimmed_reference.to_string(),
                resolved_path: None,
                fetch_url: None,
                kind,
                status: AssetStatus::UnsupportedScheme,
            };
        }
    };

    if known_paths.binary_search(&normalized_path.as_str()).is_ok() {
        return AssetRef {
            entry_path: entry_path.to_string(),
            original_reference: trimmed_reference.to_string(),
            resolved_path: Some(normalized_path),
            fetch_url: None,
            kind,
            status: AssetStatus::Resolved,
        };
    }

    diagnostic
        .missing_assets
        .push(format!("{entry_path} -> {trimmed_reference}"));

    AssetRef {
        entry_path: entry_path.to_string(),
        original_reference: trimmed_reference.to_string(),
        resolved_path: Some(normalized_path),
        fetch_url: None,
        kind,
        status: AssetStatus::Missing,
    }
}

fn resolve_local_asset_path(entry_path: &str, reference: &str) -> Result<String, ()> {
    if reference.starts_with('/') || reference.starts_with('\\') {
        let root_relative_reference: &str = reference.trim_start_matches(['/', '\\']);
        return normalize_relative_string(root_relative_reference);
    }

    let combined_reference: String = join_with_entry_directory(entry_path, reference);
    normalize_relative_string(&combined_reference)
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
    let trimmed_path: &str = path.trim();
    if trimmed_path.is_empty() {
        return Err(());
    }

    if trimmed_path.starts_with('/') || trimmed_path.starts_with('\\') {
        return Err(());
    }

    if has_windows_drive_prefix(trimmed_path) {
        return Err(());
    }

    let normalized_input: String = trimmed_path.replace('\\', "/");
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

fn strip_reference_query_and_fragment(reference: &str) -> &str {
    let query_start: usize = reference.find('?').unwrap_or(reference.len());
    let fragment_start: usize = reference.find('#').unwrap_or(reference.len());
    let suffix_start: usize = query_start.min(fragment_start);
    reference[..suffix_start].trim()
}

fn extract_markdown_image_destinations(markdown: &str) -> Vec<String> {
    let mut destinations: Vec<String> = Vec::new();
    let mut offset: usize = 0;

    while let Some(marker_index) = markdown[offset..].find("![") {
        let alt_start: usize = offset + marker_index + 2;

        // Find the closing `]` of the alt text, tracking bracket depth so
        // nested brackets (e.g. `[![inner]][ref]`) are handled correctly.
        let Some(alt_close) = find_closing_bracket(&markdown[alt_start..]) else {
            offset = alt_start;
            continue;
        };
        let after_alt: usize = alt_start + alt_close + 1;

        // Only inline-style images `![alt](url)` have `(` right after the `]`.
        // Reference-style images like `![alt][ref]` or `![alt]` must be skipped.
        if after_alt >= markdown.len() || markdown.as_bytes()[after_alt] != b'(' {
            offset = after_alt;
            continue;
        }

        let destination_start: usize = after_alt + 1;
        let Some(destination_end) = find_closing_parenthesis(&markdown[destination_start..]) else {
            break;
        };

        let raw_destination: &str =
            &markdown[destination_start..destination_start + destination_end];
        let cleaned_destination: String = clean_markdown_destination(raw_destination);
        if !cleaned_destination.is_empty() {
            destinations.push(cleaned_destination);
        }

        offset = destination_start + destination_end + 1;
    }

    destinations
}

/// Finds the position of the closing `]` that matches the bracket depth,
/// accounting for nested brackets and backslash escapes.
fn find_closing_bracket(input: &str) -> Option<usize> {
    let mut depth: usize = 0;
    let mut previous_was_escape: bool = false;

    for (index, character) in input.char_indices() {
        if previous_was_escape {
            previous_was_escape = false;
            continue;
        }

        if character == '\\' {
            previous_was_escape = true;
            continue;
        }

        match character {
            '[' => depth += 1,
            ']' => {
                if depth == 0 {
                    return Some(index);
                }
                depth -= 1;
            }
            _ => {}
        }
    }

    None
}

fn find_closing_parenthesis(input: &str) -> Option<usize> {
    let mut depth: usize = 0;
    let mut previous_was_escape: bool = false;

    for (index, character) in input.char_indices() {
        if previous_was_escape {
            previous_was_escape = false;
            continue;
        }

        if character == '\\' {
            previous_was_escape = true;
            continue;
        }

        match character {
            '(' => depth += 1,
            ')' => {
                if depth == 0 {
                    return Some(index);
                }
                depth -= 1;
            }
            _ => {}
        }
    }

    None
}

fn clean_markdown_destination(raw_destination: &str) -> String {
    let trimmed: &str = raw_destination.trim();
    if trimmed.starts_with('<') && trimmed.ends_with('>') && trimmed.len() >= 2 {
        return trimmed[1..trimmed.len() - 1].trim().to_string();
    }

    trimmed
        .split_ascii_whitespace()
        .next()
        .unwrap_or_default()
        .to_string()
}

fn extract_raw_html_img_sources(markdown: &str) -> Vec<String> {
    let lower_markdown: String = markdown.to_ascii_lowercase();
    let mut sources: Vec<String> = Vec::new();
    let mut offset: usize = 0;

    while let Some(tag_index) = lower_markdown[offset..].find("<img") {
        let tag_start: usize = offset + tag_index;
        let Some(tag_end_offset) = lower_markdown[tag_start..].find('>') else {
            break;
        };
        let tag_end: usize = tag_start + tag_end_offset + 1;
        let tag_text: &str = &markdown[tag_start..tag_end];

        if let Some(source) = extract_src_attribute(tag_text) {
            sources.push(source);
        }

        offset = tag_end;
    }

    sources
}

fn extract_src_attribute(tag: &str) -> Option<String> {
    let lower_tag: String = tag.to_ascii_lowercase();
    let mut offset: usize = 0;

    while let Some(relative_index) = lower_tag[offset..].find("src") {
        let name_start: usize = offset + relative_index;
        let Some(previous) = tag[..name_start].chars().last() else {
            offset = name_start + 3;
            continue;
        };

        if !previous.is_ascii_whitespace() && previous != '<' {
            offset = name_start + 3;
            continue;
        }

        let mut cursor: usize = name_start + 3;
        cursor = skip_ascii_whitespace(tag, cursor);

        if !tag[cursor..].starts_with('=') {
            offset = name_start + 3;
            continue;
        }
        cursor += 1;

        cursor = skip_ascii_whitespace(tag, cursor);

        let first_char: char = tag[cursor..].chars().next()?;
        if matches!(first_char, '"' | '\'') {
            let quote: char = first_char;
            cursor += quote.len_utf8();
            let end_offset: usize = tag[cursor..].find(quote)?;
            return Some(tag[cursor..cursor + end_offset].to_string());
        }

        let end_offset: usize = tag[cursor..]
            .find(|character: char| character.is_ascii_whitespace() || character == '>')
            .unwrap_or(tag[cursor..].len());
        return Some(tag[cursor..cursor + end_offset].to_string());
    }

    None
}

fn skip_ascii_whitespace(input: &str, mut cursor: usize) -> usize {
    while let Some(character) = input[cursor..].chars().next() {
        if !character.is_ascii_whitespace() {
            break;
        }
        cursor += character.len_utf8();
    }

    cursor
}

fn join_with_entry_directory(entry_path: &str, reference: &str) -> String {
    let Some((directory, _)) = entry_path.rsplit_once('/') else {
        return reference.to_string();
    };

    format!("{directory}/{reference}")
}

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn is_markdown_path(path: &str) -> bool {
    let lowercase_path: String = path.to_ascii_lowercase();
    lowercase_path.ends_with(".md") || lowercase_path.ends_with(".markdown")
}

fn is_supported_image_path(path: &str) -> bool {
    let lowercase_path: String = path.to_ascii_lowercase();
    [
        ".png", ".jpg", ".jpeg", ".gif", ".svg", ".webp", ".bmp", ".avif",
    ]
    .iter()
    .any(|extension| lowercase_path.ends_with(extension))
}

fn is_http_reference(reference: &str) -> bool {
    let lowercase_reference: String = reference.to_ascii_lowercase();
    lowercase_reference.starts_with("http://") || lowercase_reference.starts_with("https://")
}

fn is_external_reference(reference: &str) -> bool {
    let lowercase_reference: String = reference.to_ascii_lowercase();
    is_http_reference(reference) || lowercase_reference.starts_with("data:")
}

fn has_uri_scheme(reference: &str) -> bool {
    let Some(colon_index) = reference.find(':') else {
        return false;
    };

    if reference[..colon_index]
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '+' | '-' | '.'))
    {
        return true;
    }

    false
}

fn has_windows_drive_prefix(path: &str) -> bool {
    let bytes: &[u8] = path.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

fn render_markdown_to_html(markdown: &str, render_options: &RenderOptions) -> RenderedMarkdownBody {
    let mut markdown_options: MarkdownOptions = MarkdownOptions::empty();
    markdown_options.insert(MarkdownOptions::ENABLE_STRIKETHROUGH);
    markdown_options.insert(MarkdownOptions::ENABLE_TABLES);
    markdown_options.insert(MarkdownOptions::ENABLE_TASKLISTS);
    markdown_options.insert(MarkdownOptions::ENABLE_FOOTNOTES);
    markdown_options.insert(MarkdownOptions::ENABLE_HEADING_ATTRIBUTES);
    if !matches!(render_options.math_mode, MathMode::Off) {
        markdown_options.insert(MarkdownOptions::ENABLE_MATH);
    }

    let parser: Parser<'_> = Parser::new_ext(markdown, markdown_options);
    let mut events: Vec<Event<'_>> = Vec::new();
    let mut headings: Vec<RenderedHeading> = Vec::new();
    let mut pending_heading: Option<PendingHeading<'_>> = None;
    let mut used_heading_ids: HashSet<String> = HashSet::new();
    let mut in_code_block: bool = false;

    for event in parser {
        match event {
            Event::Start(Tag::Heading {
                level,
                id,
                classes,
                attrs,
            }) => {
                pending_heading = Some(PendingHeading {
                    level,
                    id,
                    classes,
                    attrs,
                    events: Vec::new(),
                    title: String::new(),
                });
            }
            Event::End(TagEnd::Heading(_)) => {
                let Some(mut heading) = pending_heading.take() else {
                    continue;
                };
                let heading_title: String = normalize_heading_title(&heading.title, headings.len());
                let heading_id: String = unique_heading_id(
                    heading
                        .id
                        .as_deref()
                        .map(normalize_heading_id)
                        .unwrap_or_else(|| slugify_heading_text(&heading_title)),
                    &mut used_heading_ids,
                );
                headings.push(RenderedHeading {
                    level: heading.level,
                    id: heading_id.clone(),
                    title: heading_title,
                });
                events.push(Event::Start(Tag::Heading {
                    level: heading.level,
                    id: Some(heading_id.into()),
                    classes: std::mem::take(&mut heading.classes),
                    attrs: std::mem::take(&mut heading.attrs),
                }));
                events.extend(std::mem::take(&mut heading.events));
                events.push(Event::End(TagEnd::Heading(heading.level)));
            }
            Event::Start(Tag::CodeBlock(kind)) => {
                in_code_block = true;
                push_heading_or_document_event(
                    &mut pending_heading,
                    &mut events,
                    Event::Start(Tag::CodeBlock(kind)),
                );
            }
            Event::End(TagEnd::CodeBlock) => {
                in_code_block = false;
                push_heading_or_document_event(
                    &mut pending_heading,
                    &mut events,
                    Event::End(TagEnd::CodeBlock),
                );
            }
            Event::Text(text) if !in_code_block => {
                let replaced_text: String = replace_github_emoji_shortcodes(text.as_ref());
                if let Some(heading) = pending_heading.as_mut() {
                    heading.title.push_str(&replaced_text);
                    heading.events.push(Event::Text(replaced_text.into()));
                } else {
                    events.push(Event::Text(replaced_text.into()));
                }
            }
            Event::Text(text) => {
                if let Some(heading) = pending_heading.as_mut() {
                    heading.title.push_str(text.as_ref());
                    heading.events.push(Event::Text(text));
                } else {
                    events.push(Event::Text(text));
                }
            }
            Event::Code(text) => {
                if let Some(heading) = pending_heading.as_mut() {
                    heading.title.push_str(text.as_ref());
                    heading.events.push(Event::Code(text));
                } else {
                    events.push(Event::Code(text));
                }
            }
            Event::SoftBreak => {
                if let Some(heading) = pending_heading.as_mut() {
                    heading.title.push(' ');
                    heading.events.push(Event::SoftBreak);
                } else {
                    events.push(Event::SoftBreak);
                }
            }
            Event::HardBreak => {
                if let Some(heading) = pending_heading.as_mut() {
                    heading.title.push(' ');
                    heading.events.push(Event::HardBreak);
                } else {
                    events.push(Event::HardBreak);
                }
            }
            other_event => {
                push_heading_or_document_event(&mut pending_heading, &mut events, other_event);
            }
        }
    }

    let mut html_output: String = String::new();
    html::push_html(&mut html_output, events.into_iter());
    if render_options.enable_toc && headings.len() > 1 {
        html_output = format!("{}{}", build_toc_html(&headings), html_output);
    }

    RenderedMarkdownBody {
        html: html_output,
        headings,
    }
}

fn replace_github_emoji_shortcodes(mut text: &str) -> String {
    let mut replaced: String = String::with_capacity(text.len());

    while let Some((token_start, shortcode_start, shortcode_end, token_end)) = text
        .find(':')
        .map(|index| (index, index + 1))
        .and_then(|(token_start, shortcode_start)| {
            text[shortcode_start..].find(':').map(|closing_offset| {
                (
                    token_start,
                    shortcode_start,
                    shortcode_start + closing_offset,
                    shortcode_start + closing_offset + 1,
                )
            })
        })
    {
        if let Some(emoji) = emojis::get_by_shortcode(&text[shortcode_start..shortcode_end]) {
            replaced.push_str(&text[..token_start]);
            replaced.push_str(emoji.as_str());
            text = &text[token_end..];
            continue;
        }

        replaced.push_str(&text[..shortcode_end]);
        text = &text[shortcode_end..];
    }

    replaced.push_str(text);
    replaced
}

fn push_heading_or_document_event<'a>(
    pending_heading: &mut Option<PendingHeading<'a>>,
    events: &mut Vec<Event<'a>>,
    event: Event<'a>,
) {
    if let Some(heading) = pending_heading.as_mut() {
        heading.events.push(event);
    } else {
        events.push(event);
    }
}

fn normalize_heading_title(title: &str, heading_index: usize) -> String {
    let trimmed = title.trim();
    if trimmed.is_empty() {
        format!("Section {}", heading_index + 1)
    } else {
        trimmed.to_string()
    }
}

fn normalize_heading_id(id: &str) -> String {
    let trimmed = id.trim().trim_start_matches('#');
    if trimmed.is_empty() {
        "section".to_string()
    } else {
        trimmed.to_string()
    }
}

fn slugify_heading_text(text: &str) -> String {
    let mut slug = String::new();
    let mut previous_was_separator = false;

    for character in text.chars().flat_map(char::to_lowercase) {
        if character.is_alphanumeric() {
            slug.push(character);
            previous_was_separator = false;
        } else if (character.is_whitespace() || matches!(character, '-' | '_'))
            && !previous_was_separator
        {
            slug.push('-');
            previous_was_separator = true;
        }
    }

    let trimmed = slug.trim_matches('-');
    if trimmed.is_empty() {
        "section".to_string()
    } else {
        trimmed.to_string()
    }
}

fn unique_heading_id(base_id: String, used_heading_ids: &mut HashSet<String>) -> String {
    if used_heading_ids.insert(base_id.clone()) {
        return base_id;
    }

    let mut index: usize = 2;
    loop {
        let candidate = format!("{base_id}-{index}");
        if used_heading_ids.insert(candidate.clone()) {
            return candidate;
        }
        index += 1;
    }
}

fn build_toc_html(headings: &[RenderedHeading]) -> String {
    let mut toc_html = String::from(
        "<nav class=\"marknest-toc\" aria-label=\"Table of contents\"><p class=\"marknest-toc-title\">Contents</p><ol class=\"marknest-toc-list\">",
    );

    for heading in headings {
        toc_html.push_str("<li class=\"marknest-toc-level-");
        toc_html.push_str(&heading_level_number(heading.level).to_string());
        toc_html.push_str("\"><a href=\"#");
        toc_html.push_str(&escape_html_attribute(&heading.id));
        toc_html.push_str("\">");
        toc_html.push_str(&escape_html_text(&heading.title));
        toc_html.push_str("</a></li>");
    }

    toc_html.push_str("</ol></nav>");
    toc_html
}

fn heading_level_number(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

fn render_entry_with_options(
    file_system: &dyn IndexedFileSystem,
    entry_path: &str,
    options: &RenderOptions,
) -> Result<RenderedHtmlDocument, RenderHtmlError> {
    let normalized_entry_path: String =
        normalize_relative_string(entry_path).map_err(|_| RenderHtmlError::InvalidEntryPath {
            entry_path: entry_path.to_string(),
        })?;
    let project_index = analyze_project(file_system).map_err(RenderHtmlError::Analyze)?;

    if !project_index
        .entry_candidates
        .iter()
        .any(|candidate| candidate.path == normalized_entry_path)
    {
        return Err(RenderHtmlError::EntryNotFound {
            entry_path: normalized_entry_path,
        });
    }

    let entry_bytes: Vec<u8> = file_system
        .file_contents(&normalized_entry_path)
        .ok_or_else(|| RenderHtmlError::EntryNotFound {
            entry_path: normalized_entry_path.clone(),
        })?
        .to_vec();
    let markdown: String =
        String::from_utf8(entry_bytes).map_err(|_| RenderHtmlError::InvalidUtf8 {
            entry_path: normalized_entry_path.clone(),
        })?;

    let rendered_markdown: RenderedMarkdownBody = render_markdown_to_html(&markdown, options);
    let body_html: String = if options.sanitize_html {
        sanitize_html_fragment(&rendered_markdown.html)
    } else {
        rendered_markdown.html
    };
    let body_html: String = expand_collapsed_details(&body_html);
    let body_html: String = inline_entry_assets(
        file_system,
        &body_html,
        &project_index.assets,
        &normalized_entry_path,
    )
    .map_err(RenderHtmlError::Io)?;
    let title: String = options
        .metadata
        .title
        .clone()
        .unwrap_or_else(|| title_from_entry_path(&normalized_entry_path));

    Ok(RenderedHtmlDocument {
        title: title.clone(),
        html: build_html_document(&title, &body_html, options),
    })
}

fn inline_entry_assets(
    file_system: &dyn IndexedFileSystem,
    html_document: &str,
    assets: &[AssetRef],
    entry_path: &str,
) -> Result<String, String> {
    let replacements: Vec<(String, String)> = assets
        .iter()
        .filter(|asset| asset.entry_path == entry_path && asset.status == AssetStatus::Resolved)
        .filter_map(|asset| {
            asset.resolved_path.as_ref().map(|resolved_path| {
                let bytes: Vec<u8> = file_system
                    .file_contents(resolved_path)
                    .ok_or_else(|| format!("Failed to read asset {resolved_path}: not found"))?
                    .to_vec();
                let mime_type: &'static str = infer_mime_type(resolved_path);
                Ok::<(String, String), String>((
                    asset.original_reference.clone(),
                    format!("data:{mime_type};base64,{}", encode_base64(&bytes)),
                ))
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(rewrite_html_img_sources(html_document, &replacements))
}

pub fn rewrite_html_img_sources(html_document: &str, replacements: &[(String, String)]) -> String {
    let lower_html_document: String = html_document.to_ascii_lowercase();
    let mut rewritten_html: String = String::new();
    let mut cursor: usize = 0;

    while let Some(relative_tag_index) = lower_html_document[cursor..].find("<img") {
        let tag_start: usize = cursor + relative_tag_index;
        let Some(relative_tag_end) = lower_html_document[tag_start..].find('>') else {
            break;
        };
        let tag_end: usize = tag_start + relative_tag_end + 1;

        rewritten_html.push_str(&html_document[cursor..tag_start]);
        rewritten_html.push_str(&rewrite_img_tag(
            &html_document[tag_start..tag_end],
            replacements,
        ));
        cursor = tag_end;
    }

    rewritten_html.push_str(&html_document[cursor..]);
    rewritten_html
}

fn expand_collapsed_details(html_document: &str) -> String {
    let lower_html_document: String = html_document.to_ascii_lowercase();
    let mut rewritten_html: String = String::new();
    let mut cursor: usize = 0;

    while let Some(relative_tag_index) = lower_html_document[cursor..].find("<details") {
        let tag_start: usize = cursor + relative_tag_index;
        let Some(relative_tag_end) = lower_html_document[tag_start..].find('>') else {
            break;
        };
        let tag_end: usize = tag_start + relative_tag_end + 1;

        rewritten_html.push_str(&html_document[cursor..tag_start]);
        rewritten_html.push_str(&expand_details_tag(&html_document[tag_start..tag_end]));
        cursor = tag_end;
    }

    rewritten_html.push_str(&html_document[cursor..]);
    rewritten_html
}

fn expand_details_tag(tag: &str) -> String {
    if tag_has_boolean_attribute(tag, "details", "open") {
        return tag.to_string();
    }

    let Some(tag_end) = tag.rfind('>') else {
        return tag.to_string();
    };
    let insertion_prefix: &str = if tag[..tag_end]
        .chars()
        .last()
        .is_some_and(|character| character.is_ascii_whitespace())
    {
        ""
    } else {
        " "
    };

    let mut expanded_tag: String = String::new();
    expanded_tag.push_str(&tag[..tag_end]);
    expanded_tag.push_str(insertion_prefix);
    expanded_tag.push_str("open");
    expanded_tag.push_str(&tag[tag_end..]);
    expanded_tag
}

fn rewrite_img_tag(tag: &str, replacements: &[(String, String)]) -> String {
    let Some(src_span) = find_src_attribute_span(tag) else {
        return tag.to_string();
    };

    let original_reference: &str = &tag[src_span.value_start..src_span.value_end];
    let Some((_, replacement)) = replacements
        .iter()
        .find(|(candidate, _)| candidate == original_reference)
    else {
        return tag.to_string();
    };

    if src_span.is_quoted {
        let mut rewritten_tag: String = String::new();
        rewritten_tag.push_str(&tag[..src_span.value_start]);
        rewritten_tag.push_str(&escape_html_attribute(replacement));
        rewritten_tag.push_str(&tag[src_span.value_end..]);
        return rewritten_tag;
    }

    let mut rewritten_tag: String = String::new();
    rewritten_tag.push_str(&tag[..src_span.value_start]);
    rewritten_tag.push('"');
    rewritten_tag.push_str(&escape_html_attribute(replacement));
    rewritten_tag.push('"');
    rewritten_tag.push_str(&tag[src_span.value_end..]);
    rewritten_tag
}

fn find_src_attribute_span(tag: &str) -> Option<SrcAttributeSpan> {
    let lower_tag: String = tag.to_ascii_lowercase();
    let mut offset: usize = 0;

    while let Some(relative_index) = lower_tag[offset..].find("src") {
        let name_start: usize = offset + relative_index;
        let previous_is_boundary: bool = match tag[..name_start].chars().last() {
            Some(character) => character.is_ascii_whitespace() || character == '<',
            None => false,
        };

        if !previous_is_boundary {
            offset = name_start + 3;
            continue;
        }

        let mut cursor: usize = name_start + 3;
        cursor = skip_ascii_whitespace(tag, cursor);

        if !tag[cursor..].starts_with('=') {
            offset = name_start + 3;
            continue;
        }
        cursor += 1;
        cursor = skip_ascii_whitespace(tag, cursor);

        let first_character: char = tag[cursor..].chars().next()?;
        if matches!(first_character, '"' | '\'') {
            let quote_character: char = first_character;
            let value_start: usize = cursor + quote_character.len_utf8();
            let value_end_offset: usize = tag[value_start..].find(quote_character)?;
            return Some(SrcAttributeSpan {
                value_start,
                value_end: value_start + value_end_offset,
                is_quoted: true,
            });
        }

        let value_end_offset: usize = tag[cursor..]
            .find(|character: char| character.is_ascii_whitespace() || character == '>')
            .unwrap_or(tag[cursor..].len());
        return Some(SrcAttributeSpan {
            value_start: cursor,
            value_end: cursor + value_end_offset,
            is_quoted: false,
        });
    }

    None
}

fn tag_has_boolean_attribute(tag: &str, expected_tag_name: &str, attribute_name: &str) -> bool {
    let lower_tag: String = tag.to_ascii_lowercase();
    if !lower_tag.starts_with(&format!("<{expected_tag_name}")) {
        return false;
    }

    let mut cursor: usize = expected_tag_name.len() + 1;
    while cursor < tag.len() {
        cursor = skip_ascii_whitespace(tag, cursor);
        if cursor >= tag.len() {
            break;
        }

        let Some(character) = tag[cursor..].chars().next() else {
            break;
        };
        if matches!(character, '>' | '/') {
            break;
        }

        let attribute_start: usize = cursor;
        while let Some(attribute_character) = tag[cursor..].chars().next() {
            if attribute_character.is_ascii_whitespace()
                || matches!(attribute_character, '=' | '>' | '/')
            {
                break;
            }
            cursor += attribute_character.len_utf8();
        }

        if tag[attribute_start..cursor].eq_ignore_ascii_case(attribute_name) {
            return true;
        }

        cursor = skip_ascii_whitespace(tag, cursor);
        if !tag[cursor..].starts_with('=') {
            continue;
        }
        cursor += 1;
        cursor = skip_ascii_whitespace(tag, cursor);

        let Some(value_start_character) = tag[cursor..].chars().next() else {
            break;
        };
        if matches!(value_start_character, '"' | '\'') {
            let quote_character: char = value_start_character;
            cursor += quote_character.len_utf8();
            let Some(value_end_offset) = tag[cursor..].find(quote_character) else {
                break;
            };
            cursor += value_end_offset + quote_character.len_utf8();
            continue;
        }

        while let Some(value_character) = tag[cursor..].chars().next() {
            if value_character.is_ascii_whitespace() || matches!(value_character, '>' | '/') {
                break;
            }
            cursor += value_character.len_utf8();
        }
    }

    false
}

fn build_html_document(title: &str, body_html: &str, options: &RenderOptions) -> String {
    let metadata_tags: String = build_metadata_tags(&options.metadata);
    let runtime_script: String = build_runtime_script(options);
    let custom_css: &str = options.custom_css.as_deref().unwrap_or("");
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>{}</title>{}<style>{}{}{}{}{}</style></head><body class=\"{}\">{}{}</body></html>",
        escape_html_text(title),
        metadata_tags,
        base_stylesheet(),
        theme_stylesheet(options.theme),
        custom_css,
        runtime_stylesheet(),
        if runtime_script.is_empty() {
            ""
        } else {
            ".marknest-mermaid svg, .math-rendered svg { max-width: 100%; height: auto; }"
        },
        theme_class_name(options.theme),
        body_html,
        runtime_script
    )
}

fn sanitize_html_fragment(html_fragment: &str) -> String {
    let mut sanitizer = HtmlSanitizerBuilder::default();
    sanitizer.url_relative(UrlRelative::PassThrough);
    sanitizer.add_generic_attributes(["aria-label", "class", "id", "title"]);
    sanitizer.add_tags(["details", "figure", "figcaption", "input", "nav", "summary"]);
    sanitizer.add_tag_attributes("img", ["width", "height", "align", "loading"]);
    sanitizer.add_tag_attributes("input", ["type", "checked", "disabled"]);
    sanitizer.add_tag_attributes("details", ["open"]);
    sanitizer.clean(html_fragment).to_string()
}

fn base_stylesheet() -> &'static str {
    "body { background: #ffffff; color: #111827; font-family: \"Segoe UI\", Arial, sans-serif; font-size: 12pt; line-height: 1.6; margin: 0; } h1, h2, h3, h4, h5, h6 { line-height: 1.25; margin: 1.2em 0 0.5em; } p, ul, ol, pre, table, blockquote, figure, details { margin: 0 0 1em; } details > summary { cursor: default; } details:not([open]) > :not(summary) { display: block; } img { max-width: 100%; vertical-align: middle; } p > img:only-child, p > a:only-child > img, body > img, body > a > img { display: block; margin: 0 0 1em; } pre { background: #f3f4f6; border-radius: 8px; padding: 12px; white-space: pre-wrap; overflow-wrap: anywhere; word-break: break-word; } pre code { white-space: inherit; overflow-wrap: inherit; word-break: inherit; } code { font-family: Consolas, \"Cascadia Code\", monospace; } table { border-collapse: collapse; width: 100%; } th, td { border: 1px solid #d1d5db; padding: 6px 10px; text-align: left; } blockquote { border-left: 4px solid #d1d5db; color: #4b5563; padding-left: 12px; } .marknest-toc { border: 1px solid #d1d5db; border-radius: 10px; background: #f8fafc; padding: 16px 18px; margin: 0 0 1.5em; } .marknest-toc-title { font-weight: 700; letter-spacing: 0.01em; margin: 0 0 0.75em; } .marknest-toc-list { margin: 0; padding-left: 1.25em; } .marknest-toc-list li { margin: 0.2em 0; } .marknest-toc-level-2 { margin-left: 1rem; } .marknest-toc-level-3 { margin-left: 2rem; } .marknest-toc-level-4 { margin-left: 3rem; } .marknest-toc-level-5 { margin-left: 4rem; } .marknest-toc-level-6 { margin-left: 5rem; } @media print { h1 { break-before: page; page-break-before: always; } h1:first-of-type { break-before: auto; page-break-before: auto; } pre, table, blockquote, figure, img, tr, .marknest-toc { break-inside: avoid; page-break-inside: avoid; } thead { display: table-header-group; } }"
}

fn theme_stylesheet(theme: ThemePreset) -> &'static str {
    match theme {
        ThemePreset::Default => "",
        ThemePreset::Github => {
            ".theme-github { color: #1f2328; font-family: -apple-system, BlinkMacSystemFont, \"Segoe UI\", Helvetica, Arial, sans-serif; } .theme-github pre { background: #f6f8fa; border: 1px solid #d0d7de; } .theme-github blockquote { border-left-color: #d0d7de; color: #57606a; } .theme-github table th, .theme-github table td { border-color: #d0d7de; }"
        }
        ThemePreset::Docs => {
            ".theme-docs { color: #102a43; font-family: Georgia, \"Times New Roman\", serif; } .theme-docs h1, .theme-docs h2, .theme-docs h3 { color: #0b7285; } .theme-docs pre { background: #f8fafc; border: 1px solid #cbd5e1; } .theme-docs blockquote { border-left-color: #0b7285; color: #334e68; }"
        }
        ThemePreset::Plain => {
            ".theme-plain { color: #111827; font-family: \"Segoe UI\", Arial, sans-serif; } .theme-plain pre, .theme-plain blockquote { background: transparent; border-radius: 0; border-left-color: #9ca3af; } .theme-plain table th, .theme-plain table td { border-color: #9ca3af; }"
        }
    }
}

fn runtime_stylesheet() -> &'static str {
    ".math.math-inline { font-style: italic; white-space: nowrap; } .math.math-display { display: block; margin: 1.2em 0; text-align: center; } .marknest-mermaid { display: block; margin: 1.2em 0; }"
}

fn theme_class_name(theme: ThemePreset) -> &'static str {
    match theme {
        ThemePreset::Default => "theme-default",
        ThemePreset::Github => "theme-github",
        ThemePreset::Docs => "theme-docs",
        ThemePreset::Plain => "theme-plain",
    }
}

fn build_metadata_tags(metadata: &PdfMetadata) -> String {
    let mut tags: String = String::new();

    if let Some(author) = &metadata.author {
        tags.push_str("<meta name=\"author\" content=\"");
        tags.push_str(&escape_html_attribute(author));
        tags.push_str("\">");
    }

    if let Some(subject) = &metadata.subject {
        tags.push_str("<meta name=\"subject\" content=\"");
        tags.push_str(&escape_html_attribute(subject));
        tags.push_str("\">");
    }

    tags
}

fn build_runtime_script(options: &RenderOptions) -> String {
    if matches!(options.mermaid_mode, MermaidMode::Off)
        && matches!(options.math_mode, MathMode::Off)
    {
        return String::new();
    }

    format!(
        r#"<script>(function () {{
const config = {{"mermaidMode":"{}","mathMode":"{}","mermaidTheme":"{}","mermaidTimeoutMs":{},"mathTimeoutMs":{},"mermaidScript":"{}","mathScript":"{}"}};
const status = {{ready:false,warnings:[],errors:[]}};
window.__MARKNEST_RENDER_CONFIG__ = config;
window.__MARKNEST_RENDER_STATUS__ = status;
const addMessage = (kind, message) => {{
  if (kind === "error") {{
    status.errors.push(message);
  }} else {{
    status.warnings.push(message);
  }}
}};
const handleFailure = (mode, message) => addMessage(mode === "on" ? "error" : "warning", message);
const withTimeout = async (promiseFactory, timeoutMs, message) => {{
  const normalizedTimeoutMs = Math.max(1, Number(timeoutMs) || 0);
  let timerId = null;
  try {{
    return await Promise.race([
      promiseFactory(),
      new Promise((_, reject) => {{
        timerId = window.setTimeout(() => reject(new Error(message)), normalizedTimeoutMs);
      }}),
    ]);
  }} finally {{
    if (timerId !== null) {{
      window.clearTimeout(timerId);
    }}
  }}
}};
const loadScript = (src) => new Promise((resolve, reject) => {{
  const existing = document.querySelector(`script[data-marknest-src="${{src}}"]`);
  if (existing) {{
    if (existing.dataset.loaded === "true") {{
      resolve();
      return;
    }}
    existing.addEventListener("load", () => resolve(), {{ once: true }});
    existing.addEventListener("error", () => reject(new Error(`Failed to load ${{src}}.`)), {{ once: true }});
    return;
  }}
  const script = document.createElement("script");
  script.src = src;
  script.async = true;
  script.dataset.marknestSrc = src;
  script.addEventListener("load", () => {{
    script.dataset.loaded = "true";
    resolve();
  }}, {{ once: true }});
  script.addEventListener("error", () => reject(new Error(`Failed to load ${{src}}.`)), {{ once: true }});
  document.head.appendChild(script);
}});
const renderMermaid = async () => {{
  if (config.mermaidMode === "off") {{
    return;
  }}
  const blocks = Array.from(document.querySelectorAll("pre > code.language-mermaid"));
  if (blocks.length === 0) {{
    return;
  }}
  try {{
    await loadScript(config.mermaidScript);
    if (!window.mermaid) {{
      throw new Error("Mermaid did not initialize.");
    }}
    window.mermaid.initialize({{ startOnLoad: false, securityLevel: "strict", theme: config.mermaidTheme }});
    for (let index = 0; index < blocks.length; index += 1) {{
      const code = blocks[index];
      const source = code.textContent ? code.textContent.trim() : "";
      if (!source) {{
        handleFailure(config.mermaidMode, `Mermaid rendering failed: diagram ${{index + 1}} is empty.`);
        continue;
      }}
      try {{
        const rendered = await withTimeout(
          () => window.mermaid.render(`marknest-mermaid-${{index}}`, source),
          config.mermaidTimeoutMs,
          `Mermaid rendering timed out: diagram ${{index + 1}}.`,
        );
        const wrapper = document.createElement("figure");
        wrapper.className = "marknest-mermaid";
        wrapper.innerHTML = rendered.svg;
        const pre = code.parentElement;
        if (pre) {{
          pre.replaceWith(wrapper);
        }}
      }} catch (error) {{
        handleFailure(
          config.mermaidMode,
          error instanceof Error && error.message
            ? error.message
            : `Mermaid rendering failed: diagram ${{index + 1}}.`,
        );
      }}
    }}
  }} catch (error) {{
    handleFailure(config.mermaidMode, `Mermaid renderer could not be loaded: ${{error.message}}`);
  }}
}};
const renderMath = async () => {{
  if (config.mathMode === "off") {{
    return;
  }}
  const nodes = Array.from(document.querySelectorAll(".math-inline, .math-display"));
  if (nodes.length === 0) {{
    return;
  }}
  try {{
    window.MathJax = {{ startup: {{ typeset: false }}, svg: {{ fontCache: "none" }} }};
    await loadScript(config.mathScript);
    if (!window.MathJax || typeof window.MathJax.tex2svgPromise !== "function") {{
      throw new Error("MathJax did not initialize.");
    }}
    for (let index = 0; index < nodes.length; index += 1) {{
      const node = nodes[index];
      const tex = node.textContent ? node.textContent.trim() : "";
      if (!tex) {{
        continue;
      }}
      try {{
        const display = node.classList.contains("math-display");
        const rendered = await withTimeout(
          () => window.MathJax.tex2svgPromise(tex, {{ display }}),
          config.mathTimeoutMs,
          `Math rendering timed out: expression ${{index + 1}}.`,
        );
        node.replaceChildren(rendered);
        node.classList.add("math-rendered");
      }} catch (error) {{
        handleFailure(
          config.mathMode,
          error instanceof Error && error.message
            ? error.message
            : `Math rendering failed: expression ${{index + 1}}.`,
        );
      }}
    }}
  }} catch (error) {{
    handleFailure(config.mathMode, `Math renderer could not be loaded: ${{error.message}}`);
  }}
}};
const finalizeRendering = async () => {{
  try {{
    await renderMermaid();
    await renderMath();
  }} finally {{
    status.ready = true;
  }}
}};
if (document.readyState === "loading") {{
  document.addEventListener("DOMContentLoaded", () => {{
    void finalizeRendering();
  }}, {{ once: true }});
}} else {{
  void finalizeRendering();
}}
}})();</script>"#,
        mermaid_mode_name(options.mermaid_mode),
        math_mode_name(options.math_mode),
        mermaid_theme_name(options.theme),
        options.mermaid_timeout_ms,
        options.math_timeout_ms,
        MERMAID_SCRIPT_URL,
        MATHJAX_SCRIPT_URL
    )
}

fn mermaid_mode_name(mode: MermaidMode) -> &'static str {
    match mode {
        MermaidMode::Off => "off",
        MermaidMode::Auto => "auto",
        MermaidMode::On => "on",
    }
}

fn math_mode_name(mode: MathMode) -> &'static str {
    match mode {
        MathMode::Off => "off",
        MathMode::Auto => "auto",
        MathMode::On => "on",
    }
}

fn mermaid_theme_name(theme: ThemePreset) -> &'static str {
    match theme {
        ThemePreset::Plain => "neutral",
        ThemePreset::Default | ThemePreset::Github | ThemePreset::Docs => "default",
    }
}

fn title_from_entry_path(entry_path: &str) -> String {
    let file_name: &str = file_name(entry_path);
    Path::new(file_name)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("document")
        .to_string()
}

fn infer_mime_type(path: &str) -> &'static str {
    let lowercase_path: String = path.to_ascii_lowercase();
    if lowercase_path.ends_with(".png") {
        "image/png"
    } else if lowercase_path.ends_with(".jpg") || lowercase_path.ends_with(".jpeg") {
        "image/jpeg"
    } else if lowercase_path.ends_with(".gif") {
        "image/gif"
    } else if lowercase_path.ends_with(".svg") {
        "image/svg+xml"
    } else if lowercase_path.ends_with(".webp") {
        "image/webp"
    } else if lowercase_path.ends_with(".bmp") {
        "image/bmp"
    } else if lowercase_path.ends_with(".avif") {
        "image/avif"
    } else {
        "application/octet-stream"
    }
}

fn encode_base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut encoded: String = String::with_capacity(bytes.len().div_ceil(3) * 4);
    let mut index: usize = 0;

    while index + 3 <= bytes.len() {
        let chunk: &[u8] = &bytes[index..index + 3];
        encoded.push(TABLE[(chunk[0] >> 2) as usize] as char);
        encoded.push(TABLE[((chunk[0] & 0b0000_0011) << 4 | (chunk[1] >> 4)) as usize] as char);
        encoded.push(TABLE[((chunk[1] & 0b0000_1111) << 2 | (chunk[2] >> 6)) as usize] as char);
        encoded.push(TABLE[(chunk[2] & 0b0011_1111) as usize] as char);
        index += 3;
    }

    match bytes.len() - index {
        1 => {
            let byte: u8 = bytes[index];
            encoded.push(TABLE[(byte >> 2) as usize] as char);
            encoded.push(TABLE[((byte & 0b0000_0011) << 4) as usize] as char);
            encoded.push('=');
            encoded.push('=');
        }
        2 => {
            let first: u8 = bytes[index];
            let second: u8 = bytes[index + 1];
            encoded.push(TABLE[(first >> 2) as usize] as char);
            encoded.push(TABLE[((first & 0b0000_0011) << 4 | (second >> 4)) as usize] as char);
            encoded.push(TABLE[((second & 0b0000_1111) << 2) as usize] as char);
            encoded.push('=');
        }
        _ => {}
    }

    encoded
}

fn escape_html_text(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn escape_html_attribute(value: &str) -> String {
    escape_html_text(value).replace('"', "&quot;")
}

#[derive(Debug, Clone, Copy)]
struct SrcAttributeSpan {
    value_start: usize,
    value_end: usize,
    is_quoted: bool,
}
