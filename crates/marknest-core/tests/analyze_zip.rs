use std::io::{Cursor, Write};

use marknest_core::{AnalyzeError, EntrySelectionReason, ProjectSourceKind, analyze_zip};
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
