# MarkNest

MarkNest is a Rust-first Markdown workspace analyzer and PDF converter for the product described in [PRD.md](./PRD.md).

Key capabilities:

- `marknest-core` analyzes a workspace directory or ZIP archive.
- It returns a reproducible `ProjectIndex` with entry candidates, resolved or missing image assets, ignored files, and path diagnostics.
- `marknest-core` can render a single workspace or ZIP entry into self-contained HTML with local images inlined as data URIs, including GitHub-style repo-root image paths that start with `/`, normalized remote HTTP image metadata, GitHub-style emoji shortcodes in prose, generated heading anchors and optional TOC markup, built-in theme presets, custom CSS overrides, and optional Mermaid/Math runtime hooks backed by vendored local runtime assets.
- ZIP analysis blocks path traversal, absolute paths, Windows drive paths, and oversized archives.
- `marknest` provides a `validate` CLI for `.md`, `.zip`, and folder inputs.
- `marknest` provides a conversion CLI with config file, debug artifact, and print template support.
- `marknest` now also exposes a reusable HTML-to-PDF helper for local fallback services.
- `marknest-wasm` exposes browser bindings for ZIP analysis, output-aware HTML preview rendering, direct single-markdown rendering (bypassing ZIP), batch preview rendering, ZIP packaging of generated PDFs, and browser-side debug bundle generation.
- WASM preview/debug render options now accept a `runtime_assets_base_url` override for embedded browser hosts, and ZIP analysis/rendering can opt into shared top-level prefix stripping for archive wrappers such as repository snapshots.
- `marknest-server` provides a local Axum fallback service that accepts multipart ZIP uploads plus shared output options, returns single PDF or batch ZIP downloads through a Playwright-driven Chromium/Chrome path, and emits structured request logs.
- `index.html` plus `web/app.js`, `web/app.css`, `web/output_options.mjs`, and `web/runtime_sync.mjs` provide a web app for ZIP upload, entry selection, warnings, rendered HTML preview, runtime-aware browser export, debug bundle download, and optional local server fallback.
- Validation supports `--entry`, `--all`, `--strict`, and `--report`.
- Conversion supports `--entry`, `--all`, `--output`, `--out-dir`, `--config`, `--render-report`, `--debug-html`, `--asset-manifest`, `--css`, `--header-template`, `--footer-template`, `--page-size`, `--margin`, `--margin-top`, `--margin-right`, `--margin-bottom`, `--margin-left`, `--theme`, `--landscape`, `--toc`, `--no-toc`, `--sanitize-html`, `--no-sanitize-html`, `--title`, `--author`, `--subject`, `--mermaid`, `--math`, `--mermaid-timeout-ms`, and `--math-timeout-ms`.
- ZIP inputs can convert a selected entry or all entries.
- Explicit folder inputs default to batch conversion and preserve relative paths under `--out-dir`.
- Batch conversion reports output collisions, failed entries, and warning-producing entries.
- Conversion precedence is fixed as `CLI args > config file > environment > built-in defaults` for supported settings such as theme and CSS.
- Conversion reports now include runtime metadata for the renderer, browser path detection, pinned Playwright version, bundled asset mode, exact Mermaid/Math versions, and pinned local runtime asset paths.
- CLI conversion and validation now best-effort fetch remote HTTP images, inline successful results into debug HTML/PDF input, and record `remote_assets` outcomes in JSON artifacts.
- Shared rendered HTML now includes print-layout guards so top-level sections can start on fresh pages and tables/code blocks are less likely to split across PDF pages.
- Shared rendered HTML can now generate a linked in-document table of contents from heading structure when TOC is enabled.
- Mermaid and Math rendering now use shared per-item timeouts with defaults of 5000 ms and 3000 ms, and the CLI/config/environment precedence applies to those timeout settings too.
- Rendered document HTML is sanitized by default across CLI, WASM, and fallback flows, with an explicit opt-out for trusted inputs.
- The web flow still analyzes ZIP bytes and renders preview HTML in-browser, but it now offers a shared output-control model plus a `Browser Fast` or `High Quality Fallback` export mode, with iframe runtime waiting so Mermaid and Math finish consistently before preview/export completes.
- Browser preview now best-effort materializes remote HTTP images before preview/export, and Browser Fast automatically retries through the local fallback server when remote images cannot be materialized in-browser and a fallback URL is configured.
- Official browser, CLI, and fallback-server flows no longer depend on external CDN fetches for Mermaid, MathJax, or html2pdf.js runtime assets.
- Large ZIP uploads trigger guidance that recommends the local fallback server for more reliable export.
- The repository now includes a first-party `Dockerfile` for building the CLI and fallback server in one runtime image.
- Exit codes are stable: `0` success, `1` warning, `2` validation failure, `3` system failure.

## Current layout

