use std::path::{Path, PathBuf};

use marknest_core::{
    MathMode, MermaidMode, PdfMetadata, RenderHtmlError, RenderOptions, ThemePreset,
    render_workspace_entry, render_workspace_entry_with_options,
};

fn fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

#[test]
fn renders_a_workspace_entry_as_self_contained_html() {
    let rendered = render_workspace_entry(&fixture_path("workspace_valid"), "README.md")
        .expect("workspace entry should render");

    assert_eq!(rendered.title, "README");
    assert!(rendered.html.contains("<h1>MarkNest</h1>"));
    assert!(rendered.html.contains("data:image/svg+xml;base64,"));
    assert!(rendered.html.contains("alt=\"Architecture\""));
    assert!(rendered.html.contains("alt=\"Raw Diagram\""));
    assert!(!rendered.html.contains("./images/architecture.svg"));
    assert!(!rendered.html.contains("./images/raw-diagram.svg"));
}

#[test]
fn returns_a_validation_error_for_an_unknown_entry() {
    let error = render_workspace_entry(&fixture_path("workspace_valid"), "missing.md")
        .expect_err("unknown entry should fail");

    assert_eq!(
        error,
        RenderHtmlError::EntryNotFound {
            entry_path: "missing.md".to_string(),
        }
    );
}

#[test]
fn renders_phase_3_runtime_markup_for_theme_mermaid_and_math() {
    let rendered = render_workspace_entry_with_options(
        &fixture_path("workspace_render_features"),
        "guide.md",
        &RenderOptions {
            theme: ThemePreset::Github,
            metadata: PdfMetadata {
                title: Some("Phase 3 Guide".to_string()),
                author: Some("Docs Team".to_string()),
                subject: Some("Rendering".to_string()),
            },
            custom_css: None,
            mermaid_mode: MermaidMode::Auto,
            math_mode: MathMode::Auto,
        },
    )
    .expect("workspace entry should render");

    assert_eq!(rendered.title, "Phase 3 Guide");
    assert!(rendered.html.contains("theme-github"));
    assert!(
        rendered
            .html
            .contains("./runtime-assets/mermaid/mermaid.min.js")
    );
    assert!(
        rendered
            .html
            .contains("./runtime-assets/mathjax/es5/tex-svg.js")
    );
    assert!(rendered.html.contains("math math-inline"));
    assert!(rendered.html.contains("math math-display"));
    assert!(
        rendered
            .html
            .contains("meta name=\"author\" content=\"Docs Team\"")
    );
    assert!(
        rendered
            .html
            .contains("meta name=\"subject\" content=\"Rendering\"")
    );
    assert!(rendered.html.contains("\"mermaidMode\":\"auto\""));
    assert!(rendered.html.contains("\"mathMode\":\"auto\""));
    assert!(rendered.html.contains("\"mermaidTheme\":\"default\""));
    assert!(!rendered.html.contains("theme: \"neutral\""));
}

#[test]
fn does_not_inject_phase_3_runtime_scripts_when_mermaid_and_math_are_off() {
    let rendered = render_workspace_entry_with_options(
        &fixture_path("workspace_render_features"),
        "guide.md",
        &RenderOptions::default(),
    )
    .expect("workspace entry should render");

    assert!(
        !rendered
            .html
            .contains("./runtime-assets/mermaid/mermaid.min.js")
    );
    assert!(
        !rendered
            .html
            .contains("./runtime-assets/mathjax/es5/tex-svg.js")
    );
}

#[test]
fn applies_custom_css_after_the_theme_stylesheet() {
    let rendered = render_workspace_entry_with_options(
        &fixture_path("workspace_valid"),
        "README.md",
        &RenderOptions {
            theme: ThemePreset::Github,
            custom_css: Some("body { color: rgb(1, 2, 3); }".to_string()),
            ..RenderOptions::default()
        },
    )
    .expect("workspace entry should render");

    let theme_offset = rendered
        .html
        .find(".theme-github")
        .expect("theme stylesheet should be present");
    let custom_css_offset = rendered
        .html
        .find("body { color: rgb(1, 2, 3); }")
        .expect("custom css should be present");

    assert!(custom_css_offset > theme_offset);
}

#[test]
fn renders_query_suffixed_local_assets_as_inlined_data_uris() {
    let rendered =
        render_workspace_entry(&fixture_path("workspace_asset_query_suffix"), "README.md")
            .expect("workspace entry should render");

    assert!(rendered.html.contains("alt=\"Query Asset\""));
    assert!(rendered.html.contains("alt=\"Raw Query Asset\""));
    assert!(rendered.html.contains("data:image/svg+xml;base64,"));
    assert!(!rendered.html.contains("./images/query-asset.svg?raw=true"));
    assert!(!rendered.html.contains("./images/raw-asset.svg?raw=true"));
}

#[test]
fn renders_long_code_lines_with_print_safe_wrapping_styles() {
    let rendered = render_workspace_entry(&fixture_path("workspace_long_code_line"), "README.md")
        .expect("workspace entry should render");

    assert!(rendered.html.contains("REMOTE_IMAGE_URL"));
    assert!(rendered.html.contains("pre {"));
    assert!(rendered.html.contains("white-space: pre-wrap;"));
    assert!(rendered.html.contains("overflow-wrap: anywhere;"));
}

#[test]
fn preserves_inline_badges_and_explicit_image_dimensions() {
    let rendered = render_workspace_entry(&fixture_path("workspace_image_layout"), "README.md")
        .expect("workspace entry should render");

    assert!(rendered.html.contains("alt=\"Project Logo\""));
    assert!(rendered.html.contains("height=\"150\""));
    assert!(
        rendered
            .html
            .contains("img { max-width: 100%; vertical-align: middle; }")
    );
    assert!(rendered.html.contains("p > img:only-child"));
    assert!(rendered.html.contains("p > a:only-child > img"));
    assert!(rendered.html.contains("body > img"));
    assert!(
        !rendered
            .html
            .contains("img { display: block; max-width: 100%; height: auto;")
    );
    assert!(
        !rendered
            .html
            .contains("img { max-width: 100%; height: auto;")
    );
}

#[test]
fn renders_github_emoji_shortcodes_in_prose_but_not_in_code() {
    let rendered = render_workspace_entry(&fixture_path("workspace_emoji_shortcodes"), "README.md")
        .expect("workspace entry should render");

    assert!(rendered.html.contains("Winner 🏆 in prose."));
    assert!(rendered.html.contains("<code>:trophy:</code>"));
    assert!(rendered.html.contains("<pre><code>:trophy:"));
}
