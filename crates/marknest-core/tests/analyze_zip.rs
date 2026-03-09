use std::io::{Cursor, Write};

use marknest_core::{
    AnalyzeError, EntrySelectionReason, ProjectSourceKind, analyze_zip, analyze_zip_strip_prefix,
};
use zip::write::SimpleFileOptions;

fn build_zip(entries: &[(&str, &str)]) -> Vec<u8> {
    let mut cursor = Cursor::new(Vec::new());
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
fn analyzes_a_safe_zip_archive() {
    let bytes = build_zip(&[
        ("README.md", "# Zip\n\n![Asset](./images/diagram.svg)\n"),
        (
            "images/diagram.svg",
            "<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>",
        ),
        ("ignored.txt", "ignore"),
    ]);

    let index = analyze_zip(&bytes).expect("safe zip should analyze");

    assert_eq!(index.source_kind, ProjectSourceKind::Zip);
    assert_eq!(index.selected_entry.as_deref(), Some("README.md"));
    assert_eq!(index.entry_selection_reason, EntrySelectionReason::Readme);
    assert_eq!(
        index.diagnostic.ignored_files,
        vec!["ignored.txt".to_string()]
    );
    assert_eq!(index.diagnostic.missing_assets, Vec::<String>::new());
}

#[test]
fn rejects_zip_slip_entries() {
    let bytes = build_zip(&[("../escape.md", "# nope")]);

    let error = analyze_zip(&bytes).expect_err("unsafe zip path should be rejected");
    assert_eq!(
        error,
        AnalyzeError::UnsafePath {
            path: "../escape.md".to_string(),
        }
    );
}

#[test]
fn rejects_windows_drive_paths_inside_zip() {
    let bytes = build_zip(&[(r"C:\docs\README.md", "# nope")]);

    let error = analyze_zip(&bytes).expect_err("drive paths should be rejected");
    assert_eq!(
        error,
        AnalyzeError::UnsafePath {
            path: r"C:\docs\README.md".to_string(),
        }
    );
}

#[test]
fn strips_common_prefix_from_github_style_zip() {
    let bytes = build_zip(&[
        (
            "repo-main/README.md",
            "# Hello\n\n![Logo](./images/logo.png)\n",
        ),
        ("repo-main/images/logo.png", "fake-png-bytes"),
    ]);

    let index = analyze_zip_strip_prefix(&bytes).expect("github-style zip should analyze");

    assert_eq!(index.selected_entry.as_deref(), Some("README.md"));
    assert_eq!(index.entry_selection_reason, EntrySelectionReason::Readme);

    let candidate_paths: Vec<&str> = index
        .entry_candidates
        .iter()
        .map(|candidate| candidate.path.as_str())
        .collect();
    assert_eq!(candidate_paths, vec!["README.md"]);

    let resolved_asset_paths: Vec<Option<&str>> = index
        .assets
        .iter()
        .map(|asset| asset.resolved_path.as_deref())
        .collect();
    assert_eq!(resolved_asset_paths, vec![Some("images/logo.png")]);
}

#[test]
fn strip_prefix_preserves_paths_when_no_common_prefix() {
    let bytes = build_zip(&[
        ("README.md", "# Root readme\n"),
        ("docs/guide.md", "# Guide\n"),
    ]);

    let index = analyze_zip_strip_prefix(&bytes).expect("zip without common prefix should analyze");

    let candidate_paths: Vec<&str> = index
        .entry_candidates
        .iter()
        .map(|candidate| candidate.path.as_str())
        .collect();
    assert!(candidate_paths.contains(&"README.md"));
    assert!(candidate_paths.contains(&"docs/guide.md"));
}

#[test]
fn strip_prefix_preserves_paths_when_multiple_top_level_directories() {
    let bytes = build_zip(&[("dir-a/README.md", "# A\n"), ("dir-b/README.md", "# B\n")]);

    let index =
        analyze_zip_strip_prefix(&bytes).expect("zip with multiple top dirs should analyze");

    let candidate_paths: Vec<&str> = index
        .entry_candidates
        .iter()
        .map(|candidate| candidate.path.as_str())
        .collect();
    assert!(candidate_paths.contains(&"dir-a/README.md"));
    assert!(candidate_paths.contains(&"dir-b/README.md"));
}

#[test]
fn strip_prefix_strips_single_nested_file() {
    let bytes = build_zip(&[("only-dir/file.md", "# Single\n")]);

    let index = analyze_zip_strip_prefix(&bytes).expect("single nested file zip should analyze");

    assert_eq!(index.selected_entry.as_deref(), Some("file.md"));
    assert_eq!(
        index.entry_selection_reason,
        EntrySelectionReason::SingleMarkdownFile
    );
}

#[test]
fn regular_analyze_zip_does_not_strip_common_prefix() {
    let bytes = build_zip(&[
        ("repo-main/README.md", "# Hello\n"),
        ("repo-main/images/logo.png", "fake-png-bytes"),
    ]);

    let index = analyze_zip(&bytes).expect("should analyze without stripping");

    assert_eq!(index.selected_entry.as_deref(), Some("repo-main/README.md"));
}