- `Cargo.toml`: workspace definition
- `crates/marknest`: validation, conversion, config resolution, and Playwright PDF glue
- `crates/marknest/playwright-runtime`: pinned Node Playwright package for the native/fallback PDF renderer
- `crates/marknest-core`: core analysis and HTML rendering library
- `crates/marknest-server`: local HTTP fallback service plus request tracing
- `crates/marknest-wasm`: WASM bindings for ZIP analysis, output-aware preview HTML rendering, debug bundles, and browser export packaging
- `index.html` and `web/`: static Trunk app shell for browser preview, output controls, remote-image materialization, quality mode selection, and download
- `runtime-assets/`: vendored Mermaid, MathJax, html2pdf.js runtime files plus third-party notices
- `validation/`: pinned 60-entry README corpus manifest, baseline artifacts, and hybrid PDF fidelity validator
- `Dockerfile`: shared CLI and fallback-server runtime image

## Installation

### Cargo

```bash
cargo install marknest
```

### npm / npx

```bash
# Run directly
npx marknest validate README.md

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

For PDF rendering, install the Playwright headless shell:

```bash
npx playwright install chromium
# or for headless-only environments:
npx playwright install --with-deps chromium
```

## Development

Run the formatter:

```bash
cargo fmt --all
```

Run the workspace tests:

```bash
cargo test
```

Check the WASM crate against the browser target:

```bash
cargo check -p marknest-wasm --target wasm32-unknown-unknown
```

Install the pinned Playwright runtime dependency for native PDF rendering:

```bash
npm ci --prefix crates/marknest/playwright-runtime
```

Install the validation-only Node dependencies for corpus diffing:

```bash
npm ci --prefix validation
```

Validate a single file, ZIP, or folder:

```bash
cargo run -p marknest -- validate README.md
cargo run -p marknest -- validate ./docs.zip --entry docs/README.md
cargo run -p marknest -- validate ./docs --all --report ./out/report.json
```

Convert a single entry into a PDF:

```bash
cargo run -p marknest -- convert
cargo run -p marknest -- convert README.md -o README.pdf
cargo run -p marknest -- convert ./docs.zip --entry docs/README.md -o out.pdf
cargo run -p marknest -- convert README.md --page-size letter --margin 24 -o README.pdf
cargo run -p marknest -- convert README.md --margin-top 24 --margin-right 12 --margin-bottom 20 --margin-left 8 -o README.pdf
cargo run -p marknest -- convert README.md --theme github --landscape -o README.pdf
cargo run -p marknest -- convert README.md --toc -o README.pdf
cargo run -p marknest -- convert README.md --no-sanitize-html -o README.pdf
cargo run -p marknest -- convert README.md --title "Guide" --author "Docs Team" --subject "Architecture" -o README.pdf
cargo run -p marknest -- convert README.md --mermaid auto --math auto -o README.pdf
cargo run -p marknest -- convert README.md --css ./pdf.css --header-template ./header.html --footer-template ./footer.html -o README.pdf
cargo run -p marknest -- convert README.md --debug-html ./out/debug.html --asset-manifest ./out/assets.json --render-report ./out/report.json -o README.pdf
cargo run -p marknest -- convert README.md --config ./.marknest.toml -o README.pdf
```

Convert all entries from a folder or ZIP into a mirrored output tree:

```bash
cargo run -p marknest -- convert ./docs --out-dir ./pdf
cargo run -p marknest -- convert ./docs.zip --all --out-dir ./pdf
cargo run -p marknest -- convert ./docs --out-dir ./pdf --render-report ./out/render-report.json
```

Convert directly from a GitHub URL:

```bash
cargo run -p marknest -- convert https://github.com/user/repo -o output.pdf
cargo run -p marknest -- convert https://github.com/user/repo/blob/main/docs/guide.md -o guide.pdf
cargo run -p marknest -- convert https://github.com/user/repo/tree/v2.0 --all --out-dir ./pdf
cargo run -p marknest -- validate https://github.com/user/repo
```

GitHub URL support downloads the repository as a ZIP archive through the GitHub API and processes it through the existing ZIP pipeline. Set `GITHUB_TOKEN` or `GH_TOKEN` for private repositories or to avoid API rate limits.

`convert` requires `node`, `npm ci --prefix crates/marknest/playwright-runtime`, and a local Chrome, Edge, Chromium, or Playwright headless shell installation for Playwright headless PDF generation.
If no supported browser is installed, install a standalone headless shell:

```bash
npx playwright install chromium-headless-shell
```

`--mermaid auto|on` and `--math auto|on` use vendored local Mermaid and MathJax runtime assets; when `--debug-html` is written with those modes enabled, a sibling `runtime-assets/` directory is emitted for offline reproduction.
Supported defaults can come from `.marknest.toml`, `marknest.toml`, `MARKNEST_CONFIG`, `MARKNEST_THEME`, `MARKNEST_CSS`, `MARKNEST_TOC`, and `MARKNEST_SANITIZE_HTML`.
Browser discovery checks `MARKNEST_BROWSER_PATH` first, then Playwright headless shell installations, then common Chrome/Edge/Chromium paths on macOS, Linux, and Windows.
Remote HTTP images are fetched best-effort during `validate` and `convert`; successful fetches are inlined into the rendered HTML, while failures stay as warnings and appear in `remote_assets` sections inside JSON reports and asset manifests.

Run the browser app:

```bash
trunk serve
```

The Trunk app loads `index.html`, compiles `crates/marknest-wasm`, and serves the static browser UI from `web/`.
The web app supports ZIP upload, entry selection, diagnostics, selected-entry PDF download, batch PDF ZIP download, debug bundle download, large-archive guidance, theme/CSS/metadata/per-side margin/print/TOC/sanitization controls, and an optional local fallback service.
`trunk` is not vendored in this repository, so install it separately before running the preview app.
The browser export path now loads `html2pdf.js` from the vendored `runtime-assets/` tree served by Trunk, and browser debug bundles include the Mermaid/Math runtime files referenced by `debug.html`.
External WASM hosts can override the Mermaid/Math/html2pdf runtime asset base with `runtime_assets_base_url`, and `analyzeZipWithOptions({ strip_zip_prefix: true })` plus matching render options make GitHub-style wrapper archives behave like a flat workspace without post-processing ZIP contents first.
A standalone `wasm-example.html` page boots `marknest_wasm` directly, loads a wrapped sample ZIP in memory, exposes `strip_zip_prefix` plus `runtime_assets_base_url` controls for quick browser-side API checks, and can rebuild a browser-local ZIP from a public GitHub repository URL for README rendering tests.
Browser preview and browser PDF export now wait for the rendered iframe to report Mermaid/Math completion through `window.__MARKNEST_RENDER_STATUS__`; runtime warnings remain non-blocking, while runtime errors block Browser Fast export and fall back to the local server when configured.
Browser preview/export also try to materialize remote HTTP images before rendering; if Browser Fast cannot materialize some remote images because of browser fetch restrictions and a fallback URL is configured, export automatically retries through the local fallback server.

Run the local fallback server:

```bash
cargo run -p marknest-server
```

The fallback server listens on `http://127.0.0.1:3476` by default and can be changed with `MARKNEST_SERVER_ADDR`.
When the web app is switched to `High Quality Fallback`, or when browser export fails and the server URL is configured, it sends the ZIP plus the active output controls, including per-side margins, to the local service and downloads the returned PDF or ZIP response.

