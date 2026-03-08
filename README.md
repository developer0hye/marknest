# MarkNest

MarkNest is a Rust-first Markdown workspace analyzer and PDF converter for the product described in [PRD.md](./PRD.md).

The repository currently contains Phase 8 of the engineering MVP:

- `marknest-core` analyzes a workspace directory or ZIP archive.
- It returns a reproducible `ProjectIndex` with entry candidates, resolved or missing image assets, ignored files, and path diagnostics.
- `marknest-core` can render a single workspace or ZIP entry into self-contained HTML with local images inlined as data URIs, normalized remote HTTP image metadata, GitHub-style emoji shortcodes in prose, built-in theme presets, custom CSS overrides, and optional Mermaid/Math runtime hooks backed by vendored local runtime assets.
- ZIP analysis blocks path traversal, absolute paths, Windows drive paths, and oversized archives.
- `marknest` provides a `validate` CLI for `.md`, `.zip`, and folder inputs.
- `marknest` provides a conversion CLI with Phase 8 config, debug artifact, and print template support.
- `marknest` now also exposes a reusable HTML-to-PDF helper for local fallback services.
- `marknest-wasm` exposes browser bindings for ZIP analysis, output-aware HTML preview rendering, batch preview rendering, ZIP packaging of generated PDFs, and browser-side debug bundle generation.
- `marknest-server` provides a local Axum fallback service that accepts multipart ZIP uploads plus shared output options, returns single PDF or batch ZIP downloads through a Playwright-driven Chromium/Chrome path, and emits structured request logs.
- `index.html` plus `web/app.js`, `web/app.css`, `web/output_options.mjs`, and `web/runtime_sync.mjs` provide a web app for ZIP upload, entry selection, warnings, rendered HTML preview, runtime-aware browser export, debug bundle download, and optional local server fallback.
- Validation supports `--entry`, `--all`, `--strict`, and `--report`.
- Conversion supports `--entry`, `--all`, `--output`, `--out-dir`, `--config`, `--render-report`, `--debug-html`, `--asset-manifest`, `--css`, `--header-template`, `--footer-template`, `--page-size`, `--margin`, `--margin-top`, `--margin-right`, `--margin-bottom`, `--margin-left`, `--theme`, `--landscape`, `--title`, `--author`, `--subject`, `--mermaid`, and `--math`.
- ZIP inputs can convert a selected entry or all entries.
- Explicit folder inputs default to batch conversion and preserve relative paths under `--out-dir`.
- Batch conversion reports output collisions, failed entries, and warning-producing entries.
- Conversion precedence is fixed as `CLI args > config file > environment > built-in defaults` for supported settings such as theme and CSS.
- Conversion reports now include runtime metadata for the renderer, browser path detection, pinned Playwright version, bundled asset mode, exact Mermaid/Math versions, and pinned local runtime asset paths.
- CLI conversion and validation now best-effort fetch remote HTTP images, inline successful results into debug HTML/PDF input, and record `remote_assets` outcomes in JSON artifacts.
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
- `crates/marknest-wasm`: Phase 8 WASM bindings for ZIP analysis, output-aware preview HTML rendering, debug bundles, and browser export packaging
- `index.html` and `web/`: static Trunk app shell for browser preview, output controls, remote-image materialization, quality mode selection, and download
- `runtime-assets/`: vendored Mermaid, MathJax, html2pdf.js runtime files plus third-party notices
- `Dockerfile`: shared CLI and fallback-server runtime image

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

`convert` requires `node`, `npm ci --prefix crates/marknest/playwright-runtime`, and a local Chrome, Edge, or Chromium installation for Playwright headless PDF generation.
`--mermaid auto|on` and `--math auto|on` use vendored local Mermaid and MathJax runtime assets; when `--debug-html` is written with those modes enabled, a sibling `runtime-assets/` directory is emitted for offline reproduction.
Supported defaults can come from `.marknest.toml`, `marknest.toml`, `MARKNEST_CONFIG`, `MARKNEST_THEME`, and `MARKNEST_CSS`.
Browser discovery checks `MARKNEST_BROWSER_PATH` first, then common Chrome/Edge/Chromium paths on macOS, Linux, and Windows.
Remote HTTP images are fetched best-effort during `validate` and `convert`; successful fetches are inlined into the rendered HTML, while failures stay as warnings and appear in `remote_assets` sections inside JSON reports and asset manifests.

Run the Phase 8 browser app:

```bash
trunk serve
```

The Trunk app loads `index.html`, compiles `crates/marknest-wasm`, and serves the static browser UI from `web/`.
The web app supports ZIP upload, entry selection, diagnostics, selected-entry PDF download, batch PDF ZIP download, debug bundle download, large-archive guidance, theme/CSS/metadata/per-side margin/print controls, and an optional local fallback service.
`trunk` is not vendored in this repository, so install it separately before running the preview app.
The browser export path now loads `html2pdf.js` from the vendored `runtime-assets/` tree served by Trunk, and browser debug bundles include the Mermaid/Math runtime files referenced by `debug.html`.
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
