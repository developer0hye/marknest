# MarkNest

[![CI](https://github.com/developer0hye/marknest/actions/workflows/readme-corpus.yml/badge.svg)](https://github.com/developer0hye/marknest/actions/workflows/readme-corpus.yml)
[![Crates.io](https://img.shields.io/crates/v/marknest)](https://crates.io/crates/marknest)
[![npm](https://img.shields.io/npm/v/marknest)](https://www.npmjs.com/package/marknest)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

A Markdown workspace analyzer and PDF converter. Upload a ZIP, point to a folder, or pass a single `.md` file — MarkNest resolves local images, renders Mermaid diagrams and math, and produces print-ready PDFs.

**Try it online:** [dontsendfile.com/md2pdf](https://www.dontsendfile.com/md2pdf)

https://github.com/user-attachments/assets/489b1b61-a840-4374-8eb5-d90d5fa1f4db

## Features

- **Workspace-aware** — analyzes folders, ZIP archives, and GitHub URLs; resolves relative image paths automatically
- **Mermaid & Math** — renders fenced Mermaid diagrams to SVG and LaTeX math via MathJax, with configurable `off`/`auto`/`on` modes
- **Themes & styling** — built-in presets (`default`, `github`, `docs`, `plain`), custom CSS, header/footer templates, per-side margins
- **Batch conversion** — convert all Markdown files in a workspace at once, preserving directory structure
- **Browser & CLI** — WASM-powered browser preview with no server upload, plus a native CLI for local and CI workflows
- **PDF quality controls** — page size, orientation, table of contents, PDF metadata, print-layout page breaks
- **Security built in** — Zip Slip prevention, path traversal blocking, HTML sanitization on by default, archive size limits
- **Offline rendering** — Mermaid, MathJax, and html2pdf.js assets are vendored locally; no external CDN required
- **Remote images** — best-effort fetch and inline of HTTP images across CLI, browser, and fallback flows
- **Local fallback server** — Playwright-driven Chromium PDF generation for high-quality output when browser export isn't enough
- **Docker image** — single image bundles CLI, fallback server, Chromium, and fonts for reproducible builds

## Quick Start

```bash
# Install
cargo install marknest

# Install a headless browser for PDF rendering
npx playwright install chromium

# Convert a Markdown file to PDF
marknest convert README.md -o README.pdf
```

## Installation

### Cargo (from crates.io)

```bash
cargo install marknest
```

### npm / npx

```bash
# Run without installing
npx marknest convert README.md -o output.pdf

# Or install globally
npm install -g marknest
marknest convert README.md -o output.pdf
```

### From source

```bash
git clone https://github.com/developer0hye/marknest.git
cd marknest
cargo install --path crates/marknest
```

### Docker

```bash
docker build -t marknest .

# CLI usage
docker run --rm -v "$PWD":/work -w /work marknest marknest convert README.md -o README.pdf

# Fallback server (default CMD)
docker run --rm -p 3476:3476 marknest
```

### Browser prerequisite

PDF rendering requires a local Chromium-based browser. If none is installed:

```bash
npx playwright install chromium
# or for headless-only environments:
npx playwright install --with-deps chromium
```

Browser discovery order: `MARKNEST_BROWSER_PATH` env var > Playwright headless shell > system Chrome/Edge/Chromium.

## Usage

### Validate

Check workspace structure, image links, and Mermaid/Math blocks without generating a PDF.

```bash
marknest validate README.md
marknest validate ./docs.zip --entry docs/README.md
marknest validate ./docs --all --report report.json
marknest validate ./docs --all --strict
```

### Convert — single file

```bash
marknest convert README.md -o README.pdf
marknest convert README.md --theme github --landscape -o README.pdf
marknest convert README.md --toc --mermaid auto --math auto -o README.pdf
marknest convert README.md --css custom.css --header-template header.html -o README.pdf
marknest convert README.md --page-size letter --margin-top 24 --margin-bottom 24 -o README.pdf
marknest convert README.md --title "Guide" --author "Team" --subject "Arch" -o README.pdf
```

### Convert — ZIP or folder (batch)

```bash
marknest convert ./docs --out-dir ./pdf
marknest convert ./docs.zip --all --out-dir ./pdf
marknest convert ./docs.zip --entry docs/README.md -o out.pdf
```

### Convert — GitHub URL

```bash
marknest convert https://github.com/user/repo -o output.pdf
marknest convert https://github.com/user/repo/blob/main/docs/guide.md -o guide.pdf
marknest convert https://github.com/user/repo/tree/v2.0 --all --out-dir ./pdf
```

Set `GITHUB_TOKEN` or `GH_TOKEN` for private repositories or to avoid API rate limits.

### Debug artifacts

```bash
marknest convert README.md \
  --debug-html ./out/debug.html \
  --asset-manifest ./out/assets.json \
  --render-report ./out/report.json \
  -o README.pdf
```

### Configuration

Settings can be provided through CLI flags, a config file, or environment variables.

**Precedence:** CLI args > config file > environment variables > built-in defaults

```bash
# Use a config file
marknest convert README.md --config .marknest.toml -o README.pdf
```

Config files are auto-discovered from `.marknest.toml` or `marknest.toml` in the working directory.

| Environment Variable       | Purpose                        |
| -------------------------- | ------------------------------ |
| `MARKNEST_CONFIG`          | Path to config file            |
| `MARKNEST_THEME`           | Default theme                  |
| `MARKNEST_CSS`             | Path to custom CSS             |
| `MARKNEST_TOC`             | Enable/disable TOC             |
| `MARKNEST_SANITIZE_HTML`   | Enable/disable HTML sanitization |
| `MARKNEST_BROWSER_PATH`    | Path to Chromium-based browser |
| `MARKNEST_SERVER_ADDR`     | Fallback server bind address   |

### Exit codes

| Code | Meaning            |
| ---- | ------------------ |
| `0`  | Success            |
| `1`  | Success with warnings |
| `2`  | Validation failure |
| `3`  | System failure     |

## Web App

The browser app provides ZIP upload, entry selection, HTML preview, and PDF download — all client-side via WASM.

```bash
trunk serve
```

Export modes:

- **Browser Fast** — client-only PDF generation, no data leaves the browser
- **High Quality Fallback** — sends ZIP to the local fallback server for Playwright/Chromium rendering

The web app also supports theme/CSS/margin/TOC/metadata controls, Mermaid/Math preview, debug bundle download, and batch ZIP export.

## Fallback Server

```bash
cargo run -p marknest-server
```

Listens on `http://127.0.0.1:3476` by default (override with `MARKNEST_SERVER_ADDR`). Accepts multipart ZIP uploads with output options and returns PDF or batch ZIP.

## Project Structure

| Path | Description |
| ---- | ----------- |
| `Cargo.toml` | Workspace definition |
| `crates/marknest` | CLI binary: validation, conversion, config resolution, Playwright PDF |
| `crates/marknest-core` | Core library: workspace analysis, HTML rendering |
| `crates/marknest-server` | Local HTTP fallback service |
| `crates/marknest-wasm` | WASM bindings for browser analysis and rendering |
| `index.html`, `web/` | Browser app (Trunk) |
| `runtime-assets/` | Vendored Mermaid, MathJax, html2pdf.js |
| `validation/` | 60-entry README corpus and PDF fidelity validator |
| `Dockerfile` | Shared CLI + server runtime image |

## Development

```bash
# Format
cargo fmt --all

# Test
cargo test

# Check WASM target
cargo check -p marknest-wasm --target wasm32-unknown-unknown

# Install Playwright runtime for native PDF
npm ci --prefix crates/marknest/playwright-runtime

# Install validation dependencies
npm ci --prefix validation
```

## License

[MIT](LICENSE)