Build the shared runtime image:

```bash
docker build -t marknest .
```

The image builds `marknest` and `marknest-server` from the same workspace, installs `node`, `chromium`, and base font packages, and copies the pinned Playwright runtime package into the final container.

## README corpus validation

The repository includes a pinned 60-entry public GitHub README corpus in `validation/readme-corpus-60.tsv`.
Each entry is locked to a commit SHA and fetched from a GitHub archive snapshot, not a mutable branch checkout.
The current corpus shape is a 10-repo smoke tier plus 50 extended entries, including 10 math-focused READMEs that stress inline and display LaTeX rendering.

Validation prerequisites:

```bash
git lfs version
pdftoppm -v
pdftotext -v
```

Check the manifest shape and IDs without network access:

```bash
node validation/readme_corpus.mjs verify-manifest --offline
```

Verify pinned repo metadata against the GitHub API:

```bash
node validation/readme_corpus.mjs verify-manifest --tier smoke
```

Fetch the smoke or full corpus into `validation/.cache/readme-corpus-60/`:

```bash
node validation/readme_corpus.mjs fetch --tier smoke
node validation/readme_corpus.mjs fetch --tier all
```

Bless or rerun the corpus against committed baselines:

```bash
node validation/readme_corpus.mjs bless --tier smoke --force
node validation/readme_corpus.mjs run --tier smoke --force
node validation/readme_corpus.mjs run --tier all --force
node validation/readme_corpus_wasm.mjs run --tier all
```

The validator builds `target/debug/marknest` once per invocation, converts each pinned README, rasterizes PDF pages, extracts PDF text, and compares the result to `validation/baselines/readme-corpus-60/`.
The WASM corpus runner builds a `nodejs` `marknest-wasm` package with `wasm-pack`, creates a minimal wrapped ZIP per cached corpus entry, and verifies both bare repo URLs and `/blob/.../README.md` URLs through the browser-oriented `analyzeZipWithOptions` and `renderHtml` bindings.
The manifest is the source of truth for corpus membership; the blessed baseline directory can temporarily contain fewer entries when newly added cases still fail blocking fidelity checks and have not been blessed yet.
Run artifacts land under `validation/.runs/<run-id>/` with per-repo PDFs, page PNGs, diff PNGs, `report.json`, `asset-manifest.json`, `source.json`, `metrics.json`, and top-level `summary.tsv` plus `summary.json`.

Blocking failures:

- conversion exits above `1`
- render report status is `failure` or contains errors
- selected-entry local assets are missing
- any source `H1`/`H2`/`H3` heading is missing from extracted PDF text
- normalized source-text token coverage falls below `0.97`
- a page is visually near-blank and also has no meaningful extracted text

Advisories:

- warning exit code `1`
- baseline page-count deltas
- remote HTTP asset fetch failures
- edge-contact signals that may indicate clipping

The fidelity goal is content preservation, not browser pixel parity.
Print-safe reflow is acceptable when content remains present, such as wrapping long code lines, stacking badge rows, expanding `<details>` content, or paginating wide sections instead of clipping them.
