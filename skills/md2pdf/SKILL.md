---
name: md2pdf
description: >
  Convert Markdown files to professional PDFs using marknest — a Rust-powered CLI that handles
  single files, entire folders, ZIP archives, and even GitHub repository URLs. Use this skill
  whenever the user wants to turn .md files into PDFs, generate printable documentation, export
  markdown as PDF, batch-convert a docs folder, or create styled PDF reports from markdown.
  Also trigger when the user mentions "markdown to PDF", "md to pdf", "export as PDF",
  "print markdown", "PDF from README", "convert docs to PDF", or asks to make markdown
  files look professional/printable. This skill covers theming, table of contents generation,
  Mermaid diagram rendering, LaTeX math support, and custom styling — so use it even if the
  user asks for "pretty PDFs", "styled documentation", or "PDF with diagrams and math".
---

# Marknest: Markdown to PDF Converter

You are helping the user convert Markdown files into professional PDFs using **marknest**, a fast Rust-powered CLI tool.

## Why marknest?

marknest handles everything needed for high-quality PDF output from Markdown: embedded images, Mermaid diagrams, LaTeX math, syntax-highlighted code blocks, table of contents, and multiple built-in themes. It works locally with no cloud dependencies — all rendering assets are vendored. It accepts single files, folders, ZIP archives, and GitHub URLs as input.

## Installation

Before using marknest, check if it's available:

```bash
npx marknest --help
```

If not installed, install it. Prefer `npx` (zero-install) or npm global:

```bash
# Option 1: Use npx (no install needed, recommended)
npx marknest convert ...

# Option 2: Install globally via npm
npm install -g marknest

# Option 3: Install via Cargo (if Rust toolchain is available)
cargo install marknest
```

**Browser requirement**: marknest uses Chromium for PDF rendering. If the user hits a browser-related error, run:

```bash
npx playwright install chromium
```

## Core Commands

### Single file conversion

```bash
npx marknest convert README.md -o README.pdf
```

### Batch conversion (folder)

```bash
npx marknest convert ./docs --all --out-dir ./pdf
```

### From GitHub URL

```bash
npx marknest convert https://github.com/user/repo/blob/main/docs/guide.md -o guide.pdf
```

### From ZIP archive

```bash
npx marknest convert ./docs.zip --all --out-dir ./pdf
```

## Styling & Themes

marknest has four built-in themes. Choose one that fits the user's needs:

| Theme     | Best for                              |
|-----------|---------------------------------------|
| `default` | General purpose, clean look           |
| `github`  | GitHub-flavored markdown appearance   |
| `docs`    | Technical documentation               |
| `plain`   | Minimal, no frills                    |

```bash
npx marknest convert README.md -o README.pdf --theme github
```

For custom styling, use `--css`:

```bash
npx marknest convert README.md -o README.pdf --css custom.css
```

## Advanced Features

### Table of Contents

```bash
npx marknest convert README.md -o README.pdf --toc
```

### Mermaid Diagrams

If the markdown contains Mermaid code blocks, enable rendering:

```bash
npx marknest convert README.md -o README.pdf --mermaid auto
```

### LaTeX Math

If the markdown contains LaTeX math expressions:

```bash
npx marknest convert README.md -o README.pdf --math auto
```

### Page Layout

```bash
# Landscape orientation
npx marknest convert README.md -o README.pdf --landscape

# Custom page size
npx marknest convert README.md -o README.pdf --page-size letter

# Custom margins (pixels)
npx marknest convert README.md -o README.pdf --margin-top 40 --margin-bottom 40
```

### PDF Metadata

```bash
npx marknest convert README.md -o README.pdf --title "Project Guide" --author "Team"
```

### Header/Footer Templates

```bash
npx marknest convert README.md -o README.pdf --header-template header.html
```

## Validation

Before converting, you can validate markdown files to catch broken links and rendering issues:

```bash
npx marknest validate README.md
npx marknest validate ./docs --all --strict
```

## Combining Multiple Options

A typical full-featured conversion command:

```bash
npx marknest convert README.md -o README.pdf \
  --theme github \
  --toc \
  --mermaid auto \
  --math auto \
  --title "Project Documentation" \
  --author "Team Name"
```

## Decision Guide

When the user asks to convert markdown to PDF, follow this logic:

1. **Identify the input**: Single file? Folder? ZIP? GitHub URL?
2. **Ask about styling** (if not specified): Theme preference? TOC needed?
3. **Check for special content**: Does the markdown have Mermaid diagrams or LaTeX math? If yes, add `--mermaid auto` and/or `--math auto`.
4. **Determine output**: Single PDF (`-o`) or batch (`--out-dir`)?
5. **Run the command** and verify the output file exists.

If the user has a `.marknest.toml` config file in their project, marknest will pick it up automatically — no need to pass flags that are already configured there.

## Troubleshooting

| Issue | Solution |
|-------|----------|
| "Browser not found" | Run `npx playwright install chromium` |
| "Playwright runtime package was not found" | Run `npm ci --prefix <path shown in error>`, or set `MARKNEST_PLAYWRIGHT_RUNTIME_DIR` to a directory containing the playwright runtime |
| Mermaid diagrams not rendering | Add `--mermaid auto` flag |
| Math equations not rendering | Add `--math auto` flag |
| Images missing in PDF | Check relative paths; marknest resolves them from the workspace root |
| Permission denied on output | Check write permissions on the output directory |
