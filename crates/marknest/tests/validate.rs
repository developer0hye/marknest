use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;
use tempfile::TempDir;
use zip::write::SimpleFileOptions;

fn fixtures_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("marknest-core")
        .join("tests")
        .join("fixtures")
}

fn fixture_path(name: &str) -> PathBuf {
    fixtures_root().join(name)
}

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_marknest")
}

fn run_validate(args: &[&str]) -> Output {
    Command::new(binary_path())
        .args(["validate"])
        .args(args)
        .output()
        .expect("validate command should run")
}

fn run_validate_in_dir(directory: &Path, args: &[&str]) -> Output {
    Command::new(binary_path())
        .current_dir(directory)
        .args(["validate"])
        .args(args)
        .output()
        .expect("validate command should run")
}

fn stdout_text(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("stdout should be utf-8")
}

fn stderr_text(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("stderr should be utf-8")
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

#[test]
fn validates_a_single_markdown_file_input() {
    let markdown_path = fixture_path("workspace_valid").join("README.md");

    let output = run_validate(&[markdown_path.to_str().expect("path should be utf-8")]);

    assert_eq!(output.status.code(), Some(0));
    assert!(stdout_text(&output).contains("Validation succeeded"));
    assert!(stdout_text(&output).contains("README.md"));
    assert_eq!(stderr_text(&output), "");
}

#[test]
fn validates_a_zip_input() {
    let (_temp_dir, zip_path) = build_zip_file(&[
        ("README.md", "# Zip\n\n![Asset](./images/diagram.svg)\n"),
        (
            "images/diagram.svg",
            "<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>",
        ),
    ]);

    let output = run_validate(&[zip_path.to_str().expect("path should be utf-8")]);

    assert_eq!(output.status.code(), Some(0));
    assert!(stdout_text(&output).contains("Validation succeeded"));
    assert!(stdout_text(&output).contains("README.md"));
}

#[test]
fn validates_all_entries_for_an_explicit_folder_input() {
    let folder_path = fixture_path("workspace_multiple_entries");

    let output = run_validate(&[folder_path.to_str().expect("path should be utf-8")]);

    assert_eq!(output.status.code(), Some(0));
    assert!(stdout_text(&output).contains("Validation succeeded"));
    assert!(stdout_text(&output).contains("guide.md"));
    assert!(stdout_text(&output).contains("tutorial.markdown"));
}

#[test]
fn returns_a_warning_exit_code_for_missing_assets_without_strict_mode() {
    let folder_path = fixture_path("workspace_missing_asset");

    let output = run_validate(&[folder_path.to_str().expect("path should be utf-8")]);

    assert_eq!(output.status.code(), Some(1));
    assert!(stdout_text(&output).contains("Validation completed with warnings"));
    assert!(stdout_text(&output).contains("does-not-exist.svg"));
}

#[test]
fn returns_a_validation_failure_for_missing_assets_in_strict_mode() {
    let folder_path = fixture_path("workspace_missing_asset");

    let output = run_validate(&[
        folder_path.to_str().expect("path should be utf-8"),
        "--strict",
    ]);

    assert_eq!(output.status.code(), Some(2));
    assert!(stderr_text(&output).contains("Validation failed"));
    assert!(stderr_text(&output).contains("does-not-exist.svg"));
}

#[test]
fn writes_a_json_report_file() {
    let markdown_path = fixture_path("workspace_valid").join("README.md");
    let temp_dir = TempDir::new().expect("temp dir should be created");
    let report_path = temp_dir.path().join("report.json");

    let output = run_validate(&[
        markdown_path.to_str().expect("path should be utf-8"),
        "--report",
        report_path.to_str().expect("path should be utf-8"),
    ]);

    assert_eq!(output.status.code(), Some(0));
    assert!(report_path.exists());

    let report_json: Value = serde_json::from_str(
        &fs::read_to_string(&report_path).expect("report file should be readable"),
    )
    .expect("report should be valid json");

    assert_eq!(report_json["selected_entries"][0], "README.md");
    assert_eq!(report_json["entry_selection_reason"], "readme");
    assert_eq!(report_json["errors"], Value::Array(Vec::new()));
}

#[test]
fn fails_without_entry_selection_when_current_directory_has_multiple_markdown_files() {
    let folder_path = fixture_path("workspace_multiple_entries");

    let output = run_validate_in_dir(&folder_path, &[]);

    assert_eq!(output.status.code(), Some(2));
    assert!(stderr_text(&output).contains("Multiple markdown files were detected"));
    assert!(stderr_text(&output).contains("--entry"));
    assert!(stderr_text(&output).contains("--all"));
}
