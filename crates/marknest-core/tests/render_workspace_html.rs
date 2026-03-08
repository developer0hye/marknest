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
    assert!(rendered.html.contains("<h1 id=\"marknest\">MarkNest</h1>"));
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
            enable_toc: false,
            sanitize_html: true,
            mermaid_mode: MermaidMode::Auto,
            math_mode: MathMode::Auto,
            mermaid_timeout_ms: 5_000,
            math_timeout_ms: 3_000,
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
    assert!(rendered.html.contains("\"mermaidTimeoutMs\":5000"));
    assert!(rendered.html.contains("\"mathTimeoutMs\":3000"));
    assert!(!rendered.html.contains("theme: \"neutral\""));
}

#[test]
fn renders_explicit_runtime_timeout_overrides() {
    let rendered = render_workspace_entry_with_options(
        &fixture_path("workspace_render_features"),
        "guide.md",
        &RenderOptions {
            mermaid_mode: MermaidMode::Auto,
            math_mode: MathMode::Auto,
            mermaid_timeout_ms: 1200,
            math_timeout_ms: 800,
            ..RenderOptions::default()
        },
    )
    .expect("workspace entry should render");

    assert!(rendered.html.contains("\"mermaidTimeoutMs\":1200"));
    assert!(rendered.html.contains("\"mathTimeoutMs\":800"));
}

#[test]
fn runtime_status_uses_dom_content_loaded_instead_of_window_load() {
    let rendered = render_workspace_entry_with_options(
        &fixture_path("workspace_render_features"),
        "guide.md",
        &RenderOptions {
            mermaid_mode: MermaidMode::Auto,
            math_mode: MathMode::Auto,
            ..RenderOptions::default()
        },
    )
    .expect("workspace entry should render");

    assert!(
        rendered
            .html
            .contains("document.readyState === \"loading\"")
    );
    assert!(
        rendered
            .html
            .contains("document.addEventListener(\"DOMContentLoaded\"")
    );
    assert!(rendered.html.contains("void finalizeRendering();"));
    assert!(!rendered.html.contains("window.addEventListener(\"load\""));
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
fn renders_repo_root_relative_assets_as_inlined_data_uris() {
    let rendered =
        render_workspace_entry(&fixture_path("workspace_root_relative_asset"), "README.md")
            .expect("workspace entry should render");

    assert!(rendered.html.contains("alt=\"Root Relative\""));
    assert!(rendered.html.contains("alt=\"Raw Root Relative\""));
    assert!(rendered.html.contains("data:image/svg+xml;base64,"));
    assert!(!rendered.html.contains("/docs/images/root-relative.svg"));
    assert!(!rendered.html.contains("/docs/images/raw-root-relative.svg"));
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
fn adds_print_layout_guards_for_sections_and_unsplittable_blocks() {
    let rendered = render_workspace_entry_with_options(
        &fixture_path("workspace_render_features"),
        "guide.md",
        &RenderOptions {
            mermaid_mode: MermaidMode::Auto,
            ..RenderOptions::default()
        },
    )
    .expect("workspace entry should render");

    assert!(rendered.html.contains("@media print"));
    assert!(
        rendered
            .html
            .contains("h1 { break-before: page; page-break-before: always; }")
    );
    assert!(
        rendered
            .html
            .contains("h1:first-of-type { break-before: auto; page-break-before: auto; }")
    );
    assert!(rendered.html.contains(
        "pre, table, blockquote, figure, img, tr, .marknest-toc { break-inside: avoid; page-break-inside: avoid; }"
    ));
    assert!(
        rendered
            .html
            .contains("thead { display: table-header-group; }")
    );
}

#[test]
fn keeps_collapsed_details_content_visible_in_print_output() {
    let rendered =
        render_workspace_entry(&fixture_path("workspace_collapsed_details"), "README.md")
            .expect("workspace entry should render");

    assert!(rendered.html.contains("<details open>"));
    assert!(rendered.html.contains("<summary>Model Families</summary>"));
    assert!(
        rendered
            .html
            .contains("segmentation, classification, pose, and OBB")
    );
    assert!(
        rendered
            .html
            .contains("details:not([open]) > :not(summary) { display: block; }")
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

#[test]
fn sanitizes_raw_html_by_default_while_preserving_safe_readme_markup() {
    let rendered =
        render_workspace_entry(&fixture_path("workspace_raw_html_sanitize"), "README.md")
            .expect("workspace entry should render");

    assert!(!rendered.html.contains("<script"));
    assert!(!rendered.html.contains("onclick="));
    assert!(!rendered.html.contains("onerror="));
    assert!(!rendered.html.contains("<iframe"));
    assert!(rendered.html.contains("<details"));
    assert!(rendered.html.contains("<summary>More</summary>"));
    assert!(rendered.html.contains("type=\"checkbox\""));
    assert!(rendered.html.contains("checked=\"\""));
    assert!(rendered.html.contains("disabled=\"\""));
    assert!(rendered.html.contains("width=\"120\""));
    assert!(rendered.html.contains("height=\"80\""));
    assert!(rendered.html.contains("data:image/svg+xml;base64,"));
}

#[test]
fn can_disable_html_sanitization_for_trusted_documents() {
    let rendered = render_workspace_entry_with_options(
        &fixture_path("workspace_raw_html_sanitize"),
        "README.md",
        &RenderOptions {
            sanitize_html: false,
            ..RenderOptions::default()
        },
    )
    .expect("workspace entry should render");

    assert!(rendered.html.contains("<script>alert(\"x\")</script>"));
    assert!(rendered.html.contains("onclick=\"alert('x')\""));
    assert!(rendered.html.contains("onerror=\"alert('img')\""));
    assert!(
        rendered
            .html
            .contains("<iframe src=\"https://example.com/embed\"></iframe>")
    );
}

#[test]
fn builds_an_optional_table_of_contents_with_stable_heading_ids() {
    let rendered = render_workspace_entry_with_options(
        &fixture_path("workspace_toc"),
        "README.md",
        &RenderOptions {
            enable_toc: true,
            ..RenderOptions::default()
        },
    )
    .expect("workspace entry should render");

    assert!(rendered.html.contains("marknest-toc"));
    assert!(rendered.html.contains("href=\"#guide\""));
    assert!(rendered.html.contains("href=\"#overview\""));
    assert!(rendered.html.contains("href=\"#overview-2\""));
    assert!(rendered.html.contains("href=\"#api-surface\""));
    assert!(rendered.html.contains("<h1 id=\"guide\">Guide</h1>"));
    assert!(rendered.html.contains("<h2 id=\"overview\">Overview</h2>"));
    assert!(
        rendered
            .html
            .contains("<h2 id=\"overview-2\">Overview</h2>")
    );
    assert!(
        rendered
            .html
            .contains("<h3 id=\"api-surface\">API Surface</h3>")
    );
}
