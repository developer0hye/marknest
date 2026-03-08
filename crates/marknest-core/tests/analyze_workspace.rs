use std::fs;
use std::path::{Path, PathBuf};

use marknest_core::{EntrySelectionReason, ProjectSourceKind, analyze_workspace};

fn fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

#[test]
fn analyzes_a_workspace_with_resolved_assets_and_stable_json() {
    let index = analyze_workspace(&fixture_path("workspace_valid"))
        .expect("valid fixture should analyze successfully");

    assert_eq!(index.source_kind, ProjectSourceKind::Workspace);
    assert_eq!(index.selected_entry.as_deref(), Some("README.md"));
    assert_eq!(index.entry_selection_reason, EntrySelectionReason::Readme);

    let actual = serde_json::to_string_pretty(&index).expect("json serialization should succeed");
    let expected = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("golden")
            .join("workspace_valid.json"),
    )
    .expect("golden json should exist");

    assert_eq!(actual.trim_end(), expected.trim_end());
}

#[test]
fn reports_missing_assets_for_markdown_and_raw_html_images() {
    let index = analyze_workspace(&fixture_path("workspace_missing_asset"))
        .expect("workspace with missing assets should still analyze");

    assert_eq!(
        index.diagnostic.missing_assets,
        vec![
            "guide.md -> ./images/does-not-exist.svg".to_string(),
            "guide.md -> ./images/raw-missing.svg".to_string(),
        ]
    );
    assert_eq!(index.selected_entry.as_deref(), Some("guide.md"));
    assert_eq!(
        index.entry_selection_reason,
        EntrySelectionReason::SingleMarkdownFile
    );
}

#[test]
fn reports_multiple_markdown_candidates_without_default_entry() {
    let index = analyze_workspace(&fixture_path("workspace_multiple_entries"))
        .expect("workspace with multiple markdown files should analyze");

    assert_eq!(index.selected_entry, None);
    assert_eq!(
        index.entry_selection_reason,
        EntrySelectionReason::MultipleCandidates
    );
    assert_eq!(index.entry_candidates.len(), 2);
    assert_eq!(index.diagnostic.missing_assets, Vec::<String>::new());
}

#[test]
fn resolves_local_assets_even_when_the_reference_has_a_query_suffix() {
    let index = analyze_workspace(&fixture_path("workspace_asset_query_suffix"))
        .expect("workspace with query-suffixed local assets should analyze");

    assert_eq!(index.diagnostic.missing_assets, Vec::<String>::new());
    assert!(index.assets.iter().any(|asset| {
        asset.original_reference == "./images/query-asset.svg?raw=true"
            && asset.resolved_path.as_deref() == Some("images/query-asset.svg")
    }));
    assert!(index.assets.iter().any(|asset| {
        asset.original_reference == "./images/raw-asset.svg?raw=true"
            && asset.resolved_path.as_deref() == Some("images/raw-asset.svg")
    }));
}

#[test]
fn resolves_repo_root_relative_assets_with_leading_slashes() {
    let index = analyze_workspace(&fixture_path("workspace_root_relative_asset"))
        .expect("workspace with repo-root-relative assets should analyze");

    assert_eq!(index.diagnostic.missing_assets, Vec::<String>::new());
    assert!(index.assets.iter().any(|asset| {
        asset.original_reference == "/docs/images/root-relative.svg"
            && asset.resolved_path.as_deref() == Some("docs/images/root-relative.svg")
    }));
    assert!(index.assets.iter().any(|asset| {
        asset.original_reference == "/docs/images/raw-root-relative.svg"
            && asset.resolved_path.as_deref() == Some("docs/images/raw-root-relative.svg")
    }));
}

#[test]
fn normalizes_remote_http_assets_into_fetch_urls() {
    let index = analyze_workspace(&fixture_path("workspace_remote_http_assets"))
        .expect("workspace with remote assets should analyze");

    assert_eq!(index.diagnostic.missing_assets, Vec::<String>::new());
    assert!(index.assets.iter().any(|asset| {
        asset.original_reference
            == "https://github.com/facebookresearch/segment-anything-2/blob/main/assets/model_diagram.png?raw=true"
            && asset.fetch_url.as_deref()
                == Some(
                    "https://github.com/facebookresearch/segment-anything-2/raw/main/assets/model_diagram.png"
                )
            && asset.resolved_path.is_none()
    }));
    assert!(index.assets.iter().any(|asset| {
        asset.original_reference
            == "https://github.com/pytorch/pytorch/raw/main/docs/source/_static/img/pytorch-logo-dark.png"
            && asset.fetch_url.as_deref()
                == Some(
                    "https://github.com/pytorch/pytorch/raw/main/docs/source/_static/img/pytorch-logo-dark.png"
                )
            && asset.resolved_path.is_none()
    }));
    assert!(index.assets.iter().any(|asset| {
        asset.original_reference
            == "https://raw.githubusercontent.com/mermaid-js/mermaid/develop/packages/mermaid/src/docs/assets/hero.png"
            && asset.fetch_url.as_deref()
                == Some(
                    "https://raw.githubusercontent.com/mermaid-js/mermaid/develop/packages/mermaid/src/docs/assets/hero.png"
                )
            && asset.resolved_path.is_none()
    }));
}
