use marknest_core::{
    PdfMetadata, RenderHtmlError, RenderOptions, ThemePreset, render_markdown_entry,
    render_markdown_entry_with_options,
};

#[test]
fn renders_a_single_markdown_as_self_contained_html() {
    let markdown_bytes = b"# Hello World\n\nThis is a test paragraph.\n";

    let rendered =
        render_markdown_entry(markdown_bytes, "README.md").expect("single markdown should render");

    assert_eq!(rendered.title, "README");
    assert!(
        rendered
            .html
            .contains("<h1 id=\"hello-world\">Hello World</h1>")
    );
    assert!(rendered.html.contains("This is a test paragraph."));
}

#[test]
fn renders_single_markdown_with_custom_options() {
    let markdown_bytes = b"# Preview\n\nSome content.\n";

    let rendered = render_markdown_entry_with_options(
        markdown_bytes,
        "guide.md",
        &RenderOptions {
            theme: ThemePreset::Docs,
            metadata: PdfMetadata {
                title: Some("Custom Title".to_string()),
                author: Some("Test Author".to_string()),
                subject: Some("Testing".to_string()),
            },
            ..RenderOptions::default()
        },
    )
    .expect("single markdown should render with options");

    assert_eq!(rendered.title, "Custom Title");
    assert!(rendered.html.contains("theme-docs"));
    assert!(
        rendered
            .html
            .contains("meta name=\"author\" content=\"Test Author\"")
    );
}

#[test]
fn derives_title_from_filename_when_no_metadata_title() {
    let markdown_bytes = b"# Content\n";

    let rendered = render_markdown_entry(markdown_bytes, "getting-started.md")
        .expect("title should derive from filename");

    assert_eq!(rendered.title, "getting-started");
}

#[test]
fn renders_single_markdown_with_unresolvable_local_images() {
    let markdown_bytes = b"# Guide\n\n![Diagram](./images/arch.png)\n";

    let rendered = render_markdown_entry(markdown_bytes, "guide.md")
        .expect("single markdown should render even with unresolvable local images");

    assert!(rendered.html.contains("<h1 id=\"guide\">Guide</h1>"));
    // Local image reference stays as-is since there are no accompanying files
    assert!(rendered.html.contains("./images/arch.png"));
}

#[test]
fn rejects_non_utf8_markdown_bytes() {
    let invalid_utf8: &[u8] = &[0xFF, 0xFE, 0x00, 0x01];

    let error = render_markdown_entry(invalid_utf8, "broken.md")
        .expect_err("non-UTF-8 markdown should fail");

    assert_eq!(
        error,
        RenderHtmlError::InvalidUtf8 {
            entry_path: "broken.md".to_string(),
        }
    );
}
