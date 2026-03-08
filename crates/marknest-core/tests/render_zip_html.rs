use std::io::Write;

use marknest_core::{
    PdfMetadata, RenderHtmlError, RenderOptions, ThemePreset, render_zip_entry,
    render_zip_entry_with_options,
};
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
fn renders_a_zip_entry_as_self_contained_html() {
    let zip_bytes = build_zip(&[
        (
            "docs/README.md",
            "# Zip Guide\n\n![Architecture](../images/architecture.svg)\n",
        ),
        (
            "images/architecture.svg",
            "<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>",
        ),
    ]);

    let rendered = render_zip_entry(&zip_bytes, "docs/README.md").expect("zip entry should render");

    assert_eq!(rendered.title, "README");
    assert!(
        rendered
            .html
            .contains("<h1 id=\"zip-guide\">Zip Guide</h1>")
    );
    assert!(rendered.html.contains("data:image/svg+xml;base64,"));
    assert!(!rendered.html.contains("../images/architecture.svg"));
}

#[test]
fn renders_phase_5_zip_preview_markup_with_options() {
    let zip_bytes = build_zip(&[
        ("guide.md", "# Preview\n"),
        (
            "images/raw-diagram.svg",
            "<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>",
        ),
    ]);

    let rendered = render_zip_entry_with_options(
        &zip_bytes,
        "guide.md",
        &RenderOptions {
            theme: ThemePreset::Docs,
            metadata: PdfMetadata {
                title: Some("Preview Title".to_string()),
                author: Some("Docs Team".to_string()),
                subject: Some("Preview".to_string()),
            },
            ..RenderOptions::default()
        },
    )
    .expect("zip entry should render");

    assert_eq!(rendered.title, "Preview Title");
    assert!(rendered.html.contains("theme-docs"));
    assert!(
        rendered
            .html
            .contains("meta name=\"author\" content=\"Docs Team\"")
    );
}

#[test]
fn returns_a_validation_error_for_an_unknown_zip_entry() {
    let zip_bytes = build_zip(&[("README.md", "# Zip Guide\n")]);

    let error = render_zip_entry(&zip_bytes, "missing.md").expect_err("missing entry should fail");

    assert_eq!(
        error,
        RenderHtmlError::EntryNotFound {
            entry_path: "missing.md".to_string(),
        }
    );
}
